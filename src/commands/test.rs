//! `acrust test`（設計 §4.7）。
//!
//! ビルドは1回、実行は並列。全 AC なら終了コード 0、そうでなければ 1 を返すので
//! シェルの `&&` や CI にそのまま繋げられる。

use crate::config::LoadedConfig;
use crate::judge::Verdict;
use crate::runner::{self, Outcome};
use crate::testcases::{SuiteKind, TestSuite};
use crate::ui;
use crate::workspace::{self, Origin, Package};
use anyhow::{bail, Result};
use std::process::ExitCode;
use std::time::Duration;

/// 制限時間が取れなかった問題で使う値。
const FALLBACK_TIMELIMIT: Duration = Duration::from_secs(10);

/// 打ち切りまでの最低待ち時間。
///
/// 手元のマシンはジャッジより遅いことがある（ノート PC・他の処理と同時・debug ビルド）。
/// TL 2 秒 × 倍率 1.5 = 3 秒で切ると、ジャッジでは通る解答を TLE と言ってしまうので、
/// 短い TL の問題でも 5 秒は待つ。
const MIN_TIMEOUT: Duration = Duration::from_secs(5);

pub fn run(problem: Option<String>, release: bool) -> Result<ExitCode> {
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
    if resolved.origin == Origin::Inferred {
        // 推定したら必ず対象を見せる（決定 D7）。
        ui::arrow(&resolved.describe(&package));
    }

    let package_rel = crate::package::relative(&config.root, &package.dir);
    let path = config.testcases_path(&package_rel, &resolved.bin.alias);
    if !path.is_file() {
        bail!(
            "{} is missing. Run `acrust fetch {}` to get the samples",
            path.display(),
            package.contest
        );
    }
    let suite = TestSuite::load(&path)?;

    if suite.kind == SuiteKind::Interactive {
        ui::warn(&format!(
            "{} {} is interactive, so there are no sample tests to run",
            package.contest, resolved.bin.alias
        ));
        return Ok(ExitCode::SUCCESS);
    }
    if suite.cases.is_empty() {
        bail!("{} has no test cases", path.display());
    }

    let profile = if release {
        crate::config::Profile::Release
    } else {
        config.config.test.profile
    };
    let executable = runner::build(&package.manifest_path, &resolved.bin.name, profile)?;

    let timelimit = suite
        .timelimit_ms()
        .map(Duration::from_millis)
        .unwrap_or(FALLBACK_TIMELIMIT);
    let timeout = timelimit
        .mul_f64(config.config.test.timeout_margin.max(1.0))
        .max(MIN_TIMEOUT);

    let outcomes = runner::run_cases(
        &executable,
        &suite.cases,
        timeout,
        config.config.test.jobs,
        suite.matching,
        suite.float,
    );

    report(&suite, &outcomes, timelimit);
    let failed = outcomes.iter().filter(|o| !o.verdict.is_accepted()).count();
    if failed == 0 {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::FAILURE)
    }
}

fn report(suite: &TestSuite, outcomes: &[Outcome], timelimit: Duration) {
    ui::info("");
    for outcome in outcomes {
        ui::verdict(
            outcome.verdict.label(),
            outcome.verdict.is_accepted(),
            &outcome.name,
            &format!("{} ms", outcome.elapsed.as_millis()),
        );
    }

    for outcome in outcomes.iter().filter(|o| !o.verdict.is_accepted()) {
        show_failure(suite, outcome, timelimit);
    }

    let passed = outcomes.iter().filter(|o| o.verdict.is_accepted()).count();
    ui::info("");
    if passed == outcomes.len() {
        ui::ok(&format!("{passed}/{} AC", outcomes.len()));
    } else {
        ui::error(&format!("{passed}/{} AC", outcomes.len()));
    }
}

/// 失敗した1ケースの中身。
///
/// ラベルは実際の入出力の名前（`input` / `expected` / `output` / `stderr`）で揃える。
/// 期待と実際は横に並べず別のブロックにし、食い違う行に `✗` を付ける。
fn show_failure(suite: &TestSuite, outcome: &Outcome, timelimit: Duration) {
    let case = suite.cases.iter().find(|case| case.name == outcome.name);
    ui::info("");
    ui::section(&format!("{} {}", outcome.name, outcome.verdict.label()));

    if let Some(case) = case {
        ui::block("input", &case.input);
        match outcome.verdict {
            Verdict::TimeLimitExceeded => {
                ui::inline(
                    "timelimit",
                    &format!(
                        "over {} ms (killed at {} ms)",
                        timelimit.as_millis(),
                        outcome.elapsed.as_millis()
                    ),
                );
            }
            Verdict::RuntimeError => {
                if let Some(status) = &outcome.status {
                    ui::inline("exit", status);
                }
            }
            _ => {
                ui::expected_and_output(&case.output, &outcome.stdout);
            }
        }
        if outcome.verdict != Verdict::WrongAnswer && !outcome.stdout.trim().is_empty() {
            ui::block("output", &outcome.stdout);
        }
    }

    let stderr = clean_stderr(&outcome.stderr);
    if !stderr.is_empty() {
        ui::block("stderr", &stderr);
    }
}

/// 標準エラーからパニックのメッセージだけ残す。
///
/// バックトレースは長いので既定では出さない（`runner` が `RUST_BACKTRACE=0` にする）。
/// そのとき付いてくる「RUST_BACKTRACE を立てろ」の案内は、立て方を知っている人には
/// 不要なので落とす。
fn clean_stderr(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| {
            !line
                .trim_start()
                .starts_with("note: run with `RUST_BACKTRACE")
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backtrace_hint_is_not_part_of_the_panic_message() {
        let stderr = "\nthread 'main' panicked at src/bin/a.rs:2:65:\n\
                      わざと落とす\n\
                      note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n";
        assert_eq!(
            clean_stderr(stderr),
            "thread 'main' panicked at src/bin/a.rs:2:65:\nわざと落とす"
        );
    }

    /// 解答が自分でデバッグ出力しているときは、消さずにそのまま見せる。
    #[test]
    fn ordinary_stderr_is_left_alone() {
        assert_eq!(clean_stderr("dbg: n = 8\n"), "dbg: n = 8");
        assert_eq!(clean_stderr("   \n"), "");
    }
}
