//! Reading the cargo-compete / snowchains format, used only by `acrust migrate`.
//!
//! Not a general YAML parser: it reads the shapes snowchains actually writes, and
//! errors on any line it does not recognise. That refusal is the whole design —
//! a migration that silently drops a case it did not understand is worse than one
//! that stops. The shapes implemented are the ones counted across 2,716 real
//! files and 7,534 cases.

use crate::testcases::{FloatTolerance, Matching, SuiteKind, TestCase, TestSuite};
use anyhow::{anyhow, bail, Context as _, Result};
use std::collections::BTreeMap;

/// One entry of `[package.metadata.cargo-compete.bin]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinEntry {
    /// The bin name, e.g. `abc042-c`.
    pub name: String,
    /// `c`.
    pub alias: String,
    /// `https://atcoder.jp/contests/abc042/tasks/arc058_a`.
    pub problem: String,
}

impl BinEntry {
    /// Splits the problem URL into contest and task screen name.
    pub fn contest_and_task(&self) -> Result<(String, String)> {
        let rest = self
            .problem
            .strip_prefix("https://atcoder.jp/contests/")
            .ok_or_else(|| anyhow!("not an AtCoder problem URL: {}", self.problem))?;
        let (contest, rest) = rest
            .split_once('/')
            .ok_or_else(|| anyhow!("cannot make sense of the problem URL: {}", self.problem))?;
        let task = rest
            .strip_prefix("tasks/")
            .ok_or_else(|| anyhow!("cannot make sense of the problem URL: {}", self.problem))?;
        if contest.is_empty() || task.is_empty() || task.contains('/') {
            bail!("cannot make sense of the problem URL: {}", self.problem);
        }
        Ok((contest.to_owned(), task.to_owned()))
    }
}

pub fn parse_bins(manifest: &str) -> Result<Vec<BinEntry>> {
    let table: toml::Table = toml::from_str(manifest).context("could not read Cargo.toml")?;
    let bins = table
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("cargo-compete"))
        .and_then(|c| c.get("bin"))
        .and_then(|b| b.as_table());
    let Some(bins) = bins else {
        return Ok(Vec::new());
    };

    bins.iter()
        .map(|(name, value)| {
            let alias = value
                .get("alias")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("{name} has no alias"))?
                .to_owned();
            let problem = value
                .get("problem")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("{name} has no problem"))?
                .to_owned();
            Ok(BinEntry {
                name: name.clone(),
                alias,
                problem,
            })
        })
        .collect()
}

/// The parts of `compete.toml` the migration carries over.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompeteConfig {
    /// The solution template embedded in `[template] src`.
    pub template_src: Option<String>,
    /// `[template.new] dependencies`.
    pub dependencies: Option<String>,
    /// `[template.new] edition`.
    pub edition: Option<String>,
    /// The value in `[template.new.copy-files]`, i.e. `./template-cargo-lock.toml`.
    pub cargo_lock: Option<String>,
    /// `[submit] language_id`. Read only to report that it is not carried over.
    pub language_id: Option<String>,
}

pub fn parse_compete_config(text: &str) -> Result<CompeteConfig> {
    let table: toml::Table = toml::from_str(text).context("compete.toml is not valid TOML")?;
    let template = table.get("template");
    let new = template.and_then(|t| t.get("new"));

    let cargo_lock = new
        .and_then(|n| n.get("copy-files"))
        .and_then(|c| c.as_table())
        .and_then(|files| {
            files
                .iter()
                .find(|(_, destination)| destination.as_str() == Some("Cargo.lock"))
                .map(|(source, _)| source.clone())
        });

    Ok(CompeteConfig {
        template_src: template
            .and_then(|t| t.get("src"))
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        dependencies: new
            .and_then(|n| n.get("dependencies"))
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        edition: new
            .and_then(|n| n.get("edition"))
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        cargo_lock,
        language_id: table
            .get("submit")
            .and_then(|s| s.get("language_id"))
            .and_then(|v| v.as_str())
            .map(str::to_owned),
    })
}

pub fn parse_test_suite(yaml: &str) -> Result<TestSuite> {
    let mut lines = Lines::new(yaml);
    let mut kind = None;
    let mut timelimit = None;
    let mut matching = Matching::Lines;
    let mut float = None;
    let mut cases: Vec<TestCase> = Vec::new();

    while let Some(line) = lines.next() {
        let raw = line.text;
        let trimmed = raw.trim_end();
        if trimmed.is_empty() || trimmed == "---" {
            continue;
        }
        // Everything from `extend:` on is snowchains' own way of naming extra
        // cases, and acrust has nothing that corresponds to it.
        if trimmed == "extend:" {
            break;
        }

        if let Some(value) = trimmed.strip_prefix("type: ") {
            kind = Some(match value.trim() {
                "Batch" => SuiteKind::Batch,
                "Interactive" => SuiteKind::Interactive,
                other => bail!("unknown type: {other}"),
            });
        } else if let Some(value) = trimmed.strip_prefix("timelimit: ") {
            timelimit = parse_timelimit(value.trim())?;
        } else if let Some(value) = trimmed.strip_prefix("match: ") {
            matching = match value.trim() {
                "Lines" => Matching::Lines,
                "SplitWhitespace" => Matching::Words,
                "Exact" => Matching::Exact,
                other => bail!("unknown match: {other}"),
            };
        } else if trimmed == "match:" {
            let (m, f) = parse_match_block(&mut lines)?;
            matching = m;
            float = f;
        } else if trimmed == "cases:" {
            cases = parse_cases(&mut lines)?;
        } else if trimmed == "cases: []" || trimmed == "cases: ~" {
            cases = Vec::new();
        } else {
            bail!("cannot make sense of line {}: {trimmed}", line.number);
        }
    }

    let kind = kind.context("no type")?;
    Ok(TestSuite {
        kind,
        timelimit,
        matching,
        float,
        cases,
    })
}

/// `2s`, `2s 500ms`, `500ms`, or `~` for none.
fn parse_timelimit(value: &str) -> Result<Option<String>> {
    if value == "~" {
        return Ok(None);
    }
    let mut millis = 0u64;
    for part in value.split_whitespace() {
        let (number, scale) = if let Some(rest) = part.strip_suffix("ms") {
            (rest, 1u64)
        } else if let Some(rest) = part.strip_suffix('s') {
            (rest, 1000)
        } else {
            bail!("unknown timelimit: {value}");
        };
        let number: u64 = number
            .parse()
            .with_context(|| format!("unknown timelimit: {value}"))?;
        millis += number * scale;
    }
    if millis == 0 {
        bail!("unknown timelimit: {value}");
    }
    Ok(Some(crate::testcases::format_duration(millis)))
}

/// ```yaml
/// match:
///   Float:
///     relative_error: 1e-6
///     absolute_error: ~
/// ```
fn parse_match_block(lines: &mut Lines) -> Result<(Matching, Option<FloatTolerance>)> {
    let header = lines.next().context("match: has no body")?;
    if header.text.trim() != "Float:" {
        bail!(
            "cannot make sense of line {}: {}",
            header.number,
            header.text.trim()
        );
    }
    let mut tolerance = FloatTolerance::default();
    while let Some(line) = lines.peek() {
        let trimmed = line.text.trim();
        let Some((key, value)) = trimmed.split_once(": ") else {
            break;
        };
        let slot = match key {
            "relative_error" => &mut tolerance.relative_error,
            "absolute_error" => &mut tolerance.absolute_error,
            _ => break,
        };
        *slot = parse_number(value.trim())
            .with_context(|| format!("could not read {key} on line {}", line.number))?;
        lines.next();
    }
    Ok((Matching::Float, Some(tolerance)))
}

fn parse_number(value: &str) -> Result<Option<f64>> {
    if value == "~" {
        return Ok(None);
    }
    Ok(Some(value.parse::<f64>()?))
}

fn parse_cases(lines: &mut Lines) -> Result<Vec<TestCase>> {
    let mut cases = Vec::new();
    while let Some(line) = lines.peek() {
        let trimmed = line.text.trim_end();
        if trimmed.is_empty() {
            lines.next();
            continue;
        }
        let Some(name) = trimmed.strip_prefix("  - name: ") else {
            break;
        };
        let name = name.trim().to_owned();
        lines.next();

        let mut input = None;
        let mut output = None;
        while let Some(line) = lines.peek() {
            let trimmed = line.text.trim_end();
            let slot = if trimmed.starts_with("    in:") {
                &mut input
            } else if trimmed.starts_with("    out:") {
                &mut output
            } else {
                break;
            };
            let number = line.number;
            lines.next();
            let value = trimmed
                .split_once(american_colon())
                .map(|(_, rest)| rest.trim().to_owned())
                .unwrap_or_default();
            *slot = Some(
                read_scalar(lines, &value, 4)
                    .with_context(|| format!("could not read the data on line {number}"))?,
            );
        }

        cases.push(TestCase {
            name,
            input: input.context("no in")?,
            output: output.context("no out")?,
        });
    }
    Ok(cases)
}

fn american_colon() -> char {
    ':'
}

/// Reads a `|` or `>` block scalar, or a double-quoted string.
fn read_scalar(lines: &mut Lines, marker: &str, key_indent: usize) -> Result<String> {
    match marker {
        "|" => Ok(read_block(lines, key_indent, false)),
        ">" => Ok(read_block(lines, key_indent, true)),
        quoted if quoted.starts_with('"') && quoted.ends_with('"') && quoted.len() >= 2 => {
            unescape(&quoted[1..quoted.len() - 1])
        }
        other => bail!("unknown way of writing the data: {other}"),
    }
}

/// The body of a block scalar. YAML's default chomping is `clip`, so the trailing
/// newlines collapse to one.
fn read_block(lines: &mut Lines, key_indent: usize, folded: bool) -> String {
    let indent = key_indent + 2;
    let mut collected: Vec<String> = Vec::new();
    while let Some(line) = lines.peek() {
        let text = line.text;
        if text.trim().is_empty() {
            // Whether a blank line is part of the body or the end of it is
            // decided by the line after it.
            let blank = text.to_owned();
            lines.next();
            if lines
                .peek()
                .is_some_and(|next| next.text.starts_with(&" ".repeat(indent)))
            {
                collected.push(blank.get(indent..).unwrap_or("").to_owned());
                continue;
            }
            break;
        }
        if !text.starts_with(&" ".repeat(indent)) {
            break;
        }
        collected.push(text[indent..].to_owned());
        lines.next();
    }

    if collected.is_empty() {
        return String::new();
    }
    if folded {
        // Folded: consecutive lines join with a space, a blank line is a newline.
        let mut out = String::new();
        for (i, line) in collected.iter().enumerate() {
            if i > 0 {
                out.push(if line.is_empty() || collected[i - 1].is_empty() {
                    '\n'
                } else {
                    ' '
                });
            }
            out.push_str(line);
        }
        return format!("{}\n", out.trim_end_matches('\n'));
    }
    format!("{}\n", collected.join("\n").trim_end_matches('\n'))
}

/// Undoes the escapes in a double-quoted string. Only `\n`, `\"` and `\\` occur.
fn unescape(text: &str) -> Result<String> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => bail!("unknown escape: \\{other}"),
            None => bail!("the escape is cut off"),
        }
    }
    Ok(out)
}

/// A line, and the number to name it by in an error.
struct Line<'a> {
    number: usize,
    text: &'a str,
}

struct Lines<'a> {
    lines: Vec<&'a str>,
    position: usize,
}

impl<'a> Lines<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            lines: text.lines().collect(),
            position: 0,
        }
    }

    fn peek(&self) -> Option<Line<'a>> {
        self.lines.get(self.position).map(|text| Line {
            number: self.position + 1,
            text,
        })
    }

    #[allow(clippy::should_implement_trait)]
    fn next(&mut self) -> Option<Line<'a>> {
        let line = self.peek()?;
        self.position += 1;
        Some(line)
    }
}

/// Rebuilds a problem URL from the metadata the migration is about to write,
/// which is how the conversion proves it lost nothing.
pub fn rebuild_problem_url(
    contest: &str,
    tasks: &BTreeMap<String, String>,
    alias: &str,
) -> Option<String> {
    Some(format!(
        "https://atcoder.jp/contests/{contest}/tasks/{}",
        tasks.get(alias)?
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_batch_suite_round_trips_into_the_acrust_model() {
        let yaml = "---\ntype: Batch\ntimelimit: 2s\nmatch: Lines\n\ncases:\n  - name: sample1\n    in: |\n      8\n      greentea\n    out: |\n      Yes\n  - name: sample2\n    in: |\n      6\n      coffee\n    out: |\n      No\n\nextend:\n  - type: Text\n    path: \"./a\"\n    in: /in/*.txt\n    out: /out/*.txt\n";
        let suite = parse_test_suite(yaml).unwrap();
        assert_eq!(suite.kind, SuiteKind::Batch);
        assert_eq!(suite.timelimit.as_deref(), Some("2s"));
        assert_eq!(suite.matching, Matching::Lines);
        assert_eq!(suite.cases.len(), 2);
        assert_eq!(suite.cases[0].name, "sample1");
        assert_eq!(suite.cases[0].input, "8\ngreentea\n");
        assert_eq!(suite.cases[0].output, "Yes\n");
        assert_eq!(suite.cases[1].input, "6\ncoffee\n");
        assert_eq!(suite.cases[1].output, "No\n");
    }

    #[test]
    fn a_float_suite_keeps_the_tolerance_including_the_missing_half() {
        let yaml = "---\ntype: Batch\ntimelimit: 2s\nmatch:\n  Float:\n    relative_error: ~\n    absolute_error: 1e-9\n\ncases:\n  - name: sample1\n    in: |\n      attitude\n    out: |\n      0.5\n";
        let suite = parse_test_suite(yaml).unwrap();
        assert_eq!(suite.matching, Matching::Float);
        let float = suite.float.unwrap();
        assert_eq!(float.relative_error, None);
        assert_eq!(float.absolute_error, Some(1e-9));
    }

    #[test]
    fn split_whitespace_becomes_words() {
        let yaml = "---\ntype: Batch\ntimelimit: 2s\nmatch: SplitWhitespace\n\ncases: []\n";
        assert_eq!(parse_test_suite(yaml).unwrap().matching, Matching::Words);
    }

    #[test]
    fn an_interactive_suite_has_no_cases() {
        let yaml = "---\ntype: Interactive\ntimelimit: 2s\n";
        let suite = parse_test_suite(yaml).unwrap();
        assert_eq!(suite.kind, SuiteKind::Interactive);
        assert!(suite.cases.is_empty());
    }

    #[test]
    fn a_missing_timelimit_and_empty_cases_are_understood() {
        let yaml = "---\ntype: Batch\ntimelimit: ~\nmatch: Lines\n\ncases: []\n\nextend:\n";
        let suite = parse_test_suite(yaml).unwrap();
        assert_eq!(suite.timelimit, None);
        assert!(suite.cases.is_empty());
    }

    #[test]
    fn fractional_time_limits_are_converted() {
        let yaml = "---\ntype: Batch\ntimelimit: 2s 500ms\nmatch: Lines\ncases: []\n";
        assert_eq!(
            parse_test_suite(yaml).unwrap().timelimit.as_deref(),
            Some("2500ms")
        );
    }

    #[test]
    fn quoted_scalars_and_empty_folded_blocks_are_understood() {
        let yaml = "---\ntype: Batch\ntimelimit: 2s\nmatch: Lines\n\ncases:\n  - name: sample1\n    in: \"5 0\\n\\n\"\n    out: >\n\nextend:\n";
        let suite = parse_test_suite(yaml).unwrap();
        assert_eq!(suite.cases[0].input, "5 0\n\n");
        assert_eq!(suite.cases[0].output, "", "an empty folded block is empty");
    }

    #[test]
    fn blank_lines_inside_a_block_are_kept() {
        let yaml = "---\ntype: Batch\ntimelimit: 2s\nmatch: Lines\n\ncases:\n  - name: sample1\n    in: |\n      1\n\n      2\n    out: |\n      ok\n";
        let suite = parse_test_suite(yaml).unwrap();
        assert_eq!(suite.cases[0].input, "1\n\n2\n");
    }

    #[test]
    fn an_unknown_line_is_an_error_rather_than_a_silent_loss() {
        let yaml = "---\ntype: Batch\ntimelimit: 2s\nmatch: Lines\nunexpected: value\n";
        let err = parse_test_suite(yaml).unwrap_err().to_string();
        assert!(err.contains("cannot make sense of"), "{err}");
        assert!(err.contains("unexpected"), "{err}");

        let yaml = "---\ntype: Batch\ntimelimit: 2 fortnights\n";
        assert!(parse_test_suite(yaml).is_err());

        let yaml = "---\ntype: Quantum\n";
        assert!(parse_test_suite(yaml).is_err());
    }

    #[test]
    fn bins_carry_the_problem_urls_that_cannot_be_derived() {
        let manifest = r#"
[package]
name = "abc042"

[package.metadata.cargo-compete.bin]
abc042-a = { alias = "a", problem = "https://atcoder.jp/contests/abc042/tasks/abc042_a" }
abc042-c = { alias = "c", problem = "https://atcoder.jp/contests/abc042/tasks/arc058_a" }
"#;
        let bins = parse_bins(manifest).unwrap();
        assert_eq!(bins.len(), 2);
        let c = bins.iter().find(|b| b.alias == "c").unwrap();
        assert_eq!(
            c.contest_and_task().unwrap(),
            ("abc042".into(), "arc058_a".into())
        );
    }

    #[test]
    fn the_rebuilt_url_matches_what_cargo_compete_had() {
        let tasks: BTreeMap<String, String> = [("c".to_owned(), "arc058_a".to_owned())]
            .into_iter()
            .collect();
        assert_eq!(
            rebuild_problem_url("abc042", &tasks, "c").as_deref(),
            Some("https://atcoder.jp/contests/abc042/tasks/arc058_a")
        );
        assert_eq!(rebuild_problem_url("abc042", &tasks, "z"), None);
    }

    #[test]
    fn the_compete_config_gives_up_its_template() {
        let text = r#"
test-suite = "{{ manifest_dir }}/testcases/{{ bin_alias }}.yml"

[template]
src = '''
fn main() {}
'''

[template.new]
edition = "2021"
dependencies = '''
proconio = "=0.4.5"
'''

[template.new.copy-files]
"./template-cargo-lock.toml" = "Cargo.lock"

[submit]
language_id = "5054"
"#;
        let config = parse_compete_config(text).unwrap();
        assert_eq!(config.template_src.as_deref(), Some("fn main() {}\n"));
        assert_eq!(config.edition.as_deref(), Some("2021"));
        assert_eq!(
            config.dependencies.as_deref(),
            Some("proconio = \"=0.4.5\"\n")
        );
        assert_eq!(
            config.cargo_lock.as_deref(),
            Some("./template-cargo-lock.toml")
        );
        assert_eq!(config.language_id.as_deref(), Some("5054"));
    }
}
