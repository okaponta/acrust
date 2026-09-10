//! ログイン状態の確認と、セッションクッキーの検証（設計 §3.1）。
//!
//! 判定は `var userScreenName` が空かどうかで行う。
//! `/contests/agc001/submit` を叩いて 200 かを見る方式（online-judge-tools）より軽い。
//!
//! # ID / パスワードでログインできない理由
//!
//! AtCoder の `/login` フォームには Cloudflare Turnstile が入っている
//! （`<div class="cf-challenge" data-sitekey="...">`）。ブラウザ上の Turnstile が
//! 隠しフィールド `cf-turnstile-response` をフォームに差し込む仕組みで、
//! これが無い POST は csrf_token が正しくても
//! 「エラーが発生しました。」で弾かれる（2026-09-10 に実地確認）。
//!
//! CAPTCHA を迂回するのは筋が悪いので、acrust は
//! **ブラウザで取得済みのセッションクッキーを取り込む**方式を採る。

use crate::atcoder::client::{AtCoderClient, BASE_URL};
use crate::atcoder::html;
use anyhow::{bail, Result};

pub const LOGIN_URL: &str = "https://atcoder.jp/login";
/// ログイン状態の確認に使うページ。robots.txt で Disallow されていない。
pub const HOME_URL: &str = "https://atcoder.jp/home";

/// ログイン済みなら `Some(ユーザー名)`。
///
/// `/home` は未ログインでも 200 を返すので、ステータスではなく
/// `userScreenName` の中身で判定する。
pub fn current_user(client: &AtCoderClient) -> Result<Option<String>> {
    let response = client.get(HOME_URL)?;
    if response.is_redirect() {
        return Ok(None);
    }
    response.error_for_status()?;
    Ok(html::user_screen_name(&response.body))
}

/// 渡されたセッションクッキーが本当に使えるかを AtCoder に確かめ、ユーザー名を返す。
pub fn verify_session_cookie(client: &AtCoderClient, cookie: &str) -> Result<String> {
    client.set_session_cookie(cookie);
    match current_user(client)? {
        Some(user) => Ok(user),
        None => bail!(
            "このセッションクッキーでは AtCoder にログインできませんでした。\
             期限切れか、コピーが途中で切れている可能性があります"
        ),
    }
}

/// ログインページに Turnstile（CAPTCHA）があるか。
///
/// AtCoder が将来これを外したら ID / パスワードでのログインを復活できるので、
/// 判定できるようにしておく。
pub fn has_captcha(login_page: &str) -> bool {
    login_page.contains("cf-challenge") || login_page.contains("turnstile")
}

/// 貼り付けられた文字列からセッションクッキーの値を取り出す。
///
/// DevTools からのコピーは `REVEL_SESSION=xxx` の形にも `xxx` だけの形にもなるし、
/// Cookie ヘッダを丸ごと貼られることもある。どれも受け付ける。
pub fn extract_session_value(pasted: &str) -> Option<String> {
    let pasted = pasted.trim();
    if pasted.is_empty() {
        return None;
    }
    let value = match pasted.find("REVEL_SESSION=") {
        Some(start) => {
            let rest = &pasted[start + "REVEL_SESSION=".len()..];
            rest.split(';').next().unwrap_or(rest)
        }
        None => pasted,
    };
    let value = value.trim().trim_matches(['"', '\'']).trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

/// `/home` などの URL を組み立てる。
#[allow(dead_code)]
pub fn url(path: &str) -> String {
    format!("{BASE_URL}{path}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_built_from_the_single_base() {
        assert_eq!(
            url("/contests/abc474/tasks"),
            "https://atcoder.jp/contests/abc474/tasks"
        );
        assert_eq!(LOGIN_URL, url("/login"));
        assert_eq!(HOME_URL, url("/home"));
    }

    #[test]
    fn detects_the_turnstile_widget() {
        let with = r#"<div class="cf-challenge" data-sitekey="0x4AAAAAAA6HJUmmLP7mLxx0"></div>"#;
        assert!(has_captcha(with));
        assert!(!has_captcha("<form action=\"\" method=\"POST\"></form>"));
    }

    #[test]
    fn accepts_every_shape_the_devtools_copy_produces() {
        // 値だけ
        assert_eq!(
            extract_session_value("abc123%00def"),
            Some("abc123%00def".to_owned())
        );
        // name=value
        assert_eq!(
            extract_session_value("REVEL_SESSION=abc123"),
            Some("abc123".to_owned())
        );
        // Cookie ヘッダ丸ごと
        assert_eq!(
            extract_session_value("REVEL_FLASH=; REVEL_SESSION=abc123; Path=/"),
            Some("abc123".to_owned())
        );
        // 前後の空白と引用符
        assert_eq!(
            extract_session_value("  \"abc123\"  "),
            Some("abc123".to_owned())
        );
        // 空
        assert_eq!(extract_session_value("   "), None);
        assert_eq!(extract_session_value("REVEL_SESSION="), None);
    }
}
