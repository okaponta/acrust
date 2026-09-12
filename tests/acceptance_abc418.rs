//! M2 の受け入れテスト（設計 §6）。
//!
//! `~/repos/atcoder-rust/abc418/testcases/*.yml` は cargo-compete が 2025-08 に取得した
//! 実データで、そのまま正解データとして使える。acrust が同じ HTML から取り出した
//! 入出力例が **バイト単位で一致する**ことを確認する。
//!
//! `live` feature が付いていないとビルドもされない。リポジトリ外のファイルに依存する
//! ためで（CI では走らない）、AtCoder の問題文をこのリポジトリに持ち込まないための
//! 措置でもある（設計 §5.3）。
//!
//! ```console
//! $ cargo test --features live --test acceptance_abc418 -- --nocapture
//! ```
//!
//! パスは `ACRUST_FIXTURES` / `ACRUST_REFERENCE_REPO` で差し替えられる。

use acrust::atcoder::scrape;
use acrust::testcases::{Matching, SuiteKind, TestSuite};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    std::env::var("ACRUST_FIXTURES")
        .unwrap_or_else(|_| "/Users/kohei/repos/personal/kyopro/acrust-fixtures".to_owned())
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

/// cargo-compete が書いた YAML から、ケース名と入出力だけを読む。
///
/// この形（ブロックスカラー `|` のみ）に特化した最小の読み取りで、汎用の YAML ではない。
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
            // ブロックスカラー `|` は末尾の空行を1つの改行に畳む。
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
            "skip: fixtures={} reference={} のどちらかがありません",
            fixtures.display(),
            reference.display()
        );
        return;
    }

    let tasks = std::fs::read_to_string(fixtures.join("abc418_tasks.html")).unwrap();
    let entries = scrape::parse_task_list(&tasks, "abc418").unwrap();
    let printed = std::fs::read_to_string(fixtures.join("abc418_tasks_print.html")).unwrap();
    let pages = scrape::parse_tasks_print(&printed).unwrap();
    assert_eq!(entries.len(), pages.len(), "問題数が一致しない");

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

        assert_eq!(kind, "Batch", "{}: 想定外の type", entry.alias);
        assert_eq!(
            page.samples.len(),
            expected.len(),
            "{}: ケース数が違う",
            entry.alias
        );

        for (i, (sample, (name, want_in, want_out))) in
            page.samples.iter().zip(&expected).enumerate()
        {
            assert_eq!(name, &format!("sample{}", i + 1), "{}: 名前", entry.alias);
            assert_eq!(
                &sample.input, want_in,
                "{} の {name} の入力が一致しない",
                entry.alias
            );
            assert_eq!(
                &sample.output, want_out,
                "{} の {name} の出力が一致しない",
                entry.alias
            );
            checked_cases += 1;
        }

        // 誤差ジャッジの検出結果も cargo-compete と一致すること。
        let expected_float = yaml.contains("Float");
        assert_eq!(
            page.float.is_some(),
            expected_float,
            "{}: 誤差ジャッジの判定が違う",
            entry.alias
        );
        if expected_float {
            let float = page.float.unwrap();
            assert_eq!(float.absolute, Some(1e-9), "{}: 絶対誤差", entry.alias);
            assert_eq!(float.relative, None, "{}: 相対誤差", entry.alias);
        }

        // 制限時間も一致すること（E は 4 秒、F は 3 秒）。
        let want_tl = yaml
            .lines()
            .find_map(|l| l.strip_prefix("timelimit: "))
            .map(|s| s.trim().to_owned());
        let got_tl = entry.timelimit_ms.map(acrust::testcases::format_duration);
        assert_eq!(got_tl, want_tl, "{}: 制限時間", entry.alias);
    }

    println!(
        "abc418: {} 問 / {checked_cases} ケースがバイト単位で一致",
        entries.len()
    );
    assert_eq!(checked_cases, 18);
}

/// 取り出したものを TOML にして読み直しても、1 バイトも変わらないこと。
#[test]
fn the_generated_toml_round_trips_the_real_samples() {
    let fixtures = fixtures();
    if !fixtures.is_dir() {
        eprintln!("skip: {} がありません", fixtures.display());
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
            assert_eq!(got.input, want.input, "{} の {}", page.label, want.name);
            assert_eq!(got.output, want.output, "{} の {}", page.label, want.name);
        }
    }
}
