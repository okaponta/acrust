//! AtCoder への HTTP アクセス。
//!
//! AtCoder は短時間に数リクエストで実際に 429 を返してくる（設計 §3.6）。
//! そのためこのクライアントは必ず
//!
//! - リクエスト間隔の下限（既定 1 秒）
//! - 429 / 5xx の指数バックオフ（`Retry-After` があればそれに従う）
//! - 素性を明示する User-Agent
//!
//! を通す。リダイレクトは追わない。ログインの成否も提出の成否も
//! 「どこへリダイレクトされたか」で判定するため、こちらで制御したほうが確実で、
//! 中間レスポンスの `Set-Cookie` も拾える。

use crate::atcoder::cookies::CookieStore;
use crate::config::AtcoderConfig;
use crate::session::{self, Session, COOKIE_NAME};
use anyhow::{bail, Context as _, Result};
use reqwest::blocking::{Client, RequestBuilder, Response};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT_LANGUAGE, LOCATION, RETRY_AFTER};
use reqwest::{Method, StatusCode};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const BASE_URL: &str = "https://atcoder.jp";

/// バックオフの初期値。以降 2 倍ずつ。
const BACKOFF_BASE: Duration = Duration::from_secs(1);
/// `Retry-After` が非常識に長い場合の上限。
const MAX_RETRY_AFTER: Duration = Duration::from_secs(60);

pub struct AtCoderClient {
    http: Client,
    cookies: Arc<CookieStore>,
    interval: Duration,
    retry: u32,
    last_request: std::cell::Cell<Option<Instant>>,
}

impl AtCoderClient {
    pub fn new(config: &AtcoderConfig) -> Result<Self> {
        let cookies = Arc::new(CookieStore::new());
        let mut headers = HeaderMap::new();
        // 問題文は日本語ページを主に見る（設計 §3.3）。
        headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("ja,en;q=0.8"));

        let http = Client::builder()
            .user_agent(config.resolved_user_agent())
            .default_headers(headers)
            .cookie_provider(Arc::clone(&cookies))
            // リダイレクトは自分で追う（Set-Cookie と Location を見たいため）。
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(30))
            .build()
            .context("could not build the HTTP client")?;

        Ok(Self {
            http,
            cookies,
            interval: Duration::from_millis(config.request_interval_ms),
            retry: config.retry,
            last_request: std::cell::Cell::new(None),
        })
    }

    /// 保存済みセッションがあれば読み込む。戻り値はログイン済みらしいかどうか。
    pub fn load_session(&self) -> Result<bool> {
        match session::load()? {
            Some(saved) => {
                self.cookies.set(COOKIE_NAME, &saved.revel_session);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    #[allow(dead_code)]
    pub fn set_session_cookie(&self, value: &str) {
        self.cookies.set(COOKIE_NAME, value);
    }

    /// 現在のセッションクッキー。ログイン直後の永続化に使う。
    pub fn session_cookie(&self) -> Option<String> {
        self.cookies.get(COOKIE_NAME)
    }

    pub fn clear_cookies(&self) {
        self.cookies.clear();
    }

    /// 現在のクッキーをそのまま `Session` にする。
    pub fn session(&self, user_screen_name: &str) -> Option<Session> {
        self.session_cookie()
            .map(|cookie| Session::new(cookie, user_screen_name.to_owned()))
    }

    pub fn get(&self, url: &str) -> Result<AtCoderResponse> {
        self.send(self.http.request(Method::GET, url), url)
    }

    /// `application/x-www-form-urlencoded` の POST。`Referer` は呼び出し側で付ける。
    pub fn post_form(
        &self,
        url: &str,
        form: &[(&str, &str)],
        referer: &str,
    ) -> Result<AtCoderResponse> {
        let request = self
            .http
            .request(Method::POST, url)
            .header(reqwest::header::REFERER, referer)
            .form(form);
        self.send(request, url)
    }

    fn send(&self, request: RequestBuilder, url: &str) -> Result<AtCoderResponse> {
        let mut attempt = 0;
        loop {
            self.wait_for_slot();
            let cloned = request.try_clone().context("could not clone the request")?;
            let result = cloned.send();
            self.last_request.set(Some(Instant::now()));

            let response = match result {
                Ok(response) => response,
                Err(e) => {
                    if attempt >= self.retry {
                        return Err(e).with_context(|| format!("the request to {url} failed"));
                    }
                    attempt += 1;
                    std::thread::sleep(backoff(attempt));
                    continue;
                }
            };

            if should_retry(response.status()) && attempt < self.retry {
                attempt += 1;
                let wait = retry_after(&response).unwrap_or_else(|| backoff(attempt));
                crate::ui::warn(&format!(
                    "got {} from {url}. Waiting {:.1}s and retrying ({attempt}/{})",
                    response.status(),
                    wait.as_secs_f64(),
                    self.retry
                ));
                std::thread::sleep(wait);
                continue;
            }

            return AtCoderResponse::from_response(response, url);
        }
    }

    /// 直前のリクエストから `interval` 経つまで待つ。
    fn wait_for_slot(&self) {
        if let Some(last) = self.last_request.get() {
            let elapsed = last.elapsed();
            if elapsed < self.interval {
                std::thread::sleep(self.interval - elapsed);
            }
        }
    }
}

fn should_retry(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn backoff(attempt: u32) -> Duration {
    BACKOFF_BASE * 2u32.saturating_pow(attempt.saturating_sub(1))
}

/// `Retry-After` は秒数形式だけ解釈する。AtCoder は HTTP-date 形式を返さない。
fn retry_after(response: &Response) -> Option<Duration> {
    let value = response.headers().get(RETRY_AFTER)?.to_str().ok()?;
    let secs: u64 = value.trim().parse().ok()?;
    Some(Duration::from_secs(secs).min(MAX_RETRY_AFTER))
}

/// レスポンスのうち acrust が使う部分だけを取り出したもの。
pub struct AtCoderResponse {
    pub url: String,
    pub status: StatusCode,
    /// `Location` ヘッダ（絶対 URL に直したもの）。
    pub location: Option<String>,
    pub body: String,
}

impl AtCoderResponse {
    fn from_response(response: Response, url: &str) -> Result<Self> {
        let status = response.status();
        let location = response
            .headers()
            .get(LOCATION)
            .and_then(|v| v.to_str().ok())
            .map(absolutize);
        let body = response
            .text()
            .with_context(|| format!("could not read the response from {url}"))?;
        Ok(Self {
            url: url.to_owned(),
            status,
            location,
            body,
        })
    }

    pub fn is_redirect(&self) -> bool {
        self.status.is_redirection()
    }

    /// 2xx でなければエラーにする。404 は「まだ存在しない」の意味を持つので呼び出し側で分岐する。
    pub fn error_for_status(&self) -> Result<()> {
        if self.status.is_success() {
            return Ok(());
        }
        let alerts = crate::atcoder::html::alerts(&self.body);
        if let Some(first) = alerts.first() {
            bail!("{} returned {}: {first}", self.url, self.status);
        }
        bail!("{} returned {}", self.url, self.status);
    }
}

fn absolutize(location: &str) -> String {
    if location.starts_with("http://") || location.starts_with("https://") {
        location.to_owned()
    } else {
        format!("{BASE_URL}{location}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_from_one_second() {
        assert_eq!(backoff(1), Duration::from_secs(1));
        assert_eq!(backoff(2), Duration::from_secs(2));
        assert_eq!(backoff(3), Duration::from_secs(4));
    }

    #[test]
    fn retries_on_429_and_5xx_only() {
        assert!(should_retry(StatusCode::TOO_MANY_REQUESTS));
        assert!(should_retry(StatusCode::INTERNAL_SERVER_ERROR));
        assert!(should_retry(StatusCode::SERVICE_UNAVAILABLE));
        assert!(!should_retry(StatusCode::OK));
        assert!(!should_retry(StatusCode::NOT_FOUND));
        assert!(!should_retry(StatusCode::FOUND));
    }

    #[test]
    fn relative_locations_become_absolute() {
        assert_eq!(
            absolutize("/contests/abc474/submissions/me"),
            "https://atcoder.jp/contests/abc474/submissions/me"
        );
        assert_eq!(
            absolutize("https://atcoder.jp/home"),
            "https://atcoder.jp/home"
        );
    }
}
