//! AtCoder の HTML から必要な値だけを取り出す。
//!
//! AtCoder は全ページの `<script>` に `csrfToken` と `userScreenName` を埋めている（設計 §3.1）。
//! ログイン状態の判定もこれで足りるので、`/contests/*/submit` を叩いて 200 かどうかを見る
//! （`online-judge-tools` の方式）より軽く、robots.txt 的にも安全。

use regex::Regex;
use std::sync::OnceLock;

fn csrf_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"var\s+csrfToken\s*=\s*"([^"]+)""#).expect("valid regex"))
}

fn screen_name_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"var\s+userScreenName\s*=\s*"([^"]*)""#).expect("valid regex"))
}

fn hidden_csrf_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"<input[^>]*name="csrf_token"[^>]*value="([^"]*)""#).expect("valid regex")
    })
}

fn alert_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?s)<div[^>]*role="alert"[^>]*>(.*?)</div>"#).expect("valid regex")
    })
}

fn tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)<[^>]*>").expect("valid regex"))
}

/// `var csrfToken = "..."`。無ければ hidden input からも探す。
pub fn csrf_token(html: &str) -> Option<String> {
    csrf_re()
        .captures(html)
        .or_else(|| hidden_csrf_re().captures(html))
        .map(|c| c[1].to_owned())
}

/// ログインしていれば `Some(ユーザー名)`、していなければ `None`。
///
/// `var userScreenName` 自体が無いページ（AtCoder 以外など）でも `None` を返す。
pub fn user_screen_name(html: &str) -> Option<String> {
    let name = screen_name_re().captures(html)?[1].to_owned();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// `<div role="alert">` の中身をプレーンテキストにして返す。ログイン失敗の理由などが入る。
pub fn alerts(html: &str) -> Vec<String> {
    alert_re()
        .captures_iter(html)
        .map(|c| strip_dismiss_button(&to_text(&c[1])))
        .filter(|s| !s.is_empty())
        .collect()
}

/// 閉じるボタンの `×` は本文ではないので落とす。
fn strip_dismiss_button(text: &str) -> String {
    text.trim_start_matches(['\u{d7}', ' ']).trim().to_owned()
}

/// タグを剥がし、実体参照を戻し、空白を1つに畳む。
pub fn to_text(html: &str) -> String {
    let stripped = tag_re().replace_all(html, " ");
    let decoded = decode_entities(&stripped);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// HTML の実体参照を戻す。`<pre>` の中身の復元にも使う（サンプルケースの取得で必要）。
pub fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        // `&` から最大 12 バイト以内に `;` があるものだけ実体参照とみなす。
        let end = rest
            .char_indices()
            .take_while(|(i, _)| *i <= 12)
            .find(|(_, c)| *c == ';')
            .map(|(i, _)| i);
        match end.and_then(|end| decode_one(&rest[1..end]).map(|c| (c, end))) {
            Some((decoded, end)) => {
                out.push_str(&decoded);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn decode_one(name: &str) -> Option<String> {
    let named = match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        "times" => Some('\u{d7}'),
        "hellip" => Some('\u{2026}'),
        "mdash" => Some('\u{2014}'),
        "ndash" => Some('\u{2013}'),
        _ => None,
    };
    if let Some(c) = named {
        return Some(c.to_string());
    }
    let digits = name.strip_prefix('#')?;
    let code = match digits.strip_prefix(['x', 'X']) {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => digits.parse::<u32>().ok()?,
    };
    char::from_u32(code).map(|c| c.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 実物の `/login` と同じ形（設計 §3.1 で確認した構造）。問題文は一切含めない。
    const LOGIN_PAGE: &str = r#"
<!DOCTYPE html><html><head><script>
    var LANG = "ja";
    var userScreenName = "";
    var csrfToken = "mY2yi3ntJ5L3YuUmDEk2Y9SGDlCav0Fxaemu5h6VtxY="
</script></head><body>
<form class="form-horizontal" action="" method="POST">
  <input type="hidden" name="csrf_token" value="mY2yi3ntJ5L3YuUmDEk2Y9SGDlCav0Fxaemu5h6VtxY=" />
  <input type="text" name="username" />
  <input type="password" name="password" />
</form>
</body></html>
"#;

    const LOGGED_IN_PAGE: &str = r#"<script>
    var userScreenName = "okaponta";
    var csrfToken = "abc+def/ghi="
    var contestScreenName = "abc474";
</script>"#;

    #[test]
    fn reads_the_csrf_token_from_the_script_block() {
        assert_eq!(
            csrf_token(LOGIN_PAGE).as_deref(),
            Some("mY2yi3ntJ5L3YuUmDEk2Y9SGDlCav0Fxaemu5h6VtxY=")
        );
    }

    #[test]
    fn falls_back_to_the_hidden_input_when_the_script_is_absent() {
        let html = r#"<input type="hidden" name="csrf_token" value="fallback=" />"#;
        assert_eq!(csrf_token(html).as_deref(), Some("fallback="));
    }

    #[test]
    fn an_empty_screen_name_means_logged_out() {
        assert_eq!(user_screen_name(LOGIN_PAGE), None);
        assert_eq!(
            user_screen_name(LOGGED_IN_PAGE).as_deref(),
            Some("okaponta")
        );
        assert_eq!(user_screen_name("<html></html>"), None);
    }

    #[test]
    fn alerts_come_back_as_plain_text() {
        let html = r#"<div class="alert alert-danger" role="alert">
            <button type="button" class="close">&times;</button>
            Username or Password is incorrect.
        </div>"#;
        assert_eq!(alerts(html), ["Username or Password is incorrect."]);
    }

    #[test]
    fn decodes_named_and_numeric_entities() {
        assert_eq!(decode_entities("I&#39;m a teapot"), "I'm a teapot");
        assert_eq!(
            decode_entities("a &lt; b &amp;&amp; c &gt; d"),
            "a < b && c > d"
        );
        assert_eq!(decode_entities("&quot;x&quot;"), "\"x\"");
        assert_eq!(decode_entities("&#x3042;"), "あ");
        // 実体参照でない `&` はそのまま残す（サンプル入力が壊れないこと）。
        assert_eq!(
            decode_entities("1 & 2 &notanentity 3"),
            "1 & 2 &notanentity 3"
        );
        assert_eq!(decode_entities("no entities here"), "no entities here");
    }
}
