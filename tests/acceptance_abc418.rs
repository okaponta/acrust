//! Acceptance test against real pages.
//!
//! `abc418/testcases/*.yml` in a cargo-compete repository was fetched from
//! AtCoder in 2025-08 and serves as the answer key: what acrust reads out of the
//! same HTML has to match it byte for byte.
//!
//! Not compiled without the `live` feature, because it reads files from outside
//! the repository — which is also how AtCoder's problem statements stay out of it.
//!
//! ```console
//! $ cargo test --features live --test acceptance_abc418 -- --nocapture
//! ```
//!
//! `ACRUST_FIXTURES` and `ACRUST_REFERENCE_REPO` override where it looks.

use acrust::atcoder::scrape;
use acrust::testcases::{Matching, SuiteKind, TestSuite};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    std::env::var("ACRUST_FIXTURES")
        .unwrap_or_else(|_| "/path/to/acrust-fixtures".to_owned())
        .into()
}

fn reference_repo() -> PathBuf {
    std::env::var("ACRUST_REFERENCE_REPO")
        .unwrap_or_else(|_| {
            format!(
                "{}/repos/atcoder-rust",
                std::env::var("HOME").unwrap_or_default()
            )
        })
        .into()
}

/// Reads case names and data out of the YAML cargo-compete wrote.
///
/// The least parser that handles this one shape (block scalars only). Not YAML.
fn parse_reference_yaml(text: &str) -> (String, Vec<(String, String, String)>) {
    let mut kind = String::new();
    let mut cases: Vec<(String, String, String)> = Vec::new();
    let mut current: Option<(String, Option<String>, Option<String>)> = None;
    let mut lines = text.lines().peekable();

    while let Some(line) = lines.next() {
        if let Some(rest) = line.strip_prefix("type: ") {
            kind = rest.trim().to_owned();
            continue;
        }
        if line.starts_with("extend:") {
            break;
        }
        if let Some(rest) = line.strip_prefix("  - name: ") {
            if let Some((name, input, output)) = current.take() {
                cases.push((name, input.unwrap_or_default(), output.unwrap_or_default()));
            }
            current = Some((rest.trim().to_owned(), None, None));
            continue;
        }
        for (prefix, slot) in [("    in: |", 1usize), ("    out: |", 2usize)] {
            if line.trim_end() != prefix {
                continue;
            }
            let mut block = String::new();
            while let Some(next) = lines.peek() {
                if !next.starts_with("      ") && !next.trim().is_empty() {
                    break;
                }
                let next = lines.next().expect("peeked");
                block.push_str(next.strip_prefix("      ").unwrap_or(""));
                block.push('\n');
            }
            // A `|` block scalar collapses its trailing blank lines to one.
            let block = format!("{}\n", block.trim_end_matches('\n'));
            if let Some(case) = current.as_mut() {
                if slot == 1 {
                    case.1 = Some(block);
                } else {
                    case.2 = Some(block);
                }
            }
        }
    }
    if let Some((name, input, output)) = current.take() {
        cases.push((name, input.unwrap_or_default(), output.unwrap_or_default()));
    }
    (kind, cases)
}

#[test]
fn the_samples_match_cargo_competes_own_output_byte_for_byte() {
    let fixtures = fixtures();
    let reference = reference_repo().join("abc418/testcases");
    if !fixtures.is_dir() || !reference.is_dir() {
        eprintln!(
            "skip: need both fixtures={} and reference={}",
            fixtures.display(),
            reference.display()
        );
        return;
    }

    let tasks = std::fs::read_to_string(fixtures.join("abc418_tasks.html")).unwrap();
    let entries = scrape::parse_task_list(&tasks, "abc418").unwrap();
    let printed = std::fs::read_to_string(fixtures.join("abc418_tasks_print.html")).unwrap();
    let pages = scrape::parse_tasks_print(&printed).unwrap();
    assert_eq!(entries.len(), pages.len(), "different number of problems");

    let by_alias: BTreeMap<&str, &scrape::ProblemPage> = entries
        .iter()
        .zip(&pages)
        .map(|(entry, page)| (entry.alias.as_str(), page))
        .collect();

    let mut checked_cases = 0;
    for entry in &entries {
        let page = by_alias[entry.alias.as_str()];
        let yaml = std::fs::read_to_string(reference.join(format!("{}.yml", entry.alias))).unwrap();
        let (kind, expected) = parse_reference_yaml(&yaml);

        assert_eq!(kind, "Batch", "{}: unexpected type", entry.alias);
        assert_eq!(
            page.samples.len(),
            expected.len(),
            "{}: different number of cases",
            entry.alias
        );

        for (i, (sample, (name, want_in, want_out))) in
            page.samples.iter().zip(&expected).enumerate()
        {
            assert_eq!(name, &format!("sample{}", i + 1), "{}: name", entry.alias);
            assert_eq!(
                &sample.input, want_in,
                "{}: input of {name} differs",
                entry.alias
            );
            assert_eq!(
                &sample.output, want_out,
                "{}: output of {name} differs",
                entry.alias
            );
            checked_cases += 1;
        }

        // Float judging has to be detected exactly where cargo-compete found it.
        let expected_float = yaml.contains("Float");
        assert_eq!(
            page.float.is_some(),
            expected_float,
            "{}: disagrees about float judging",
            entry.alias
        );
        if expected_float {
            let float = page.float.unwrap();
            assert_eq!(float.absolute, Some(1e-9), "{}: absolute", entry.alias);
            assert_eq!(float.relative, None, "{}: relative", entry.alias);
        }

        // And the time limits: E is 4 seconds here, F is 3.
        let want_tl = yaml
            .lines()
            .find_map(|l| l.strip_prefix("timelimit: "))
            .map(|s| s.trim().to_owned());
        let got_tl = entry.timelimit_ms.map(acrust::testcases::format_duration);
        assert_eq!(got_tl, want_tl, "{}: time limit", entry.alias);
    }

    println!(
        "abc418: {} problems / {checked_cases} cases match byte for byte",
        entries.len()
    );
    assert_eq!(checked_cases, 18);
}

/// Writing what was scraped as TOML and reading it back changes not one byte.
#[test]
fn the_generated_toml_round_trips_the_real_samples() {
    let fixtures = fixtures();
    if !fixtures.is_dir() {
        eprintln!("skip: {} is not there", fixtures.display());
        return;
    }
    let printed = std::fs::read_to_string(fixtures.join("abc418_tasks_print.html")).unwrap();
    let pages = scrape::parse_tasks_print(&printed).unwrap();

    for page in &pages {
        let mut suite = TestSuite::batch(page.timelimit_ms);
        if page.float.is_some() {
            suite.matching = Matching::Float;
        }
        suite.cases = page
            .samples
            .iter()
            .enumerate()
            .map(|(i, sample)| acrust::testcases::TestCase {
                name: format!("sample{}", i + 1),
                input: sample.input.clone(),
                output: sample.output.clone(),
            })
            .collect();

        let text = suite.to_toml().unwrap();
        let parsed = TestSuite::parse(&text).unwrap();
        assert_eq!(parsed.kind, SuiteKind::Batch);
        assert_eq!(parsed.cases.len(), suite.cases.len(), "{}", page.label);
        for (got, want) in parsed.cases.iter().zip(&suite.cases) {
            assert_eq!(got.input, want.input, "{} {}", page.label, want.name);
            assert_eq!(got.output, want.output, "{} {}", page.label, want.name);
        }
    }
}
