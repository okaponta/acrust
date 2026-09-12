//! `acrust test`.
//!
//! One build, then the cases run in parallel. All AC exits 0 and anything else
//! exits 1, so it drops straight into a shell `&&` or a CI step.

use crate::config::LoadedConfig;
use crate::judge::Verdict;
use crate::runner::{self, Outcome};
use crate::testcases::{SuiteKind, TestSuite};
use crate::ui;
use crate::workspace::{self, Origin, Package};
use anyhow::{bail, Result};
use std::process::ExitCode;
use std::time::Duration;

/// Used when the problem's time limit could not be read.
const FALLBACK_TIMELIMIT: Duration = Duration::from_secs(10);

/// Floor on how long a case is given before it is killed.
///
/// A laptop, busy with other work, running a debug build, is slower than the
/// judge. A 2s limit scaled by 1.5 would cut off at 3s and call a solution TLE
/// that the judge accepts, so even short limits wait at least this long.
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
        // Having guessed the problem, say which one out loud: submitting to the
        // wrong one costs a penalty that cannot be taken back.
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

/// The details of one failed case.
///
/// Labels are the names of the things themselves — `input`, `expected`, `output`,
/// `stderr`. Expected and actual go in separate blocks rather than side by side,
/// with `✗` against the lines that differ; competitive output is wide, and
/// columns force a wrap exactly when the answer is long enough to be interesting.
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

/// Keeps the panic message and drops the rest.
///
/// Backtraces are long, so `runner` sets `RUST_BACKTRACE=0`. The note about
/// setting it that comes back in exchange is noise to anyone who would want it.
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

    /// A solution's own debug output is shown as it is, never filtered.
    #[test]
    fn ordinary_stderr_is_left_alone() {
        assert_eq!(clean_stderr("dbg: n = 8\n"), "dbg: n = 8");
        assert_eq!(clean_stderr("   \n"), "");
    }
}
