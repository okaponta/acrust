//! Login state, and checking a session cookie against the live site.
//!
//! Whether we are logged in is read off `var userScreenName`, which is cheaper
//! than POSTing to `/contests/agc001/submit` and looking at the status, the way
//! online-judge-tools does it.
//!
//! # Why there is no id / password login
//!
//! AtCoder put Cloudflare Turnstile in front of `/login` in March 2025
//! (`<div class="cf-challenge" data-sitekey="...">`). The widget is what fills in
//! the hidden `cf-turnstile-response` field, so a plain POST is turned away with
//! "エラーが発生しました。" however correct its csrf_token — confirmed against the
//! live site on 2026-09-10.
//!
//! Working around a CAPTCHA is the wrong thing to build, so acrust takes the
//! session cookie the browser already holds instead.

use crate::atcoder::client::{AtCoderClient, BASE_URL};
use crate::atcoder::html;
use anyhow::{bail, Result};

pub const LOGIN_URL: &str = "https://atcoder.jp/login";
/// The page login state is read from. Not Disallowed by robots.txt.
pub const HOME_URL: &str = "https://atcoder.jp/home";

/// `Some(user name)` when logged in.
///
/// `/home` answers 200 to anonymous requests too, so the status line proves
/// nothing; only the contents of `userScreenName` do.
pub fn current_user(client: &AtCoderClient) -> Result<Option<String>> {
    let response = client.get(HOME_URL)?;
    if response.is_redirect() {
        return Ok(None);
    }
    response.error_for_status()?;
    Ok(html::user_screen_name(&response.body))
}

pub fn verify_session_cookie(client: &AtCoderClient, cookie: &str) -> Result<String> {
    client.set_session_cookie(cookie);
    match current_user(client)? {
        Some(user) => Ok(user),
        None => bail!(
            "that session cookie does not log in to AtCoder. \
             It may have expired, or the copy may be cut short"
        ),
    }
}

/// Whether the login page still carries the Turnstile widget. If AtCoder ever
/// drops it, id / password login can come back.
pub fn has_captcha(login_page: &str) -> bool {
    login_page.contains("cf-challenge") || login_page.contains("turnstile")
}

/// Digs the session value out of whatever was pasted.
///
/// A DevTools copy comes out as `REVEL_SESSION=xxx`, as a bare `xxx`, or as an
/// entire Cookie header, depending on where the user clicked. All three are fine.
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
        // the value on its own
        assert_eq!(
            extract_session_value("abc123%00def"),
            Some("abc123%00def".to_owned())
        );
        // name=value
        assert_eq!(
            extract_session_value("REVEL_SESSION=abc123"),
            Some("abc123".to_owned())
        );
        // an entire Cookie header
        assert_eq!(
            extract_session_value("REVEL_FLASH=; REVEL_SESSION=abc123; Path=/"),
            Some("abc123".to_owned())
        );
        // surrounding space and quotes
        assert_eq!(
            extract_session_value("  \"abc123\"  "),
            Some("abc123".to_owned())
        );
        // nothing usable
        assert_eq!(extract_session_value("   "), None);
        assert_eq!(extract_session_value("REVEL_SESSION="), None);
    }
}
