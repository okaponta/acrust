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

/// **終了したコンテストの提出フォームには Turnstile が入っている**（2026-09-12 に実地確認）。
///
/// AtCoder は 2025-03 に Cloudflare Turnstile を入れ、**コンテスト終了後**の提出にも
/// これを出すようになった。`/login` と同じ sitekey で、隠しフィールド
/// `cf-turnstile-response` はブラウザ上の JS が差し込むため、素の POST は
/// csrf_token が正しくても「エラーが発生しました。」で弾かれる。
///
/// **開催中の提出はコマンドから通る**ので、`acrust submit` は現役のまま。
/// ここで見張っているのは「終了後も塞がれたままか」だけ。
///
/// `practice` は常設なので常に「終了後」と同じ扱いになる。これが**落ちたら**
/// AtCoder が終了後の提出から CAPTCHA を外したということ。
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
        "終了後の提出フォームから Turnstile が消えている。README の注意書きを見直す"
    );
}
