//! Building, and running the test cases.
//!
//! The path to the executable is never guessed: it is whatever
//! `cargo build --message-format=json` reports as `executable`. That holds up
//! against a `target-dir` in `.cargo/config.toml` and against any workspace
//! layout.

use crate::config::Profile;
use crate::judge::Verdict;
use crate::testcases::{FloatTolerance, Matching, TestCase};
use anyhow::{bail, Context as _, Result};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The result of one case.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub name: String,
    pub verdict: Verdict,
    pub elapsed: Duration,
    pub stdout: String,
    pub stderr: String,
    /// How it died, when it did: `exit code 101`, `killed by signal 6`.
    pub status: Option<String>,
}

/// Builds once and returns the executable cargo says it produced.
pub fn build(manifest_path: &Path, bin: &str, profile: Profile) -> Result<PathBuf> {
    let mut command = Command::new("cargo");
    command
        .arg("build")
        .arg("--manifest-path")
        .arg(manifest_path)
        .arg("--bin")
        .arg(bin)
        // Diagnostics stay human-readable on stderr; only the artifact
        // information comes back as JSON.
        .arg("--message-format=json-render-diagnostics")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if profile == Profile::Release {
        command.arg("--release");
    }

    let output = command.output().context("could not start cargo build")?;
    if !output.status.success() {
        bail!("the build failed ({})", output.status);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let executable = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|message| message["reason"] == "compiler-artifact")
        .filter_map(|message| message["executable"].as_str().map(PathBuf::from))
        .next_back();

    executable.with_context(|| format!("cargo build did not report an executable for {bin}"))
}

/// Runs the cases in parallel. `jobs` of 0 means one per logical core.
pub fn run_cases(
    executable: &Path,
    cases: &[TestCase],
    timeout: Duration,
    jobs: usize,
    matching: Matching,
    float: Option<FloatTolerance>,
) -> Vec<Outcome> {
    if cases.is_empty() {
        return Vec::new();
    }
    warm_up(executable);
    let jobs = resolve_jobs(jobs).min(cases.len());
    let next = AtomicUsize::new(0);
    let collected: Mutex<Vec<(usize, Outcome)>> = Mutex::new(Vec::with_capacity(cases.len()));

    std::thread::scope(|scope| {
        for _ in 0..jobs {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::SeqCst);
                if index >= cases.len() {
                    return;
                }
                let outcome = run_case(executable, &cases[index], timeout, matching, float);
                collected
                    .lock()
                    .expect("result list is not poisoned")
                    .push((index, outcome));
            });
        }
    });

    // Run in parallel, reported in the order the cases are written.
    let mut collected = collected.into_inner().expect("result list is not poisoned");
    collected.sort_by_key(|(index, _)| *index);
    collected.into_iter().map(|(_, outcome)| outcome).collect()
}

/// One throwaway run before anything is measured.
///
/// On macOS the first run of a freshly built binary takes upwards of 300ms —
/// signature checking and paging in. `acrust test` always runs right after a
/// build, and the cases run in parallel, so without this every case pays that
/// cost and every measurement is inflated by it.
///
/// stdin is closed immediately, so a solution that reads input finishes at once;
/// the short limit is for one that does not. The result is ignored.
fn warm_up(executable: &Path) {
    // Measured at 300-400ms, with room to spare for a busy machine.
    const WARM_UP_LIMIT: Duration = Duration::from_secs(1);

    let Ok(mut child) = Command::new(executable)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return;
    };
    if let Ok(None) = wait_with_timeout(&mut child, WARM_UP_LIMIT) {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn resolve_jobs(jobs: usize) -> usize {
    if jobs > 0 {
        return jobs;
    }
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

fn run_case(
    executable: &Path,
    case: &TestCase,
    timeout: Duration,
    matching: Matching,
    float: Option<FloatTolerance>,
) -> Outcome {
    let started = Instant::now();
    match execute(executable, &case.input, timeout) {
        Ok(run) => {
            let verdict = if run.timed_out {
                Verdict::TimeLimitExceeded
            } else if run.status.is_some() {
                Verdict::RuntimeError
            } else if crate::judge::matches(&case.output, &run.stdout, matching, float) {
                Verdict::Accepted
            } else {
                Verdict::WrongAnswer
            };
            Outcome {
                name: case.name.clone(),
                verdict,
                elapsed: run.elapsed,
                stdout: run.stdout,
                stderr: run.stderr,
                status: run.status,
            }
        }
        Err(e) => Outcome {
            name: case.name.clone(),
            verdict: Verdict::RuntimeError,
            elapsed: started.elapsed(),
            stdout: String::new(),
            stderr: format!("{e:#}"),
            status: Some("could not start it".to_owned()),
        },
    }
}

/// What you want from an RE is the panic's location and message; the backtrace is
/// just long. It stays off unless the user set `RUST_BACKTRACE` themselves.
fn backtrace_env() -> Option<(&'static str, &'static str)> {
    std::env::var_os("RUST_BACKTRACE")
        .is_none()
        .then_some(("RUST_BACKTRACE", "0"))
}

struct Run {
    stdout: String,
    stderr: String,
    elapsed: Duration,
    timed_out: bool,
    /// `None` when it exited cleanly.
    status: Option<String>,
}

/// Feeds stdin and drains stdout and stderr on their own threads, with a timeout.
///
/// Waiting without draining is what makes a fast solution look like a TLE: fill
/// the pipe buffer and the process blocks on its own output.
fn execute(executable: &Path, input: &str, timeout: Duration) -> Result<Run> {
    let started = Instant::now();
    let mut child = Command::new(executable)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .envs(backtrace_env())
        .spawn()
        .with_context(|| format!("could not start {}", executable.display()))?;

    let mut stdin = child.stdin.take().context("could not take stdin")?;
    let payload = input.to_owned();
    let writer = std::thread::spawn(move || {
        // EPIPE here is normal: plenty of solutions stop reading early.
        let _ = stdin.write_all(payload.as_bytes());
        let _ = stdin.flush();
        // Dropped here, which is what sends EOF.
    });

    let mut stdout = child.stdout.take().context("could not take stdout")?;
    let stdout_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stdout.read_to_end(&mut buffer);
        buffer
    });
    let mut stderr = child.stderr.take().context("could not take stderr")?;
    let stderr_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stderr.read_to_end(&mut buffer);
        buffer
    });

    let waited = wait_with_timeout(&mut child, timeout)
        .context("could not wait for the process to finish")?;
    let timed_out = waited.is_none();
    let status = match waited {
        Some(status) => Some(status),
        None => {
            let _ = child.kill();
            child.wait().ok()
        }
    };
    let elapsed = started.elapsed();

    let _ = writer.join();
    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();

    let failure = match (timed_out, status) {
        (true, _) => None,
        (false, Some(status)) if status.success() => None,
        (false, Some(status)) => Some(describe(&status)),
        (false, None) => Some("could not get the exit status".to_owned()),
    };

    Ok(Run {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        elapsed,
        timed_out,
        status: failure,
    })
}

/// Waits for the child, giving up at `timeout`.
///
/// Not the `wait-timeout` crate: its first call costs about 190ms setting up
/// SIGCHLD handling, and that lands on every measurement. Polling from a short
/// interval up to 2ms keeps the error on a fast case under a millisecond, and
/// costs nothing worth counting on a slow one.
fn wait_with_timeout(child: &mut Child, timeout: Duration) -> std::io::Result<Option<ExitStatus>> {
    const FIRST_INTERVAL: Duration = Duration::from_micros(200);
    const MAX_INTERVAL: Duration = Duration::from_millis(2);

    let start = Instant::now();
    let mut interval = FIRST_INTERVAL;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            return Ok(None);
        }
        std::thread::sleep(interval.min(timeout - elapsed));
        interval = (interval * 2).min(MAX_INTERVAL);
    }
}

#[cfg(unix)]
fn describe(status: &std::process::ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt as _;
    match (status.code(), status.signal()) {
        (Some(code), _) => format!("exit code {code}"),
        (None, Some(signal)) => format!("killed by signal {signal}"),
        (None, None) => "abnormal exit".to_owned(),
    }
}

#[cfg(not(unix))]
fn describe(status: &std::process::ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("exit code {code}"),
        None => "abnormal exit".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_jobs_means_one_per_logical_core() {
        assert!(resolve_jobs(0) >= 1);
        assert_eq!(resolve_jobs(3), 3);
    }
}
