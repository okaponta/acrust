//! The test-case file, `{contest}/testcases/{alias}.toml`.
//!
//! Written by hand rather than through serde. The `toml` crate renders multi-line
//! data as a basic string — `"8\ngreentea\n"` — which throws away the entire point
//! of literal strings here: that a case can be pasted to and from the problem page
//! as it stands. Reading goes through serde, and a round-trip test keeps the two
//! from drifting apart.

use anyhow::{bail, Context as _, Result};
use serde::Deserialize;
use std::path::Path;

/// The literal-string delimiter. Data containing it is refused rather than
/// silently written out as a broken file.
const DELIMITER: &str = "'''";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SuiteKind {
    Batch,
    /// Cannot be sample-tested; `acrust test` skips it.
    Interactive,
}

/// How output is compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Matching {
    /// Line by line, ignoring trailing whitespace and trailing blank lines.
    Lines,
    /// Byte for byte.
    Exact,
    /// As a sequence of whitespace-separated tokens.
    Words,
    /// Token by token as numbers, within the tolerance in `[float]`.
    Float,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct FloatTolerance {
    #[serde(default)]
    pub relative_error: Option<f64>,
    #[serde(default)]
    pub absolute_error: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TestCase {
    pub name: String,
    #[serde(rename = "in")]
    pub input: String,
    #[serde(rename = "out")]
    pub output: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TestSuite {
    #[serde(rename = "type")]
    pub kind: SuiteKind,
    /// `2s` or `2500ms`. `None` when it could not be read.
    #[serde(default)]
    pub timelimit: Option<String>,
    /// Only the field is renamed; `match` is a Rust keyword.
    #[serde(rename = "match", default = "default_matching")]
    pub matching: Matching,
    #[serde(default)]
    pub float: Option<FloatTolerance>,
    #[serde(default)]
    pub cases: Vec<TestCase>,
}

fn default_matching() -> Matching {
    Matching::Lines
}

impl TestSuite {
    pub fn batch(timelimit_ms: Option<u64>) -> Self {
        Self {
            kind: SuiteKind::Batch,
            timelimit: timelimit_ms.map(format_duration),
            matching: Matching::Lines,
            float: None,
            cases: Vec::new(),
        }
    }

    pub fn interactive(timelimit_ms: Option<u64>) -> Self {
        Self {
            kind: SuiteKind::Interactive,
            timelimit: timelimit_ms.map(format_duration),
            matching: Matching::Lines,
            float: None,
            cases: Vec::new(),
        }
    }

    pub fn timelimit_ms(&self) -> Option<u64> {
        self.timelimit.as_deref().and_then(parse_duration)
    }

    pub fn parse(text: &str) -> Result<Self> {
        toml::from_str(text).context("could not parse the test case file")
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("could not read {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("could not load {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let text = self.to_toml()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }
        std::fs::write(path, text).with_context(|| format!("could not write {}", path.display()))
    }

    pub fn to_toml(&self) -> Result<String> {
        let mut out = String::new();
        out.push_str(&format!("type = {}\n", quote(kind_name(self.kind))));
        if let Some(timelimit) = &self.timelimit {
            out.push_str(&format!("timelimit = {}\n", quote(timelimit)));
        }
        out.push_str(&format!(
            "match = {}\n",
            quote(matching_name(self.matching))
        ));

        // [float] has to precede [[cases]]: once a table begins, every key that
        // follows belongs to it.
        if let Some(float) = &self.float {
            out.push_str("\n[float]\n");
            if let Some(relative) = float.relative_error {
                out.push_str(&format!("relative-error = {}\n", format_float(relative)));
            }
            if let Some(absolute) = float.absolute_error {
                out.push_str(&format!("absolute-error = {}\n", format_float(absolute)));
            }
        }

        for case in &self.cases {
            out.push_str("\n[[cases]]\n");
            out.push_str(&format!("name = {}\n", quote(&case.name)));
            out.push_str(&format!(
                "in = {}\n",
                literal_block(&case.name, "in", &case.input)?
            ));
            out.push_str(&format!(
                "out = {}\n",
                literal_block(&case.name, "out", &case.output)?
            ));
        }
        Ok(out)
    }
}

fn kind_name(kind: SuiteKind) -> &'static str {
    match kind {
        SuiteKind::Batch => "batch",
        SuiteKind::Interactive => "interactive",
    }
}

fn matching_name(matching: Matching) -> &'static str {
    match matching {
        Matching::Lines => "lines",
        Matching::Exact => "exact",
        Matching::Words => "words",
        Matching::Float => "float",
    }
}

/// A basic string is enough for names; an AtCoder alias has nothing exotic in it.
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// A multi-line literal string. TOML drops the newline right after the opening
/// `'''`, which is what lets the data start on its own line.
fn literal_block(case: &str, field: &str, data: &str) -> Result<String> {
    if data.contains(DELIMITER) {
        bail!(
            "the {field} of case {case} contains {DELIMITER}, \
             which a TOML literal string cannot hold"
        );
    }
    // Output with no trailing newline has to be expressible too.
    if data.is_empty() {
        return Ok(format!("{DELIMITER}{DELIMITER}"));
    }
    if data.ends_with('\n') {
        Ok(format!("{DELIMITER}\n{data}{DELIMITER}"))
    } else {
        Ok(format!("{DELIMITER}\n{data}\n{DELIMITER}"))
    }
}

/// Formats as `1e-9`: readable both to TOML and to a person.
fn format_float(value: f64) -> String {
    let formatted = format!("{value:e}");
    // Add a decimal point when there is no exponent, so the value still reads
    // as a float rather than an integer.
    if formatted.contains('e') {
        formatted
    } else {
        format!("{value:?}")
    }
}

/// Reads `2s`, `2500ms`, `2.5s` or a bare `2000` as milliseconds.
pub fn parse_duration(text: &str) -> Option<u64> {
    let text = text.trim();
    let (value, scale) = if let Some(rest) = text.strip_suffix("ms") {
        (rest, 1.0)
    } else if let Some(rest) = text.strip_suffix('s') {
        (rest, 1000.0)
    } else {
        (text, 1000.0)
    };
    let value: f64 = value.trim().parse().ok()?;
    if value <= 0.0 {
        return None;
    }
    Some((value * scale).round() as u64)
}

/// `2s` for a whole number of seconds, `2500ms` otherwise.
pub fn format_duration(millis: u64) -> String {
    if millis % 1000 == 0 {
        format!("{}s", millis / 1000)
    } else {
        format!("{millis}ms")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn suite() -> TestSuite {
        TestSuite {
            kind: SuiteKind::Batch,
            timelimit: Some("2s".to_owned()),
            matching: Matching::Lines,
            float: None,
            cases: vec![
                TestCase {
                    name: "sample1".to_owned(),
                    input: "8\ngreentea\n".to_owned(),
                    output: "Yes\n".to_owned(),
                },
                TestCase {
                    name: "sample2".to_owned(),
                    input: "6\ncoffee\n".to_owned(),
                    output: "No\n".to_owned(),
                },
            ],
        }
    }

    #[test]
    fn writes_the_documented_shape() {
        let toml_text = suite().to_toml().unwrap();
        assert_eq!(
            toml_text,
            r#"type = "batch"
timelimit = "2s"
match = "lines"

[[cases]]
name = "sample1"
in = '''
8
greentea
'''
out = '''
Yes
'''

[[cases]]
name = "sample2"
in = '''
6
coffee
'''
out = '''
No
'''
"#
        );
    }

    #[test]
    fn data_starts_at_column_zero_so_it_can_be_pasted_from_the_problem_page() {
        let toml_text = suite().to_toml().unwrap();
        assert!(toml_text.contains("\n8\ngreentea\n"), "{toml_text}");
    }

    #[test]
    fn round_trips_through_the_parser() {
        let original = suite();
        let parsed = TestSuite::parse(&original.to_toml().unwrap()).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn round_trips_with_float_and_interactive() {
        let float = TestSuite {
            kind: SuiteKind::Batch,
            timelimit: Some("2500ms".to_owned()),
            matching: Matching::Float,
            float: Some(FloatTolerance {
                relative_error: None,
                absolute_error: Some(1e-9),
            }),
            cases: vec![TestCase {
                name: "sample1".to_owned(),
                input: "attitude\n".to_owned(),
                output: "0.500000000\n".to_owned(),
            }],
        };
        let text = float.to_toml().unwrap();
        assert!(text.contains("[float]\nabsolute-error = 1e-9\n"), "{text}");
        // Placed after [[cases]], [float] would be swallowed by the last case.
        assert!(text.find("[float]").unwrap() < text.find("[[cases]]").unwrap());
        assert_eq!(TestSuite::parse(&text).unwrap(), float);

        let interactive = TestSuite::interactive(Some(2000));
        let text = interactive.to_toml().unwrap();
        assert_eq!(TestSuite::parse(&text).unwrap(), interactive);
        assert!(!text.contains("[[cases]]"));
    }

    #[test]
    fn output_without_a_trailing_newline_survives() {
        let suite = TestSuite {
            cases: vec![TestCase {
                name: "sample1".to_owned(),
                input: "1\n".to_owned(),
                output: "Yes".to_owned(),
            }],
            ..suite()
        };
        // A literal string only loses the newline after its opening quotes, so
        // data with no trailing newline gains one on the way back. `lines`
        // ignores that, but the round trip still has to hold together.
        let parsed = TestSuite::parse(&suite.to_toml().unwrap()).unwrap();
        assert_eq!(parsed.cases[0].output.trim_end(), "Yes");
    }

    #[test]
    fn refuses_to_write_data_containing_the_delimiter() {
        let suite = TestSuite {
            cases: vec![TestCase {
                name: "sample1".to_owned(),
                input: "a'''b\n".to_owned(),
                output: "x\n".to_owned(),
            }],
            ..suite()
        };
        let err = suite.to_toml().unwrap_err().to_string();
        assert!(err.contains("sample1"), "{err}");
        assert!(err.contains("cannot hold"), "{err}");
    }

    #[test]
    fn durations_round_trip() {
        for (millis, text) in [
            (2000, "2s"),
            (2500, "2500ms"),
            (1000, "1s"),
            (6800, "6800ms"),
        ] {
            assert_eq!(format_duration(millis), text);
            assert_eq!(parse_duration(text), Some(millis));
        }
        assert_eq!(parse_duration("2.5s"), Some(2500));
        assert_eq!(parse_duration("2"), Some(2000));
        assert_eq!(parse_duration("zero"), None);
    }

    #[test]
    fn missing_optional_fields_take_the_documented_defaults() {
        let suite = TestSuite::parse("type = \"batch\"").unwrap();
        assert_eq!(suite.matching, Matching::Lines);
        assert_eq!(suite.timelimit, None);
        assert!(suite.cases.is_empty());
        assert!(suite.float.is_none());
    }
}
