//! ビルドと、テストケースの実行（設計 §4.7）。
//!
//! 実行ファイルのパスは推測せず、`cargo build --message-format=json` が
//! 報告する `executable` をそのまま使う。`.cargo/config.toml` の `target-dir` や
//! ワークスペースの配置に左右されない。

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

/// 1 ケースの結果。
#[derive(Debug, Clone)]
pub struct Outcome {
    pub name: String,
    pub verdict: Verdict,
    pub elapsed: Duration,
    pub stdout: String,
    pub stderr: String,
    /// 異常終了したときの説明（`exit status: 101`、`signal: 6` など）。
    pub status: Option<String>,
}

/// `cargo build --bin {bin}` を1回だけ実行し、できた実行ファイルのパスを返す。
pub fn build(manifest_path: &Path, bin: &str, profile: Profile) -> Result<PathBuf> {
    let mut command = Command::new("cargo");
    command
        .arg("build")
        .arg("--manifest-path")
        .arg(manifest_path)
        .arg("--bin")
        .arg(bin)
        // 診断は人間向けに stderr へ出しつつ、成果物の情報は JSON で受け取る。
        .arg("--message-format=json-render-diagnostics")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if profile == Profile::Release {
        command.arg("--release");
    }

    let output = command
        .output()
        .context("cargo build を起動できませんでした")?;
    if !output.status.success() {
        bail!("ビルドに失敗しました（{}）", output.status);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let executable = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|message| message["reason"] == "compiler-artifact")
        .filter_map(|message| message["executable"].as_str().map(PathBuf::from))
        .next_back();

    executable.with_context(|| format!("cargo build が {bin} の実行ファイルを報告しませんでした"))
}

/// ケースを並列に実行する。`jobs` が 0 なら論理コア数。
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

    // 実行は並列だが、表示はテストケースの順に揃える。
    let mut collected = collected.into_inner().expect("result list is not poisoned");
    collected.sort_by_key(|(index, _)| *index);
    collected.into_iter().map(|(_, outcome)| outcome).collect()
}

/// 計測の前に1回だけ空実行しておく。
///
/// macOS ではビルドし直した直後の初回実行に 300ms 以上かかる（署名の検証とページイン）。
/// `acrust test` は必ずビルドの直後に走るので、これをやらないと**全ケースの計測値が
/// 300ms 水増しされる**（並列に走るので全ケースが初回のコストを払う）。
///
/// 標準入力はすぐ閉じるので、入力を読む解答は即座に終わる。読まずに回り続ける解答のために
/// 短い上限を掛けてある。結果は一切見ない。
fn warm_up(executable: &Path) {
    // 初回実行は実測で 300〜400ms。負荷が高いときのために少し余裕を持たせる。
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
            status: Some("起動できませんでした".to_owned()),
        },
    }
}

/// RE のときに見たいのはパニックの位置とメッセージで、バックトレースは長いだけ。
///
/// 既定では出さないが、自分で `RUST_BACKTRACE` を立てている人の設定は尊重する。
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
    /// 正常終了なら `None`。
    status: Option<String>,
}

/// 標準入力を書き込み、標準出力と標準エラーを別スレッドで吸いながらタイムアウト付きで待つ。
///
/// パイプを読まずに待つと、出力の多い解答がバッファを埋めて止まり、
/// 実際には速いのに TLE に見える。
fn execute(executable: &Path, input: &str, timeout: Duration) -> Result<Run> {
    let started = Instant::now();
    let mut child = Command::new(executable)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .envs(backtrace_env())
        .spawn()
        .with_context(|| format!("{} を起動できませんでした", executable.display()))?;

    let mut stdin = child.stdin.take().context("標準入力を掴めませんでした")?;
    let payload = input.to_owned();
    let writer = std::thread::spawn(move || {
        // 相手が先に終了して EPIPE になるのは異常ではない（入力を読み切らない解答）。
        let _ = stdin.write_all(payload.as_bytes());
        let _ = stdin.flush();
        // ここで drop されて EOF が伝わる。
    });

    let mut stdout = child.stdout.take().context("標準出力を掴めませんでした")?;
    let stdout_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stdout.read_to_end(&mut buffer);
        buffer
    });
    let mut stderr = child
        .stderr
        .take()
        .context("標準エラーを掴めませんでした")?;
    let stderr_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stderr.read_to_end(&mut buffer);
        buffer
    });

    let waited =
        wait_with_timeout(&mut child, timeout).context("プロセスの終了を待てませんでした")?;
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
        (false, None) => Some("終了状態を取得できませんでした".to_owned()),
    };

    Ok(Run {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        elapsed,
        timed_out,
        status: failure,
    })
}

/// 終了を待つ。`timeout` を過ぎたら `None`。
///
/// `wait-timeout` クレートを使わないのは、初回呼び出しに 190ms ほどの固定コストがあり
/// （SIGCHLD まわりの初期化）、全ケースの計測値がその分だけ水増しされるため。
/// ここでは短い間隔から始めて 2ms まで伸ばすポーリングにしている。
/// 速いケースの誤差は 1ms 未満で、遅いケースでも待ちのコストは無視できる。
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
        (Some(code), _) => format!("終了コード {code}"),
        (None, Some(signal)) => format!("シグナル {signal} で終了"),
        (None, None) => "異常終了".to_owned(),
    }
}

#[cfg(not(unix))]
fn describe(status: &std::process::ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("終了コード {code}"),
        None => "異常終了".to_owned(),
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
