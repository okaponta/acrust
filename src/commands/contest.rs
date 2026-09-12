//! `acrust new` and `acrust fetch`.
//!
//! Both bring the local copy of a contest up to date; the only difference is
//! whether creating the package is allowed.
//!
//! Nothing that already exists is destroyed. `src/bin/*.rs` holds solutions and
//! is never overwritten, `Cargo.toml` only gains the entries it is missing (via
//! `toml_edit`, so comments and formatting survive), and test suites keep the
//! cases and the `match` mode the user edited by hand.

use crate::atcoder::scrape::{self, ProblemPage, TaskEntry};
use crate::atcoder::AtCoderClient;
use crate::config::LoadedConfig;
use crate::package::{self, ProblemSpec, Written};
use crate::testcases::{FloatTolerance, Matching, TestCase, TestSuite};
use crate::ui;
use crate::workspace::Package;
use anyhow::{bail, Context as _, Result};
use std::collections::BTreeMap;
use std::path::Path;

pub fn new(contest: &str) -> Result<()> {
    sync(contest, true, false)
}

pub fn fetch(contest: Option<String>, overwrite: bool) -> Result<()> {
    let contest = match contest {
        Some(contest) => contest,
        None => {
            Package::find()
                .context("name a contest (you can omit it inside a package)")?
                .contest
        }
    };
    sync(&contest, false, overwrite)
}

fn sync(contest: &str, may_create: bool, overwrite: bool) -> Result<()> {
    let config = LoadedConfig::find()?;
    let package_dir = config.contest_dir(contest);
    if !may_create && !package_dir.is_dir() {
        bail!(
            "{} does not exist. Run `acrust new {contest}` first",
            package_dir.display()
        );
    }

    let client = AtCoderClient::new(&config.config.atcoder)?;
    // Past contests are readable while logged out; use the session if there is one.
    let _ = client.load_session();

    let (entries, pages) = fetch_contest(&client, contest, &config)?;
    let problems = problem_specs(&entries);
    if problems.is_empty() {
        bail!("could not work out a single problem for {contest}");
    }

    let mut written = Written::default();
    write_package(&config, contest, &package_dir, &problems, &mut written)?;
    let suites = write_testcases(
        &config,
        contest,
        &package_dir,
        &problems,
        &entries,
        &pages,
        overwrite,
    )?;

    report(contest, &package_dir, &problems, &pages, &suites, &written);
    Ok(())
}

/// One contest in two requests: `/tasks` and `/tasks_print`.
fn fetch_contest(
    client: &AtCoderClient,
    contest: &str,
    config: &LoadedConfig,
) -> Result<(Vec<TaskEntry>, BTreeMap<String, ProblemPage>)> {
    let tasks_url = format!("https://atcoder.jp/contests/{contest}/tasks");
    let response = client.get(&tasks_url)?;

    if response.status.as_u16() == 404 {
        // Tell "not started yet" apart from "no such contest", and create nothing
        // either way. A skeleton written before the problem URLs and samples exist
        // buys nothing: `new` has to be run again once the contest opens.
        let top = client.get(&format!("https://atcoder.jp/contests/{contest}"))?;
        if top.status.is_success() {
            bail!("contest {contest} has not started yet");
        }
        bail!("no such contest: {contest}");
    }
    response.error_for_status()?;

    let entries = scrape::parse_task_list(&response.body, contest)?;
    if entries.is_empty() {
        bail!("{tasks_url} lists no problems");
    }
    ui::arrow(&format!("{contest}: {} problems", entries.len()));

    let print_url = format!("https://atcoder.jp/contests/{contest}/tasks_print");
    let printed = client.get(&print_url)?;
    let pages = if printed.status.is_success() {
        match scrape::parse_tasks_print(&printed.body) {
            Ok(pages) => match_pages(&entries, pages),
            Err(e) => {
                ui::warn(&format!("could not parse {print_url}: {e}"));
                fetch_each_task(client, contest, &entries)?
            }
        }
    } else {
        ui::warn(&format!(
            "{print_url} returned {}. Falling back to one request per problem",
            printed.status
        ));
        fetch_each_task(client, contest, &entries)?
    };

    let missing: Vec<&str> = entries
        .iter()
        .filter(|e| !pages.contains_key(&e.alias))
        .map(|e| e.label.as_str())
        .collect();
    if !missing.is_empty() {
        ui::warn(&format!(
            "could not get the samples for: {}",
            missing.join(", ")
        ));
    }
    let _ = config;
    Ok((entries, pages))
}

/// Lines the problems of `tasks_print` up with the rows of `/tasks`, by heading
/// label (`A`, `Ex`, `001`) where they agree and by position where they do not.
fn match_pages(entries: &[TaskEntry], pages: Vec<ProblemPage>) -> BTreeMap<String, ProblemPage> {
    let by_label: BTreeMap<&str, &TaskEntry> =
        entries.iter().map(|e| (e.label.as_str(), e)).collect();

    let all_labels_known = pages
        .iter()
        .all(|page| by_label.contains_key(page.label.as_str()));

    if all_labels_known {
        return pages
            .into_iter()
            .filter_map(|page| {
                let entry = by_label.get(page.label.as_str())?;
                Some((entry.alias.clone(), page))
            })
            .collect();
    }

    // Falling back to position is safe: the two pages do list in the same order.
    ui::warn("the headings do not match the problem list, so matching them in order of appearance");
    entries
        .iter()
        .zip(pages)
        .map(|(entry, page)| (entry.alias.clone(), page))
        .collect()
}

/// The slow path for contests whose `tasks_print` is unusable. The client spaces
/// the requests out on its own.
fn fetch_each_task(
    client: &AtCoderClient,
    contest: &str,
    entries: &[TaskEntry],
) -> Result<BTreeMap<String, ProblemPage>> {
    let mut pages = BTreeMap::new();
    for entry in entries {
        let url = format!(
            "https://atcoder.jp/contests/{contest}/tasks/{}",
            entry.screen_name
        );
        let response = client.get(&url)?;
        if !response.status.is_success() {
            ui::warn(&format!("{url} returned {}", response.status));
            continue;
        }
        match scrape::parse_task_page(&response.body) {
            Ok(page) => {
                pages.insert(entry.alias.clone(), page);
            }
            Err(e) => ui::warn(&format!("could not parse {url}: {e}")),
        }
    }
    Ok(pages)
}

fn problem_specs(entries: &[TaskEntry]) -> Vec<ProblemSpec> {
    entries
        .iter()
        .map(|entry| ProblemSpec {
            alias: entry.alias.clone(),
            screen_name: Some(entry.screen_name.clone()),
        })
        .collect()
}

fn write_package(
    config: &LoadedConfig,
    contest: &str,
    package_dir: &Path,
    problems: &[ProblemSpec],
    written: &mut Written,
) -> Result<()> {
    let manifest_path = package::manifest_path(package_dir);
    if manifest_path.is_file() {
        if crate::manifest::merge(&manifest_path, contest, problems)? {
            written.updated.push("Cargo.toml".to_owned());
        } else {
            written.kept.push("Cargo.toml".to_owned());
        }
    } else {
        let manifest = package::render_manifest(
            contest,
            problems,
            &config.config.package.edition,
            &config.config.package.profile,
            &package::read_dependencies(config)?,
        )?;
        package::write_new_file(&manifest_path, &manifest)?;
        written.created.push("Cargo.toml".to_owned());
    }

    let template = package::read_template_source(config)?;
    package::write_sources(package_dir, problems, &template, written)?;
    package::copy_cargo_lock(config, package_dir, written)?;
    package::copy_template_dir(&config.template_copy_dir(), package_dir, written)?;
    Ok(())
}

/// Writes each problem's test suite, keeping hand-added cases and a hand-picked
/// `match` mode.
fn write_testcases(
    config: &LoadedConfig,
    contest: &str,
    package_dir: &Path,
    problems: &[ProblemSpec],
    entries: &[TaskEntry],
    pages: &BTreeMap<String, ProblemPage>,
    overwrite: bool,
) -> Result<BTreeMap<String, TestSuite>> {
    let package_rel = package::relative(&config.root, package_dir);
    let timelimits: BTreeMap<&str, Option<u64>> = entries
        .iter()
        .map(|e| (e.alias.as_str(), e.timelimit_ms))
        .collect();

    let mut suites = BTreeMap::new();
    for problem in problems {
        let Some(page) = pages.get(&problem.alias) else {
            continue;
        };
        let timelimit_ms = page
            .timelimit_ms
            .or_else(|| timelimits.get(problem.alias.as_str()).copied().flatten());

        let mut suite = suite_for(page, timelimit_ms);
        let path = config.testcases_path(&package_rel, &problem.alias);
        if !overwrite && path.is_file() {
            let existing = TestSuite::load(&path)?;
            suite = merge_suite(existing, suite);
        }
        suite
            .save(&path)
            .with_context(|| format!("could not write {} for {contest}", problem.alias))?;
        suites.insert(problem.alias.clone(), suite);
    }
    Ok(suites)
}

fn suite_for(page: &ProblemPage, timelimit_ms: Option<u64>) -> TestSuite {
    if page.interactive {
        // Interactive problems have no sample cases to compare against.
        return TestSuite::interactive(timelimit_ms);
    }

    let mut suite = TestSuite::batch(timelimit_ms);
    if let Some(float) = page.float {
        suite.matching = Matching::Float;
        suite.float = Some(FloatTolerance {
            relative_error: float.relative,
            absolute_error: float.absolute,
        });
    }
    suite.cases = page
        .samples
        .iter()
        .enumerate()
        .map(|(i, sample)| TestCase {
            name: format!("sample{}", i + 1),
            input: sample.input.clone(),
            output: sample.output.clone(),
        })
        .collect();
    suite
}

/// Merges a re-fetch into what is already on disk, so a fetch never undoes an
/// edit.
///
/// `match` and `[float]` stay as they are: a problem that accepts several answers
/// is switched to `words` by hand, and nothing in the statement says so. Cases
/// that are not samples are kept at the end. `type` and `timelimit` are taken
/// from the fresh copy.
fn merge_suite(existing: TestSuite, fresh: TestSuite) -> TestSuite {
    let fresh_names: Vec<&str> = fresh.cases.iter().map(|c| c.name.as_str()).collect();
    let extra: Vec<TestCase> = existing
        .cases
        .iter()
        .filter(|case| !fresh_names.contains(&case.name.as_str()))
        .cloned()
        .collect();

    let mut merged = fresh;
    merged.matching = existing.matching;
    merged.float = existing.float;
    merged.cases.extend(extra);
    merged
}

fn report(
    contest: &str,
    package_dir: &Path,
    problems: &[ProblemSpec],
    pages: &BTreeMap<String, ProblemPage>,
    suites: &BTreeMap<String, TestSuite>,
    written: &Written,
) {
    ui::info("");
    for problem in problems {
        let detail = match (pages.get(&problem.alias), suites.get(&problem.alias)) {
            (Some(page), Some(_)) if page.interactive => {
                format!("{} - interactive (no sample tests)", page.title)
            }
            (Some(page), Some(suite)) => {
                let float = match suite.float {
                    Some(_) => " / float judge",
                    None => "",
                };
                format!("{} - {} cases{float}", page.title, suite.cases.len())
            }
            _ => "no samples".to_owned(),
        };
        ui::field(&problem.alias, &detail);
    }

    ui::info("");
    if written.is_empty() {
        ui::ok(&format!(
            "{contest}: nothing changed ({})",
            package_dir.display()
        ));
    } else {
        ui::ok(&format!(
            "{contest}: {} created / {} updated / {} left alone ({})",
            written.created.len(),
            written.updated.len(),
            written.kept.len(),
            package_dir.display()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testcases::SuiteKind;

    fn case(name: &str, output: &str) -> TestCase {
        TestCase {
            name: name.to_owned(),
            input: "1\n".to_owned(),
            output: output.to_owned(),
        }
    }

    #[test]
    fn merging_keeps_hand_written_cases_and_the_hand_picked_match() {
        let mut existing = TestSuite::batch(Some(2000));
        existing.matching = Matching::Words;
        existing.cases = vec![case("sample1", "old\n"), case("mycase", "custom\n")];

        let mut fresh = TestSuite::batch(Some(3000));
        fresh.cases = vec![case("sample1", "new\n"), case("sample2", "new2\n")];

        let merged = merge_suite(existing, fresh);
        // taken from the fresh copy
        assert_eq!(merged.timelimit.as_deref(), Some("3s"));
        assert_eq!(merged.cases[0].output, "new\n");
        assert_eq!(merged.cases[1].name, "sample2");
        // kept from what was on disk
        assert_eq!(merged.matching, Matching::Words);
        assert_eq!(merged.cases[2].name, "mycase");
        assert_eq!(merged.cases.len(), 3);
    }

    #[test]
    fn an_interactive_page_becomes_a_suite_without_cases() {
        let page = ProblemPage {
            label: "C".to_owned(),
            title: "Yamanote Line Game".to_owned(),
            timelimit_ms: Some(2000),
            samples: Vec::new(),
            interactive: true,
            float: None,
        };
        let suite = suite_for(&page, Some(2000));
        assert_eq!(suite.kind, SuiteKind::Interactive);
        assert!(suite.cases.is_empty());
    }

    #[test]
    fn a_float_page_carries_the_tolerance_the_statement_gave() {
        let page = ProblemPage {
            label: "B".to_owned(),
            title: "You're a teapot".to_owned(),
            timelimit_ms: Some(2000),
            samples: vec![scrape::Sample {
                input: "attitude\n".to_owned(),
                output: "0.5\n".to_owned(),
            }],
            interactive: false,
            float: Some(scrape::FloatTolerance {
                relative: None,
                absolute: Some(1e-9),
            }),
        };
        let suite = suite_for(&page, Some(2000));
        assert_eq!(suite.matching, Matching::Float);
        assert_eq!(suite.float.unwrap().absolute_error, Some(1e-9));
        assert_eq!(suite.float.unwrap().relative_error, None);
        assert_eq!(suite.cases[0].name, "sample1");
    }
}
