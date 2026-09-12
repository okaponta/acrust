//! Runner tests.
//!
//! `rustc` builds one small subject program whose behaviour follows its input,
//! which covers AC / WA / RE / TLE and what happens when a solution writes a lot.

use acrust::judge::Verdict;
use acrust::runner;
use acrust::testcases::{Matching, TestCase};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

/// The subject program. Its first line of input decides what it does.
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
        // Enough output to block on the pipe if nobody is draining it.
        "big" => {
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            for _ in 0..200_000 {
                writeln!(out, "0123456789012345678901234567890123456789").unwrap();
            }
        }
        "noisy" => {
            eprintln!("something that looks like a warning");
            println!("expected");
        }
        other => println!("{other}"),
    }
}
"#;

/// Serialises these tests.
///
/// Each one starts `rustc`, and one of them measures elapsed time. Run in
/// parallel, that measurement follows how busy the machine is and the test fails
/// on correct code.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    // A poisoned lock here means another test failed, not that this one can't run.
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
        .expect("rustc should be runnable");
    assert!(status.success(), "the subject program did not build");

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

    // Results come back in the order of the cases, not of the runs.
    let names: Vec<&str> = outcomes.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(names, ["ac", "wa", "re", "tle"]);

    // An RE shows both how it died and what it said.
    assert!(outcomes[2].status.is_some());
    assert!(
        outcomes[2].stderr.contains("boom"),
        "{}",
        outcomes[2].stderr
    );

    // A TLE waits for the cutoff, and not much longer.
    assert!(outcomes[3].elapsed >= Duration::from_millis(600));
    assert!(outcomes[3].elapsed < Duration::from_millis(2000));
}

#[test]
fn a_program_that_writes_a_lot_does_not_look_like_a_timeout() {
    let _serial = serial();
    // An implementation that waits without draining stdout reports this as TLE.
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
        "should not be a TLE"
    );
    assert!(
        outcomes[0].stdout.len() > 8_000_000,
        "the whole output should come back"
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
    assert!(outcomes[0]
        .stderr
        .contains("something that looks like a warning"));
}

/// The same binary at a path that has never been executed.
///
/// macOS pays the first-run cost per inode, so a copy is cold again.
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
    // The first run of a freshly built binary costs upwards of 300ms on macOS —
    // signature checking and paging in. Without the warm-up, that lands whole on
    // the first measurement.
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
        .expect("there is at least one case");

    // By now the binary is certainly warm. Measuring again under the same load
    // gives a baseline that does not depend on how busy the machine is.
    let reference = time_one_run(&cold).min(time_one_run(&cold));
    assert!(
        measured <= reference * 4 + Duration::from_millis(80),
        "measured {measured:?} against a warm {reference:?}: the warm-up is not working"
    );
}
