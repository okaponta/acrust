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
            "{} がありません。`acrust fetch {}` でサンプルを取得してください",
            path.display(),
            package.contest
        );
    }
    let suite = TestSuite::load(&path)?;

    if suite.kind == SuiteKind::Interactive {
        ui::warn(&format!(
            "{} {} はインタラクティブ問題なので、サンプルテストはできません",
            package.contest, resolved.bin.alias
        ));
        return Ok(ExitCode::SUCCESS);
    }
    if suite.cases.is_empty() {
        bail!("{} にテストケースがありません", path.display());
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
    let timeout = timelimit.mul_f64(config.config.test.timeout_margin.max(1.0));

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

fn show_failure(suite: &TestSuite, outcome: &Outcome, timelimit: Duration) {
    let case = suite.cases.iter().find(|case| case.name == outcome.name);
    ui::info("");
    ui::section(&format!("{} {}", outcome.name, outcome.verdict.label()));

    if let Some(case) = case {
        ui::block("入力", &case.input);
        match outcome.verdict {
            Verdict::TimeLimitExceeded => {
                ui::field(
                    "制限時間",
                    &format!(
                        "{} ms を超えました（{} ms で打ち切り）",
                        timelimit.as_millis(),
                        outcome.elapsed.as_millis()
                    ),
                );
            }
            Verdict::RuntimeError => {
                if let Some(status) = &outcome.status {
                    ui::field("終了状態", status);
                }
            }
            _ => {
                ui::diff("期待", &case.output, "実際", &outcome.stdout);
            }
        }
        if outcome.verdict != Verdict::WrongAnswer && !outcome.stdout.trim().is_empty() {
            ui::block("ここまでの出力", &outcome.stdout);
        }
    }

    if !outcome.stderr.trim().is_empty() {
        // バックトレースは全部出すと画面が流れる。パニックの位置が分かれば十分。
        ui::block_limited("標準エラー", outcome.stderr.trim(), 12);
    }
}
