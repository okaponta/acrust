//! A cookie store whose contents can be read back out.
//!
//! `reqwest::cookie::Jar` never hands back the cookies it holds, so it cannot be
//! used to pick up `REVEL_SESSION` after login and persist it. acrust only ever
//! talks to atcoder.jp, so a flat name-to-value map is enough.

use reqwest::header::HeaderValue;
use std::collections::BTreeMap;
use std::sync::Mutex;

#[derive(Debug, Default)]
pub struct CookieStore {
    jar: Mutex<BTreeMap<String, String>>,
}

impl CookieStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, name: &str, value: &str) {
        self.jar
            .lock()
            .expect("cookie jar is not poisoned")
            .insert(name.to_owned(), value.to_owned());
    }

    pub fn get(&self, name: &str) -> Option<String> {
        self.jar
            .lock()
            .expect("cookie jar is not poisoned")
            .get(name)
            .cloned()
    }

    pub fn clear(&self) {
        self.jar.lock().expect("cookie jar is not poisoned").clear();
    }
}

/// Takes `name=value` out of a `Set-Cookie` header and drops the attributes.
///
/// One domain and one session leaves nothing to do with `Path` or `Expires`. A
/// deletion (`Max-Age=0`, or an `Expires` in the past) arrives with an empty
/// value, so an empty value is reported as a removal.
fn parse_set_cookie(header: &str) -> Option<(String, Option<String>)> {
    let pair = header.split(';').next()?.trim();
    let (name, value) = pair.split_once('=')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let value = value.trim();
    if value.is_empty() {
        Some((name.to_owned(), None))
    } else {
        Some((name.to_owned(), Some(value.to_owned())))
    }
}

impl reqwest::cookie::CookieStore for CookieStore {
    fn set_cookies(
        &self,
        cookie_headers: &mut dyn Iterator<Item = &HeaderValue>,
        _url: &reqwest::Url,
    ) {
        let mut jar = self.jar.lock().expect("cookie jar is not poisoned");
        for header in cookie_headers {
            let Ok(header) = header.to_str() else {
                continue;
            };
            let Some((name, value)) = parse_set_cookie(header) else {
                continue;
            };
            match value {
                Some(value) => {
                    jar.insert(name, value);
                }
                None => {
                    jar.remove(&name);
                }
            }
        }
    }

    fn cookies(&self, _url: &reqwest::Url) -> Option<HeaderValue> {
        let jar = self.jar.lock().expect("cookie jar is not poisoned");
        if jar.is_empty() {
            return None;
        }
        let joined = jar
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ");
        HeaderValue::from_str(&joined).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::cookie::CookieStore as _;

    fn url() -> reqwest::Url {
        "https://atcoder.jp/login".parse().unwrap()
    }

    fn headers(values: &[&str]) -> Vec<HeaderValue> {
        values
            .iter()
            .map(|v| HeaderValue::from_str(v).unwrap())
            .collect()
    }

    #[test]
    fn keeps_the_value_and_drops_the_attributes() {
        let store = CookieStore::new();
        let headers = headers(&[
            "REVEL_SESSION=abc%3Ddef; Path=/; Expires=Mon, 08 Feb 2027 00:00:00 GMT; HttpOnly",
        ]);
        store.set_cookies(&mut headers.iter(), &url());
        assert_eq!(store.get("REVEL_SESSION").as_deref(), Some("abc%3Ddef"));
        assert_eq!(
            store.cookies(&url()).unwrap().to_str().unwrap(),
            "REVEL_SESSION=abc%3Ddef"
        );
    }

    #[test]
    fn an_empty_value_deletes_the_cookie() {
        let store = CookieStore::new();
        store.set("REVEL_SESSION", "abc");
        let headers = headers(&["REVEL_SESSION=; Max-Age=0; Path=/"]);
        store.set_cookies(&mut headers.iter(), &url());
        assert_eq!(store.get("REVEL_SESSION"), None);
        assert!(store.cookies(&url()).is_none());
    }

    #[test]
    fn later_headers_win() {
        let store = CookieStore::new();
        let headers = headers(&["REVEL_SESSION=old; Path=/", "REVEL_SESSION=new; Path=/"]);
        store.set_cookies(&mut headers.iter(), &url());
        assert_eq!(store.get("REVEL_SESSION").as_deref(), Some("new"));
    }
}
