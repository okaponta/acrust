//! `acrust submit`.
//!
//! What is sent is always `src/bin/{alias}.rs` as it stands. There is no hook for
//! rewriting it on the way out, which is what makes "what you submitted is what
//! the repository holds" true without qualification.
//!
//! The result is followed by default. `/contests/*/submissions/` is Disallowed by
//! robots.txt, and the moment you want to watch is right after a contest, when
//! AtCoder is busiest — so watching backs off exponentially, stops the instant
//! the verdict settles, gives up after a minute, and honours `Retry-After`.

use crate::atcoder::submit::{self as parse, Language, Submission};
use crate::atcoder::{auth, AtCoderClient};
use crate::cache;
use crate::config::LoadedConfig;
use crate::testcases::{SuiteKind, TestSuite};
use crate::ui;
use crate::workspace::{self, Origin, Package, Resolved};
use anyhow::{bail, Context as _, Result};
use std::io::Write as _;
use std::process::ExitCode;
use std::time::{Duration, Instant};

/// Each poll waits this much longer than the last.
const BACKOFF_FACTOR: f64 = 1.5;
/// Ceiling on the interval.
const MAX_INTERVAL: Duration = Duration::from_secs(10);
/// Give up after this many failures in a row.
const MAX_CONSECUTIVE_FAILURES: u32 = 3;

pub fn run(problem: Option<String>, force: bool, no_watch: bool) -> Result<ExitCode> {
    let config = LoadedConfig::find()?;
    let package = Package::find()?;
    let resolved = workspace::resolve_problem(
        &package,
        problem.as_deref(),
        config.config.test.resolve,
        std::fs::read_to_string(config.template_src())
            .ok()
            .as_deref(),
    )?;
    ui::arrow(&resolved.describe(&package));

    // Confirm only what was guessed. The costs are lopsided: a wrong guess is a
    // penalty that cannot be taken back, a wrong confirmation is one keystroke.
    if resolved.origin == Origin::Inferred && !confirm(&resolved, &package)? {
        ui::info("did not submit");
        return Ok(ExitCode::FAILURE);
    }

    let task = package.task_url(&resolved.bin.alias)?;
    let screen_name = task
        .rsplit('/')
        .next()
        .context("could not pull the screen name out of the problem URL")?
        .to_owned();

    let source = std::fs::read_to_string(&resolved.bin.src_path)
        .with_context(|| format!("could not read {}", resolved.bin.src_path.display()))?;
    if source.trim().is_empty() {
        bail!("{} is empty", resolved.bin.src_path.display());
    }

    if config.config.submit.test_before_submit && !force {
        if let Some(code) = test_first(&config, &package, &resolved)? {
            return Ok(code);
        }
    }

    let client = AtCoderClient::new(&config.config.atcoder)?;
    if !client.load_session()? {
        bail!("not logged in. Run `acrust login`");
    }
    let user = auth::current_user(&client)?
        .context("the session is not valid. Run `acrust login` again")?;
    ui::field("login", &user);

    let submit_url = format!("https://atcoder.jp/contests/{}/submit", package.contest);
    let page = client.get(&submit_url)?;
    page.error_for_status()?;
    let csrf_token = crate::atcoder::html::csrf_token(&page.body)
        .context("could not find csrf_token on the submit page")?;

    let pattern = config.config.submit.language_pattern.clone();
    let language = choose_language(&config, &page.body, &screen_name, &pattern)?;
    ui::field(
        "language",
        &format!("{} (id={})", language.name, language.id),
    );

    let form = submit_form(
        &page.body,
        &package.contest,
        &csrf_token,
        &screen_name,
        &language.id,
        &source,
    );
    let form: Vec<(&str, &str)> = form
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    let response = client.post_form(&submit_url, &form, &submit_url)?;

    let location = match &response.location {
        // AtCoder sends an accepted submission to /submissions/me itself.
        Some(location) if location.contains("/submissions/me") => location.clone(),
        Some(location) => {
            // A stale language id is one way to land here; drop the cached one
            // so the next attempt reads it off the page again.
            let _ = cache::forget_language(&pattern);
            bail!("the submission was not accepted (redirected to {location})");
        }
        None => {
            let _ = cache::forget_language(&pattern);
            report_rejection(&response, auth::has_captcha(&page.body));
            bail!("the submission was not accepted");
        }
    };

    let listing = client.get(&location)?;
    listing.error_for_status()?;
    let submission = parse::parse_latest_submission(&listing.body)
        .context("could not find your submission in the list")?;
    if !submission.task.is_empty() && submission.task != screen_name {
        ui::warn(&format!(
            "the newest submission is for a different problem ({}). Check in your browser",
            submission.task
        ));
    }
    let url = submission.url(&package.contest);
    ui::ok(&format!("submitted  {}  {url}", submission.verdict));

    if no_watch || !config.config.submit.watch {
        return Ok(ExitCode::SUCCESS);
    }
    Ok(watch(&client, &config, &package.contest, &submission, &url))
}

/// Builds the form to POST: every hidden field the page carries, then the problem,
/// the language and the source on top.
///
/// Sending `csrf_token` alone would work until the day AtCoder adds a field, and
/// the only thing said about it would be "エラーが発生しました。".
fn submit_form(
    page: &str,
    contest: &str,
    csrf_token: &str,
    screen_name: &str,
    language_id: &str,
    source: &str,
) -> Vec<(String, String)> {
    let mut form = parse::hidden_inputs(page, contest);
    if !form.iter().any(|(name, _)| name == "csrf_token") {
        form.push(("csrf_token".to_owned(), csrf_token.to_owned()));
    }
    for (name, value) in [
        ("data.TaskScreenName", screen_name),
        ("data.LanguageId", language_id),
        ("sourceCode", source),
    ] {
        match form.iter_mut().find(|(existing, _)| existing == name) {
            Some(slot) => slot.1 = value.to_owned(),
            None => form.push((name.to_owned(), value.to_owned())),
        }
    }
    form
}

/// Says enough about a rejection for the user to know what to do next.
///
/// AtCoder gives no reason beyond "エラーが発生しました。", so this adds what can
/// be worked out from here: whether the page had a CAPTCHA, the HTTP status, and
/// whether the reply still considers us logged in.
fn report_rejection(response: &crate::atcoder::client::AtCoderResponse, page_had_captcha: bool) {
    if page_had_captcha {
        // Since March 2025 the submit form of a *finished* contest carries
        // Cloudflare Turnstile too (confirmed on 2026-09-12). The hidden
        // `cf-turnstile-response` field comes from the browser's JS, so a plain
        // POST is refused however correct its csrf_token — the same wall as
        // `/login`. Submitting during a contest still goes through.
        ui::warn("the contest is over, so submit cannot run. Use copy and submit it by hand");
        return;
    }
    ui::warn(&format!("AtCoder replied: {}", response.status));
    for alert in crate::atcoder::html::alerts(&response.body) {
        ui::warn_detail(&alert);
    }
    if crate::atcoder::html::user_screen_name(&response.body).is_none() {
        ui::warn_detail("this reply treats you as logged out");
        ui::warn_detail("run `acrust login` to get a fresh session");
    } else {
        ui::warn_detail("check whether the same submission works from your browser");
    }
}

/// Runs the sample tests first, and stops here if they do not pass.
fn test_first(
    config: &LoadedConfig,
    package: &Package,
    resolved: &Resolved,
) -> Result<Option<ExitCode>> {
    let package_rel = crate::package::relative(&config.root, &package.dir);
    let path = config.testcases_path(&package_rel, &resolved.bin.alias);
    if !path.is_file() {
        ui::warn(&format!(
            "{} is missing, so submitting without testing",
            path.display()
        ));
        return Ok(None);
    }
    if TestSuite::load(&path)?.kind == SuiteKind::Interactive {
        ui::warn("interactive problem, so skipping the sample tests");
        return Ok(None);
    }

    let code = crate::commands::test::run(Some(resolved.bin.alias.clone()), false)?;
    if code == ExitCode::SUCCESS {
        return Ok(None);
    }
    ui::error("the sample tests did not pass, so not submitting (-f overrides this)");
    Ok(Some(ExitCode::FAILURE))
}

/// Reads the language id off the submit page rather than hardcoding one.
fn choose_language(
    config: &LoadedConfig,
    page: &str,
    screen_name: &str,
    pattern: &str,
) -> Result<Language> {
    let configured = config.config.submit.language_id.trim();
    if !configured.is_empty() {
        return Ok(Language {
            id: configured.to_owned(),
            name: format!("(set in the config: {configured})"),
        });
    }
    if let Some(cached) = cache::language_for(pattern) {
        return Ok(cached);
    }
    let languages = parse::parse_languages(page, Some(screen_name));
    let language = parse::pick_language(&languages, pattern)?;
    let _ = cache::remember_language(pattern, &language);
    Ok(language)
}

fn watch(
    client: &AtCoderClient,
    config: &LoadedConfig,
    contest: &str,
    submission: &Submission,
    url: &str,
) -> ExitCode {
    if parse::is_final(&submission.verdict) {
        return verdict_exit_code(&submission.verdict);
    }

    // The API the AtCoder page itself polls: a fortieth of the page's size.
    let status_url = format!(
        "https://atcoder.jp/contests/{contest}/submissions/me/status/json?reload=true&sids[]={}",
        submission.id
    );
    let deadline = Instant::now() + Duration::from_secs(config.config.submit.watch_timeout_s);
    let mut interval = Duration::from_millis(config.config.submit.watch_interval_ms);
    let mut failures = 0;
    let started = Instant::now();

    while Instant::now() < deadline {
        std::thread::sleep(interval.min(deadline.saturating_duration_since(Instant::now())));
        if Instant::now() >= deadline {
            break;
        }

        match client.get(&status_url) {
            Ok(response) if response.status.is_success() => {
                failures = 0;
                if let Some(status) = parse::parse_status_json(&response.body, &submission.id) {
                    let detail = [status.time.as_deref(), status.memory.as_deref()]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" / ");
                    ui::verdict(
                        &status.verdict,
                        status.verdict == "AC",
                        &format!("{:.0}s", started.elapsed().as_secs_f64()),
                        &detail,
                    );
                    if parse::is_final(&status.verdict) {
                        return verdict_exit_code(&status.verdict);
                    }
                }
            }
            Ok(response) => {
                failures += 1;
                ui::warn(&format!("could not get the result ({})", response.status));
            }
            Err(e) => {
                failures += 1;
                ui::warn(&format!("could not get the result: {e:#}"));
            }
        }

        if failures >= MAX_CONSECUTIVE_FAILURES {
            ui::warn(&format!(
                "giving up on following the result. Check in your browser: {url}"
            ));
            return ExitCode::SUCCESS;
        }
        interval = next_interval(interval);
    }

    // When the judge is backed up, the browser is the faster place to watch.
    ui::info(&format!(
        "no verdict after {} seconds, so no longer following it: {url}",
        config.config.submit.watch_timeout_s
    ));
    ExitCode::SUCCESS
}

/// The next polling interval: 1.5x each time, never past 10 seconds.
fn next_interval(current: Duration) -> Duration {
    current.mul_f64(BACKOFF_FACTOR).min(MAX_INTERVAL)
}

/// Anything but AC exits non-zero, so `submit && ...` works.
fn verdict_exit_code(verdict: &str) -> ExitCode {
    if verdict.trim() == "AC" {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Asks before submitting a guessed problem. The default answer is no.
fn confirm(resolved: &Resolved, package: &Package) -> Result<bool> {
    use std::io::IsTerminal as _;

    if !std::io::stdin().is_terminal() {
        bail!(
            "inferred a problem but cannot confirm it (not a terminal). \
             Name the problem, as in `acrust submit {}`",
            resolved.bin.alias
        );
    }
    print!("Submit {} {}? [y/N]: ", package.contest, resolved.bin.alias);
    std::io::stdout().flush().ok();
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .context("could not read stdin")?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "Yes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2s -> 3s -> 4.5s -> 6.75s -> 10s -> 10s ...
    #[test]
    fn the_backoff_follows_the_documented_schedule() {
        let mut interval = Duration::from_millis(2000);
        let mut schedule = vec![interval];
        for _ in 0..5 {
            interval = next_interval(interval);
            schedule.push(interval);
        }
        assert_eq!(
            schedule,
            [
                Duration::from_millis(2000),
                Duration::from_millis(3000),
                Duration::from_millis(4500),
                Duration::from_millis(6750),
                Duration::from_millis(10000),
                Duration::from_millis(10000),
            ]
        );
    }

    /// At most 8 requests a minute, so watching cannot hammer a busy judge.
    #[test]
    fn one_minute_of_watching_is_at_most_eight_requests() {
        let deadline = Duration::from_secs(60);
        let mut interval = Duration::from_millis(2000);
        let mut elapsed = Duration::ZERO;
        let mut requests = 0;
        while elapsed + interval < deadline {
            elapsed += interval;
            requests += 1;
            interval = next_interval(interval);
        }
        assert_eq!(requests, 8, "{requests} requests in a minute");
    }

    /// What goes out is the page's own form plus problem, language and source.
    #[test]
    fn the_form_keeps_every_hidden_field_the_page_carries() {
        let page = r#"
<form action="/contests/abc418/submit" method="POST">
  <input type="hidden" name="csrf_token" value="from-the-page" />
  <input type="hidden" name="data.SomethingNew" value="42" />
</form>"#;
        let form = submit_form(
            page,
            "abc418",
            "fallback",
            "abc418_a",
            "6088",
            "fn main(){}",
        );
        assert_eq!(
            form,
            [
                ("csrf_token".to_owned(), "from-the-page".to_owned()),
                ("data.SomethingNew".to_owned(), "42".to_owned()),
                ("data.TaskScreenName".to_owned(), "abc418_a".to_owned()),
                ("data.LanguageId".to_owned(), "6088".to_owned()),
                ("sourceCode".to_owned(), "fn main(){}".to_owned()),
            ]
        );
    }

    /// An unreadable form still submits, on the token from `var csrfToken`.
    #[test]
    fn a_page_without_a_form_falls_back_to_the_scripts_token() {
        let form = submit_form(
            "<html></html>",
            "abc418",
            "fallback",
            "abc418_a",
            "6088",
            "x",
        );
        assert_eq!(form[0], ("csrf_token".to_owned(), "fallback".to_owned()));
        assert_eq!(form.len(), 4);
    }

    #[test]
    fn only_ac_exits_successfully() {
        assert_eq!(verdict_exit_code("AC"), ExitCode::SUCCESS);
        for verdict in ["WA", "TLE", "RE", "CE"] {
            assert_eq!(verdict_exit_code(verdict), ExitCode::FAILURE, "{verdict}");
        }
    }
}
