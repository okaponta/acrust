//! AtCoder を実際に叩くテスト。
//!
//! `live` feature が付いていないとビルドもされない（CI では走らない）。
//! `#[ignore]` ではなく feature にしているのは、普段の `cargo test` の結果に
//! "ignored" として並ばないようにするため。
//!
//! 手元で確認するとき:
//!
//! ```console
//! $ cargo test --features live --test network -- --test-threads=1
//! ```
//!
//! リクエスト間隔はクライアント側で 1 秒以上空く（設計 §3.6）。

use acrust::atcoder::{auth, html, AtCoderClient};
use acrust::config::AtcoderConfig;

#[test]
fn the_login_page_still_exposes_the_csrf_token_and_the_screen_name() {
    let client = AtCoderClient::new(&AtcoderConfig::default()).unwrap();
    let response = client.get(auth::LOGIN_URL).unwrap();
    response.error_for_status().unwrap();

    let token = html::csrf_token(&response.body)
        .expect("var csrfToken / hidden input のどちらかから取れること");
    assert!(!token.is_empty());
    // 未ログインなら userScreenName は空。
    assert_eq!(html::user_screen_name(&response.body), None);
}

#[test]
fn an_anonymous_client_is_not_logged_in() {
    let client = AtCoderClient::new(&AtcoderConfig::default()).unwrap();
    assert_eq!(auth::current_user(&client).unwrap(), None);
}

/// ID / パスワードでのログインを塞いでいる当の Turnstile が、まだそこにあるか。
///
/// これが落ちたら AtCoder が CAPTCHA を外したということなので、
/// ID / パスワードでのログインを復活させられる（`atcoder::auth` 参照）。
#[test]
fn the_login_form_is_still_behind_a_captcha() {
    let client = AtCoderClient::new(&AtcoderConfig::default()).unwrap();
    let response = client.get(auth::LOGIN_URL).unwrap();
    response.error_for_status().unwrap();
    assert!(
        auth::has_captcha(&response.body),
        "Turnstile が消えている。ID / パスワードでのログインを検討できる"
    );
}
