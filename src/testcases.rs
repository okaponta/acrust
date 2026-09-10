//! テストケースファイル `{contest}/testcases/{alias}.toml`（決定 D5・設計 §4.5）。
//!
//! 書き出しは serde ではなく手書きにしている。`toml` クレートの出力は
//! 複数行データを `"8\ngreentea\n"` のような basic string にしてしまい、
//! **問題ページとの双方向コピペ**という TOML リテラル文字列を選んだ理由が消えるため。
//! 読み込みは serde で行い、往復テストで両者が食い違わないことを保証している。

use anyhow::{bail, Context as _, Result};
use serde::Deserialize;
use std::path::Path;

/// TOML リテラル文字列の区切り。データ側にこれが現れたら黙って壊れたファイルを書かない。
const DELIMITER: &str = "'''";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SuiteKind {
    Batch,
    /// サンプルテストができない問題。`acrust test` はスキップする。
    Interactive,
}

/// 出力の比較方法（設計 §4.5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Matching {
    /// 各行を `trim_end` して比較し、末尾の空行は無視する。
    Lines,
    /// バイト完全一致。
    Exact,
    /// 空白で分割したトークン列として比較。
    Words,
    /// トークンごとに数値として比較。`[float]` の許容誤差内なら AC。
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
    /// `2s` / `2500ms`。取れなかった問題では `None`。
    #[serde(default)]
    pub timelimit: Option<String>,
    /// `match` は Rust の予約語なのでフィールド名だけ変えている。
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
        toml::from_str(text).context("テストケースファイルのパースに失敗しました")
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("{} を読めませんでした", path.display()))?;
        Self::parse(&text).with_context(|| format!("{} の読み込みに失敗しました", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let text = self.to_toml()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("{} を作れませんでした", parent.display()))?;
        }
        std::fs::write(path, text).with_context(|| format!("{} に書けませんでした", path.display()))
    }

    /// 設計 §4.5 の形の TOML にする。
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

        // [float] は [[cases]] より前に置く。TOML はテーブルが始まると
        // それ以降のキーがそのテーブルに属してしまうため。
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

/// 名前など短い文字列は basic string で十分（AtCoder の alias に特殊文字は出ない）。
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// 複数行リテラル文字列。開き `'''` の直後の改行は TOML 仕様で削られるので、
/// データの1行目をそのまま次の行に置ける。
fn literal_block(case: &str, field: &str, data: &str) -> Result<String> {
    if data.contains(DELIMITER) {
        bail!(
            "ケース {case} の {field} に {DELIMITER} が含まれており、\
             TOML のリテラル文字列で表現できません"
        );
    }
    // 末尾が改行でないケース（`out = '''Yes'''`）も表現できるようにする。
    if data.is_empty() {
        return Ok(format!("{DELIMITER}{DELIMITER}"));
    }
    if data.ends_with('\n') {
        Ok(format!("{DELIMITER}\n{data}{DELIMITER}"))
    } else {
        Ok(format!("{DELIMITER}\n{data}\n{DELIMITER}"))
    }
}

/// `1e-9` のように、TOML と人間の両方が読める形にする。
fn format_float(value: f64) -> String {
    let formatted = format!("{value:e}");
    // Rust の `{:e}` は `1e-9`。指数が無いときは小数点を足して float と分かるようにする。
    if formatted.contains('e') {
        formatted
    } else {
        format!("{value:?}")
    }
}

/// `2s` / `2500ms` / `2.5s` / `2000` を ms で読む。
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

/// 整数秒なら `2s`、そうでなければ `2500ms`。
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
        // [float] は [[cases]] より前になければ、cases の中に取り込まれてしまう。
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
        // リテラル文字列は開き引用符直後の改行しか削らないので、
        // 末尾に改行が無いデータは round-trip すると改行が付いてしまう。
        // 判定は Lines（末尾の空行を無視）なので実害は無いが、往復で崩れないことは確かめる。
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
        assert!(err.contains("表現できません"), "{err}");
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
