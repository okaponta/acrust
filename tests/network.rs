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

/// **提出フォームにも Turnstile が入っている**（2026-09-12 に実地確認）。
///
/// これがある限り、素の POST での提出は csrf_token が正しくても
/// 「エラーが発生しました。」で弾かれる。`/login` と同じ sitekey・同じ塞がれ方で、
/// acrust は CAPTCHA を迂回しないので `submit` はブラウザに渡す形になる。
///
/// このテストが**落ちたら** AtCoder が提出から CAPTCHA を外したということなので、
/// `acrust submit` の自動提出を復活できる。そのための見張り。
///
/// セッションが要る（`ACRUST_SESSION_FILE` か既定の保存先）。未ログインなら飛ばす。
#[test]
fn the_submit_form_is_still_behind_a_captcha() {
    let client = AtCoderClient::new(&AtcoderConfig::default()).unwrap();
    if !client.load_session().unwrap() {
        eprintln!("skip: ログインしていないので提出フォームを見られません");
        return;
    }
    let response = client
        .get("https://atcoder.jp/contests/practice/submit")
        .unwrap();
    response.error_for_status().unwrap();
    if html::user_screen_name(&response.body).is_none() {
        eprintln!("skip: セッションが無効です");
        return;
    }
    assert!(
        response.body.contains("form-code-submit"),
        "提出フォームが見つからない"
    );
    assert!(
        auth::has_captcha(&response.body),
        "提出フォームから Turnstile が消えている。自動提出を復活できるか検討する"
    );
}
