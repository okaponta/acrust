//! `acrust submit`（設計 §3.4 / §4.9 / 決定 D7 / D9 / D13）。
//!
//! 提出するのは常に `src/bin/{alias}.rs` そのもの。差し替え口は設けない（D13）ので、
//! 「提出したもの = リポジトリの中身」が常に成り立つ。
//!
//! 提出後の結果追跡は既定 ON（D9）。`/contests/*/submissions/` は robots.txt で
//! Disallow されているうえ、追いたい瞬間はコンテスト直後＝AtCoder が最も混む時間帯なので、
//! §4.9 の作法（指数バックオフ・確定即停止・1 分打ち切り・`Retry-After` 遵守）を必ず守る。

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

/// 追跡の間隔をこの倍率で伸ばす（設計 §4.9）。
const BACKOFF_FACTOR: f64 = 1.5;
/// 間隔の上限。
const MAX_INTERVAL: Duration = Duration::from_secs(10);
/// 連続でこれだけ失敗したら追跡をやめる。
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

    // 誤推定のコストが非対称なので、推定したときだけ確認する（決定 D7）。
    // 明示指定なら確認しない。
    if resolved.origin == Origin::Inferred && !confirm(&resolved, &package)? {
        ui::info("提出をやめました");
        return Ok(ExitCode::FAILURE);
    }

    let task = package.task_url(&resolved.bin.alias)?;
    let screen_name = task
        .rsplit('/')
        .next()
        .context("問題URLから screen name を取り出せませんでした")?
        .to_owned();

    let source = std::fs::read_to_string(&resolved.bin.src_path)
        .with_context(|| format!("{} を読めませんでした", resolved.bin.src_path.display()))?;
    if source.trim().is_empty() {
        bail!("{} が空です", resolved.bin.src_path.display());
    }

    if config.config.submit.test_before_submit && !force {
        if let Some(code) = test_first(&config, &package, &resolved)? {
            return Ok(code);
        }
    }

    let client = AtCoderClient::new(&config.config.atcoder)?;
    if !client.load_session()? {
        bail!("ログインしていません。`acrust login` を実行してください");
    }
    let user = auth::current_user(&client)?
        .context("セッションが無効です。`acrust login` をやり直してください")?;
    ui::field("ログイン", &user);

    let submit_url = format!("https://atcoder.jp/contests/{}/submit", package.contest);
    let page = client.get(&submit_url)?;
    page.error_for_status()?;
    let csrf_token = crate::atcoder::html::csrf_token(&page.body)
        .context("提出ページから csrf_token を取り出せませんでした")?;

    let pattern = config.config.submit.language_pattern.clone();
    let language = choose_language(&config, &page.body, &screen_name, &pattern)?;
    ui::field("言語", &format!("{} (id={})", language.name, language.id));

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
        // AtCoder 自身が /submissions/me へ飛ばす。ここに来れば受理されている。
        Some(location) if location.contains("/submissions/me") => location.clone(),
        Some(location) => {
            // 言語 ID が古いと弾かれる。キャッシュを捨てて次回に備える。
            let _ = cache::forget_language(&pattern);
            bail!("提出が受理されませんでした（{location} に飛ばされました）");
        }
        None => {
            let _ = cache::forget_language(&pattern);
            report_rejection(&response);
            bail!("提出が受理されませんでした");
        }
    };

    let listing = client.get(&location)?;
    listing.error_for_status()?;
    let submission = parse::parse_latest_submission(&listing.body)
        .context("提出一覧から自分の提出を見つけられませんでした")?;
    if !submission.task.is_empty() && submission.task != screen_name {
        ui::warn(&format!(
            "提出一覧の先頭が別の問題（{}）でした。ブラウザで確認してください",
            submission.task
        ));
    }
    let url = submission.url(&package.contest);
    ui::ok(&format!("提出しました  {}  {url}", submission.verdict));

    if no_watch || !config.config.submit.watch {
        return Ok(ExitCode::SUCCESS);
    }
    Ok(watch(&client, &config, &package.contest, &submission, &url))
}

/// POST するフォームを組む。
///
/// 隠しフィールドはページに書かれている通りに写し、そのうえで問題・言語・ソースを載せる。
/// `csrf_token` だけを送る形にしないのは、AtCoder が隠しフィールドを増やしたときに
/// 「エラーが発生しました。」とだけ言われて原因が分からなくなるため。
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

/// 受理されなかったときに、次の一手が分かるだけの情報を出す。
///
/// AtCoder は理由を「エラーが発生しました。」としか言わないことがあるので、
/// こちら側で分かること（HTTP ステータス・セッションが生きているか）を添える。
fn report_rejection(response: &crate::atcoder::client::AtCoderResponse) {
    ui::warn(&format!("AtCoder の応答: {}", response.status));
    for alert in crate::atcoder::html::alerts(&response.body) {
        ui::warn_detail(&alert);
    }
    if crate::atcoder::html::user_screen_name(&response.body).is_none() {
        ui::warn_detail("この応答ではログインしていない扱いになっています");
        ui::warn_detail("`acrust login` でセッションを取り直してください");
    } else {
        ui::warn_detail("ブラウザから同じ問題に提出できるか確かめてください");
    }
}

/// 提出前にサンプルテストを通す。通らなければここで止める。
fn test_first(
    config: &LoadedConfig,
    package: &Package,
    resolved: &Resolved,
) -> Result<Option<ExitCode>> {
    let package_rel = crate::package::relative(&config.root, &package.dir);
    let path = config.testcases_path(&package_rel, &resolved.bin.alias);
    if !path.is_file() {
        ui::warn(&format!(
            "{} がありません。テストせずに提出します",
            path.display()
        ));
        return Ok(None);
    }
    if TestSuite::load(&path)?.kind == SuiteKind::Interactive {
        ui::warn("インタラクティブ問題なのでサンプルテストは省きます");
        return Ok(None);
    }

    let code = crate::commands::test::run(Some(resolved.bin.alias.clone()), false)?;
    if code == ExitCode::SUCCESS {
        return Ok(None);
    }
    ui::error("サンプルテストが通らなかったので提出しません（-f で無視できます）");
    Ok(Some(ExitCode::FAILURE))
}

/// 言語 ID はハードコードせず、提出ページの `<select>` から選ぶ（設計 §3.4）。
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
            name: format!("(設定で指定: {configured})"),
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

/// 提出結果を追う（設計 §4.9）。
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

    // AtCoder のページ自身がポーリングしている API。実測でページ全体の 1/40 の大きさ。
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
                ui::warn(&format!("結果の取得に失敗しました（{}）", response.status));
            }
            Err(e) => {
                failures += 1;
                ui::warn(&format!("結果の取得に失敗しました: {e:#}"));
            }
        }

        if failures >= MAX_CONSECUTIVE_FAILURES {
            ui::warn(&format!(
                "結果の追跡をやめます。ブラウザで確認してください: {url}"
            ));
            return ExitCode::SUCCESS;
        }
        interval = next_interval(interval);
    }

    // ジャッジが混んで長引くときは、ブラウザで見た方が早い。
    ui::info(&format!(
        "{} 秒たっても確定しなかったので追跡をやめます: {url}",
        config.config.submit.watch_timeout_s
    ));
    ExitCode::SUCCESS
}

/// 次のポーリング間隔。1.5 倍ずつ、上限 10 秒（設計 §4.9）。
fn next_interval(current: Duration) -> Duration {
    current.mul_f64(BACKOFF_FACTOR).min(MAX_INTERVAL)
}

/// AC 以外は失敗として返す。シェルの `&&` で繋げられるように。
fn verdict_exit_code(verdict: &str) -> ExitCode {
    if verdict.trim() == "AC" {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// 推定した問題を提出してよいか尋ねる。既定は「いいえ」。
fn confirm(resolved: &Resolved, package: &Package) -> Result<bool> {
    use std::io::IsTerminal as _;

    if !std::io::stdin().is_terminal() {
        bail!(
            "問題を推定しましたが確認できません（対話端末ではありません）。\
             `acrust submit {}` のように問題を指定してください",
            resolved.bin.alias
        );
    }
    print!(
        "{} {} を提出します。よろしいですか？ [y/N]: ",
        package.contest, resolved.bin.alias
    );
    std::io::stdout().flush().ok();
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .context("標準入力を読めませんでした")?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "Yes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 設計 §4.9 が定めた間隔（2s → 3s → 4.5s → 6.75s → 10s → 10s …）。
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

    /// 1 分の窓に入るリクエストは 8 回まで。混む時間帯に叩き続けないための上限。
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
        assert_eq!(requests, 8, "1 分で {requests} 回");
    }

    /// 提出するのは「ページのフォーム + 問題・言語・ソース」。
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

    /// フォームが読めなくても、`var csrfToken` から取った値で組み立てられること。
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
