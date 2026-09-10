//! パーサのテスト（設計 §6 の M2 要件）。
//!
//! fixture は **構造だけを写した合成 HTML**。AtCoder の問題文は入っていない（設計 §5.3）。
//! 実データに対する突き合わせは `tests/acceptance_abc418.rs`（`#[ignore]`）が行う。

use acrust::atcoder::scrape::{self, ProblemPage};

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn problems() -> Vec<ProblemPage> {
    scrape::parse_tasks_print(&fixture("tasks_print.html")).unwrap()
}

#[test]
fn the_task_list_keeps_screen_names_that_do_not_follow_the_naming_rule() {
    let entries = scrape::parse_task_list(&fixture("tasks.html"), "dummy001").unwrap();
    assert_eq!(entries.len(), 4);

    assert_eq!(entries[0].alias, "a");
    assert_eq!(entries[0].screen_name, "dummy001_a");
    assert_eq!(entries[0].title, "Alpha & Beta", "実体参照が戻っていない");
    assert_eq!(entries[0].timelimit_ms, Some(2000));

    // C が別コンテストの問題を指す（abc042 の C が arc058_a になる類）。
    assert_eq!(entries[2].label, "C");
    assert_eq!(entries[2].screen_name, "other999_a");
    assert_eq!(entries[2].timelimit_ms, Some(2500), "小数の制限時間");

    // Ex 問題は alias も ex になる。
    assert_eq!(entries[3].label, "Ex");
    assert_eq!(entries[3].alias, "ex");
    assert_eq!(entries[3].screen_name, "dummy001_h");
    assert_eq!(entries[3].timelimit_ms, Some(8000));
}

#[test]
fn a_missing_table_is_an_error_that_says_so() {
    let err = scrape::parse_task_list("<html><body>Sign In</body></html>", "dummy001")
        .unwrap_err()
        .to_string();
    assert!(err.contains("問題一覧を取り出せませんでした"), "{err}");
}

#[test]
fn every_problem_on_the_print_page_is_found() {
    let problems = problems();
    assert_eq!(problems.len(), 4);
    assert_eq!(problems[0].label, "A");
    assert_eq!(problems[0].title, "Alpha & Beta");
    assert_eq!(problems[3].label, "Ex");
    assert_eq!(problems[3].title, "Extra");
}

#[test]
fn samples_come_from_the_japanese_statement_and_ignore_the_input_format_block() {
    let problems = problems();
    let a = &problems[0];
    // 「入力」（書式の説明）の <pre> を拾ってしまうと 3 ケースになる。
    assert_eq!(a.samples.len(), 2);
    assert_eq!(a.samples[0].input, "3\n1 2 3\n");
    assert_eq!(a.samples[0].output, "6\n", "解説の <p> を巻き込んでいない");
    assert_eq!(a.samples[1].input, "1\n5\n");
    assert_eq!(a.samples[1].output, "5\n");
    // lang-en 側の値（999）が混ざっていないこと。
    assert!(a.samples.iter().all(|s| !s.input.contains("999")));
    assert_eq!(a.timelimit_ms, Some(2000));
}

#[test]
fn an_error_bound_in_the_statement_becomes_a_float_judgement() {
    let b = &problems()[1];
    let float = b.float.expect("誤差ジャッジとして検出されること");
    // 「絶対誤差が 10^{-9} 以下」なので、相対誤差は設定しない。
    assert_eq!(float.absolute, Some(1e-9));
    assert_eq!(float.relative, None);
    assert_eq!(b.timelimit_ms, Some(4000));
}

#[test]
fn an_interactive_problem_is_marked_and_has_no_pairs() {
    let c = &problems()[2];
    assert!(c.interactive);
    assert!(c.samples.is_empty(), "入出力例はペアにならない");
    assert_eq!(c.timelimit_ms, Some(2500));
}

#[test]
fn an_old_problem_without_the_lang_wrapper_still_parses() {
    let ex = &problems()[3];
    assert_eq!(ex.samples.len(), 1);
    assert_eq!(ex.samples[0].input, "1 < 2\n", "実体参照が戻っていない");
    assert_eq!(ex.samples[0].output, "Yes\n");
    assert!(!ex.interactive);
    assert!(ex.float.is_none(), "問題文の < > を誤差と読み違えていない");
}

#[test]
fn an_english_only_problem_is_parsed_and_paired_by_number() {
    let problems = scrape::parse_tasks_print(&fixture("tasks_print_english.html")).unwrap();
    let a = &problems[0];
    // 出現順は 2 が先だが、番号でペアにして並べ直す。
    assert_eq!(a.samples.len(), 2);
    assert_eq!(a.samples[0].input, "4\n");
    assert_eq!(a.samples[0].output, "4.0000000\n");
    assert_eq!(a.samples[1].input, "7\n");
    assert_eq!(a.samples[1].output, "7.0000000\n");

    // 英語の「absolute or relative error ... 10^{-6}」も拾う。
    let float = a.float.expect("誤差ジャッジ");
    assert_eq!(float.absolute, Some(1e-6));
    assert_eq!(float.relative, Some(1e-6));
    assert_eq!(a.timelimit_ms, Some(3000));
}

#[test]
fn a_single_task_page_parses_like_the_print_page() {
    let page = scrape::parse_task_page(&fixture("tasks_print.html")).unwrap();
    assert_eq!(page.label, "A");
    assert_eq!(page.samples.len(), 2);
}

#[test]
fn an_unrecognisable_page_is_an_error_that_says_so() {
    let err = scrape::parse_tasks_print("<html><body><p>404</p></body></html>")
        .unwrap_err()
        .to_string();
    assert!(err.contains("入出力例を1問も取り出せませんでした"), "{err}");
}
