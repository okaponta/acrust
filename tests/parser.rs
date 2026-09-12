//! Parser tests.
//!
//! The fixtures are synthetic HTML copying only the structure of the real pages;
//! no AtCoder problem text is checked in. Parsing against real pages happens in
//! `tests/acceptance_abc418.rs`.

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
    assert_eq!(entries[0].title, "Alpha & Beta", "entities are not decoded");
    assert_eq!(entries[0].timelimit_ms, Some(2000));

    // C points at another contest's problem, as abc042's C points at arc058_a.
    assert_eq!(entries[2].label, "C");
    assert_eq!(entries[2].screen_name, "other999_a");
    assert_eq!(entries[2].timelimit_ms, Some(2500), "fractional time limit");

    // An Ex problem gets `ex` as its alias.
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
    assert!(err.contains("could not pull out the problem list"), "{err}");
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
    // Picking up the <pre> of the input-format section would make this 3.
    assert_eq!(a.samples.len(), 2);
    assert_eq!(a.samples[0].input, "3\n1 2 3\n");
    assert_eq!(
        a.samples[0].output, "6\n",
        "an explanatory <p> was dragged in"
    );
    assert_eq!(a.samples[1].input, "1\n5\n");
    assert_eq!(a.samples[1].output, "5\n");
    // Nothing from the lang-en half (999) leaked in.
    assert!(a.samples.iter().all(|s| !s.input.contains("999")));
    assert_eq!(a.timelimit_ms, Some(2000));
}

#[test]
fn an_error_bound_in_the_statement_becomes_a_float_judgement() {
    let b = &problems()[1];
    let float = b.float.expect("should be detected as float-judged");
    // The statement names only an absolute bound, so relative stays unset.
    assert_eq!(float.absolute, Some(1e-9));
    assert_eq!(float.relative, None);
    assert_eq!(b.timelimit_ms, Some(4000));
}

#[test]
fn an_interactive_problem_is_marked_and_has_no_pairs() {
    let c = &problems()[2];
    assert!(c.interactive);
    assert!(c.samples.is_empty(), "an interactive problem has no pairs");
    assert_eq!(c.timelimit_ms, Some(2500));
}

#[test]
fn an_old_problem_without_the_lang_wrapper_still_parses() {
    let ex = &problems()[3];
    assert_eq!(ex.samples.len(), 1);
    assert_eq!(ex.samples[0].input, "1 < 2\n", "entities are not decoded");
    assert_eq!(ex.samples[0].output, "Yes\n");
    assert!(!ex.interactive);
    assert!(
        ex.float.is_none(),
        "a < > in the statement is not an error bound"
    );
}

#[test]
fn an_english_only_problem_is_parsed_and_paired_by_number() {
    let problems = scrape::parse_tasks_print(&fixture("tasks_print_english.html")).unwrap();
    let a = &problems[0];
    // Case 2 comes first on the page; pairing is by number, not by order.
    assert_eq!(a.samples.len(), 2);
    assert_eq!(a.samples[0].input, "4\n");
    assert_eq!(a.samples[0].output, "4.0000000\n");
    assert_eq!(a.samples[1].input, "7\n");
    assert_eq!(a.samples[1].output, "7.0000000\n");

    // The English "absolute or relative error ... 10^{-6}" is read too.
    let float = a.float.expect("should be detected as float-judged");
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
    assert!(err.contains("could not pull out samples"), "{err}");
}
