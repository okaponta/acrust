//! ランナーのテスト（設計 §4.7）。
//!
//! `rustc` で小さな被験プログラムを1つ作り、入力で振る舞いを変えて
//! AC / WA / RE / TLE と、出力が多いときの挙動を確かめる。

use acrust::judge::Verdict;
use acrust::runner;
use acrust::testcases::{Matching, TestCase};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

/// 入力の1行目で振る舞いを変える被験プログラム。
const SUBJECT: &str = r#"
use std::io::{Read, Write};

fn main() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).unwrap();
    match input.lines().next().unwrap_or("") {
        "ok" => println!("expected"),
        "wrong" => println!("different"),
        "panic" => panic!("boom"),
        "loop" => loop { std::hint::black_box(0); },
        // 標準出力を読まずに待つと、パイプが詰まって止まる。
        "big" => {
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            for _ in 0..200_000 {
                writeln!(out, "0123456789012345678901234567890123456789").unwrap();
            }
        }
        "noisy" => {
            eprintln!("警告らしきもの");
            println!("expected");
        }
        other => println!("{other}"),
    }
}
"#;

/// テストを直列化する。
///
/// 各テストが `rustc` を起動し、そのうち1つは実行時間を測る。並列に走らせると
/// 計測がマシンの busy さに引きずられて、コードは正しいのにテストが落ちる。
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    // 他のテストが失敗して毒されていても、このロックの意味は変わらない。
    SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct Subject {
    dir: PathBuf,
    executable: PathBuf,
}

impl Drop for Subject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn subject() -> Subject {
    let dir = std::env::temp_dir().join(format!(
        "acrust-runner-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("subject.rs");
    std::fs::write(&source, SUBJECT).unwrap();
    let executable = dir.join("subject");

    let status = Command::new("rustc")
        .arg("-O")
        .arg("--edition=2021")
        .arg("-o")
        .arg(&executable)
        .arg(&source)
        .status()
        .expect("rustc を起動できること");
    assert!(status.success(), "被験プログラムのビルドに失敗した");

    Subject { dir, executable }
}

fn case(name: &str, input: &str, output: &str) -> TestCase {
    TestCase {
        name: name.to_owned(),
        input: format!("{input}\n"),
        output: output.to_owned(),
    }
}

#[test]
fn each_verdict_is_reported_for_the_right_reason() {
    let _serial = serial();
    let subject = subject();
    let cases = vec![
        case("ac", "ok", "expected\n"),
        case("wa", "wrong", "expected\n"),
        case("re", "panic", "expected\n"),
        case("tle", "loop", "expected\n"),
    ];

    let outcomes = runner::run_cases(
        &subject.executable,
        &cases,
        Duration::from_millis(600),
        4,
        Matching::Lines,
        None,
    );

    let verdicts: Vec<Verdict> = outcomes.iter().map(|o| o.verdict).collect();
    assert_eq!(
        verdicts,
        [
            Verdict::Accepted,
            Verdict::WrongAnswer,
            Verdict::RuntimeError,
            Verdict::TimeLimitExceeded
        ]
    );

    // 結果は実行順ではなくテストケースの順で返る。
    let names: Vec<&str> = outcomes.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(names, ["ac", "wa", "re", "tle"]);

    // RE では終了状態とパニックの内容が見える。
    assert!(outcomes[2].status.is_some());
    assert!(
        outcomes[2].stderr.contains("boom"),
        "{}",
        outcomes[2].stderr
    );

    // TLE は打ち切りまで待つが、待ちすぎない。
    assert!(outcomes[3].elapsed >= Duration::from_millis(600));
    assert!(outcomes[3].elapsed < Duration::from_millis(2000));
}

#[test]
fn a_program_that_writes_a_lot_does_not_look_like_a_timeout() {
    let _serial = serial();
    // 標準出力を読まずに待つ実装だと、パイプが詰まってここが TLE になる。
    let subject = subject();
    let cases = vec![case("big", "big", "")];
    let outcomes = runner::run_cases(
        &subject.executable,
        &cases,
        Duration::from_secs(20),
        1,
        Matching::Lines,
        None,
    );
    assert_eq!(
        outcomes[0].verdict,
        Verdict::WrongAnswer,
        "TLE ではないこと"
    );
    assert!(
        outcomes[0].stdout.len() > 8_000_000,
        "出力が全部取れていること"
    );
}

#[test]
fn stderr_is_kept_even_when_the_case_passes() {
    let _serial = serial();
    let subject = subject();
    let cases = vec![case("noisy", "noisy", "expected\n")];
    let outcomes = runner::run_cases(
        &subject.executable,
        &cases,
        Duration::from_secs(5),
        1,
        Matching::Lines,
        None,
    );
    assert_eq!(outcomes[0].verdict, Verdict::Accepted);
    assert!(outcomes[0].stderr.contains("警告らしきもの"));
}

/// 同じ内容を別のパスに置いた、まだ一度も実行していない実行ファイル。
///
/// macOS の初回実行コストはパス（inode）ごとに掛かるので、コピーすれば冷えた状態に戻せる。
fn cold_copy(subject: &Subject, name: &str) -> PathBuf {
    let copy = subject.dir.join(name);
    std::fs::copy(&subject.executable, &copy).unwrap();
    copy
}

fn time_one_run(executable: &std::path::Path) -> Duration {
    let started = std::time::Instant::now();
    let mut child = Command::new(executable)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    child.wait().unwrap();
    started.elapsed()
}

#[test]
fn timing_is_not_inflated_by_the_first_execution() {
    let _serial = serial();
    // ビルドし直した直後の初回実行は macOS で 300ms 以上かかる（署名の検証とページイン）。
    // ウォームアップを入れていないと、その値が1回目の計測にそのまま載る。
    let subject = subject();
    let cold = cold_copy(&subject, "subject-cold");

    let cases: Vec<TestCase> = (0..4)
        .map(|i| case(&format!("case{i}"), "ok", "expected\n"))
        .collect();
    let outcomes = runner::run_cases(
        &cold,
        &cases,
        Duration::from_secs(5),
        4,
        Matching::Lines,
        None,
    );
    assert!(outcomes.iter().all(|o| o.verdict == Verdict::Accepted));
    let measured = outcomes
        .iter()
        .map(|o| o.elapsed)
        .min()
        .expect("ケースがある");

    // ここまで来れば同じ実行ファイルは確実に温まっている。
    // 同じ負荷の下で測り直した値を基準にすれば、マシンの busy さに左右されない。
    let reference = time_one_run(&cold).min(time_one_run(&cold));
    assert!(
        measured <= reference * 4 + Duration::from_millis(80),
        "計測値 {measured:?} が、温まった状態の {reference:?} に比べて大きすぎる。\
         ウォームアップが効いていない"
    );
}
