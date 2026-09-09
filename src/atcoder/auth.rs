//! ログインと、ログイン状態の確認（設計 §3.1）。
//!
//! 成否の判定は `var userScreenName` が空かどうかで行う。
//! `/contests/agc001/submit` を叩いて 200 かを見る方式（online-judge-tools）より軽い。

use crate::atcoder::client::{AtCoderClient, BASE_URL};
use crate::atcoder::html;
use anyhow::{bail, Context as _, Result};

pub const LOGIN_URL: &str = "https://atcoder.jp/login";
/// ログイン状態の確認に使うページ。robots.txt で Disallow されていない。
pub const HOME_URL: &str = "https://atcoder.jp/home";

/// ログイン済みなら `Some(ユーザー名)`。
pub fn current_user(client: &AtCoderClient) -> Result<Option<String>> {
    let response = client.get(HOME_URL)?;
    // 未ログインだと /login へリダイレクトされる。
    if response.is_redirect() {
        return Ok(None);
    }
    response.error_for_status()?;
    Ok(html::user_screen_name(&response.body))
}

/// ID / パスワードでログインし、セッションクッキーを持つクライアントにする。
///
/// 成功したらユーザー名を返す。パスワードはどこにも保存しない。
pub fn login(client: &AtCoderClient, username: &str, password: &str) -> Result<String> {
    let page = client.get(LOGIN_URL)?;
    page.error_for_status()?;
    let csrf_token = html::csrf_token(&page.body).context(
        "ログインページから csrf_token を取り出せませんでした。\
         AtCoder の HTML が変わった可能性があります",
    )?;

    let response = client.post_form(
        LOGIN_URL,
        &[
            ("csrf_token", csrf_token.as_str()),
            ("username", username),
            ("password", password),
        ],
        LOGIN_URL,
    )?;

    // 成功・失敗のどちらでもリダイレクトが返る。行き先で区別できる（設計 §3.1）。
    let location = match &response.location {
        Some(location) => location.clone(),
        None => {
            response.error_for_status()?;
            // リダイレクトが無いのは想定外。alert があればそれを見せる。
            if let Some(alert) = html::alerts(&response.body).first() {
                bail!("ログインに失敗しました: {alert}");
            }
            bail!("ログインに失敗しました（リダイレクトが返りませんでした）");
        }
    };

    // リダイレクト先を1回だけ辿り、`userScreenName` で最終判定する。
    let landed = client.get(&location)?;
    if let Some(name) = html::user_screen_name(&landed.body) {
        return Ok(name);
    }

    let alerts = html::alerts(&landed.body);
    match alerts.first() {
        Some(alert) => bail!("ログインに失敗しました: {alert}"),
        None if location.contains("/login") => {
            bail!("ログインに失敗しました。ユーザー名かパスワードが違う可能性があります")
        }
        None => bail!("ログインに失敗しました（{location} に飛ばされました）"),
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
}
