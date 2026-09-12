//! Picking the few values acrust needs out of AtCoder's HTML.
//!
//! Every page carries `csrfToken` and `userScreenName` in a `<script>` block, so
//! a single GET answers both "who am I" and "what token do I post with".

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

/// Reads `var csrfToken = "..."`, falling back to the form's hidden input on the
/// pages that do not carry the script block.
pub fn csrf_token(html: &str) -> Option<String> {
    csrf_re()
        .captures(html)
        .or_else(|| hidden_csrf_re().captures(html))
        .map(|c| c[1].to_owned())
}

/// `Some(user name)` when logged in. A page without `var userScreenName` at all
/// (anything that is not AtCoder) reads as logged out rather than as an error.
pub fn user_screen_name(html: &str) -> Option<String> {
    let name = screen_name_re().captures(html)?[1].to_owned();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// The `<div role="alert">` banners as plain text. This is where AtCoder puts the
/// reason a request was turned away.
pub fn alerts(html: &str) -> Vec<String> {
    alert_re()
        .captures_iter(html)
        .map(|c| strip_dismiss_button(&to_text(&c[1])))
        .filter(|s| !s.is_empty())
        .collect()
}

/// The `×` belongs to the dismiss button, not to the message.
fn strip_dismiss_button(text: &str) -> String {
    text.trim_start_matches(['\u{d7}', ' ']).trim().to_owned()
}

pub fn to_text(html: &str) -> String {
    let stripped = tag_re().replace_all(html, " ");
    let decoded = decode_entities(&stripped);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Undoes HTML entities. Also used on `<pre>` contents, where getting this wrong
/// corrupts the sample cases themselves.
pub fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        // Only a `;` within 12 bytes counts: sample inputs are full of bare `&`,
        // and an unbounded search would swallow everything up to the next one.
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

    /// Shaped like the real `/login`. Carries no problem text.
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
        // A bare `&` survives untouched, or sample inputs would be corrupted.
        assert_eq!(
            decode_entities("1 & 2 &notanentity 3"),
            "1 & 2 &notanentity 3"
        );
        assert_eq!(decode_entities("no entities here"), "no entities here");
    }
}
