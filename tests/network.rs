//! Tests that talk to the live AtCoder.
//!
//! Without the `live` feature they are not even compiled, so CI never runs them.
//! A feature rather than `#[ignore]`, so an ordinary `cargo test` does not report
//! a row of ignored tests that nobody intends to run.
//!
//! To run them:
//!
//! ```console
//! $ cargo test --features live --test network -- --test-threads=1
//! ```
//!
//! The client spaces the requests a second apart on its own.

use acrust::atcoder::{auth, html, AtCoderClient};
use acrust::config::AtcoderConfig;

#[test]
fn the_login_page_still_exposes_the_csrf_token_and_the_screen_name() {
    let client = AtCoderClient::new(&AtcoderConfig::default()).unwrap();
    let response = client.get(auth::LOGIN_URL).unwrap();
    response.error_for_status().unwrap();

    let token =
        html::csrf_token(&response.body).expect("from var csrfToken or from the hidden input");
    assert!(!token.is_empty());
    // Logged out, userScreenName is empty.
    assert_eq!(html::user_screen_name(&response.body), None);
}

#[test]
fn an_anonymous_client_is_not_logged_in() {
    let client = AtCoderClient::new(&AtcoderConfig::default()).unwrap();
    assert_eq!(auth::current_user(&client).unwrap(), None);
}

/// Whether the Turnstile that rules out id / password login is still there.
///
/// If this fails, AtCoder has dropped the CAPTCHA and password login could come
/// back; see `atcoder::auth`.
#[test]
fn the_login_form_is_still_behind_a_captcha() {
    let client = AtCoderClient::new(&AtcoderConfig::default()).unwrap();
    let response = client.get(auth::LOGIN_URL).unwrap();
    response.error_for_status().unwrap();
    assert!(
        auth::has_captcha(&response.body),
        "Turnstile is gone; id / password login is worth reconsidering"
    );
}

/// The submit form of a finished contest carries Turnstile too — confirmed
/// against the live site on 2026-09-12.
///
/// Since March 2025 AtCoder shows the widget on submissions after a contest ends,
/// with the same sitekey as `/login`. The hidden `cf-turnstile-response` field is
/// filled in by the browser's JS, so a plain POST is refused however correct its
/// csrf_token.
///
/// Submitting during a contest still works from the command line, so this watches
/// one thing only: whether the door stays shut after the contest.
///
/// `practice` is permanent, which makes it behave like a finished contest at any
/// hour. If this test fails, AtCoder has taken the CAPTCHA off post-contest
/// submissions.
///
/// Needs a session (`ACRUST_SESSION_FILE`, or the usual place). Skips without one.
#[test]
fn the_submit_form_is_still_behind_a_captcha() {
    let client = AtCoderClient::new(&AtcoderConfig::default()).unwrap();
    if !client.load_session().unwrap() {
        eprintln!("skip: not logged in, so the submit form is out of reach");
        return;
    }
    let response = client
        .get("https://atcoder.jp/contests/practice/submit")
        .unwrap();
    response.error_for_status().unwrap();
    if html::user_screen_name(&response.body).is_none() {
        eprintln!("skip: the session is not valid");
        return;
    }
    assert!(
        response.body.contains("form-code-submit"),
        "the submit form is not where it used to be"
    );
    assert!(
        auth::has_captcha(&response.body),
        "Turnstile is gone from a finished contest's submit form; revisit the note in the README"
    );
}
