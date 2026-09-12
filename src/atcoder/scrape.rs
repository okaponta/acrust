//! 問題一覧とサンプルケースの取り出し（設計 §3.2 / §3.3）。
//!
//! 主経路は `/contests/{contest}/tasks_print` で、**全問の入出力例が 1 ページに入っている**。
//! `/tasks` と合わせて 1 コンテストあたり 2 リクエストで済む。
//!
//! パースは意図的に緩くしてある。AtCoder の HTML は年代で揺れがあり、
//! - `span.lang-ja` が無い古い問題
//! - 日本語版が無く `Sample Input` しかない問題
//! - 解説の `<p>` が `<pre>` の後ろに付く問題
//!
//! のいずれでも壊れないようにしている。壊れたときは「どの問題の何が取れなかったか」を言う。

use crate::atcoder::html;
use anyhow::{anyhow, Result};
use scraper::{ElementRef, Html, Selector};
use std::sync::OnceLock;

/// `/tasks` の1行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskEntry {
    /// 表の先頭列（`A`、`Ex`、`001` など）。
    pub label: String,
    /// `src/bin/{alias}.rs` になる名前。label を小文字にしたもの。
    pub alias: String,
    /// `abc042` の C が `arc058_a` になるような、規則から導けない ID（設計 §4.8）。
    pub screen_name: String,
    pub title: String,
    pub timelimit_ms: Option<u64>,
}

/// `tasks_print` から取り出した1問。
#[derive(Debug, Clone, PartialEq)]
pub struct ProblemPage {
    pub label: String,
    pub title: String,
    pub timelimit_ms: Option<u64>,
    pub samples: Vec<Sample>,
    /// インタラクティブ問題はサンプルテストができない（設計 §3.3）。
    pub interactive: bool,
    /// 誤差ジャッジなら許容誤差。
    pub float: Option<FloatTolerance>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample {
    pub input: String,
    pub output: String,
}

/// 「絶対誤差または相対誤差が `10^{-6}` 以下」のような判定条件。
///
/// 片方しか書かれていない問題があるので、それぞれ独立に持つ。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloatTolerance {
    pub relative: Option<f64>,
    pub absolute: Option<f64>,
}

fn selector(s: &'static str) -> Selector {
    Selector::parse(s).expect("static selector is valid")
}

/// `/contests/{contest}/tasks` をパースする。
pub fn parse_task_list(html_text: &str, contest: &str) -> Result<Vec<TaskEntry>> {
    let document = Html::parse_document(html_text);
    let row = selector("table tbody tr");
    let cell = selector("td");
    let link = selector("a");

    let href_prefix = format!("/contests/{contest}/tasks/");
    let mut entries = Vec::new();
    for tr in document.select(&row) {
        let cells: Vec<ElementRef> = tr.select(&cell).collect();
        if cells.len() < 2 {
            continue;
        }
        let Some(anchor) = cells[0].select(&link).next() else {
            continue;
        };
        let Some(href) = anchor.value().attr("href") else {
            continue;
        };
        let Some(screen_name) = href.rsplit('/').next() else {
            continue;
        };
        if !href.contains(&href_prefix) || screen_name.is_empty() {
            continue;
        }

        let label = text_of(anchor);
        if label.is_empty() {
            continue;
        }
        let title = cells.get(1).map(|c| text_of(*c)).unwrap_or_default();
        let timelimit_ms = cells.get(2).and_then(|c| parse_duration(&text_of(*c)));

        entries.push(TaskEntry {
            alias: alias_for(&label),
            label,
            screen_name: screen_name.to_owned(),
            title,
            timelimit_ms,
        });
    }

    if entries.is_empty() {
        return Err(anyhow!(
            "could not pull out the problem list for {contest}. AtCoder's HTML may have changed"
        ));
    }
    Ok(entries)
}

/// `A` -> `a`、`Ex` -> `ex`、`001` -> `001`。
fn alias_for(label: &str) -> String {
    label.trim().to_lowercase()
}

/// `/contests/{contest}/tasks_print` をパースする。1 ページに全問入っている。
pub fn parse_tasks_print(html_text: &str) -> Result<Vec<ProblemPage>> {
    let document = Html::parse_document(html_text);
    let container = selector("div.col-sm-12");
    let problems: Vec<ProblemPage> = document
        .select(&container)
        .filter(|element| element.select(&selector("span.h2")).next().is_some())
        .filter_map(|element| parse_problem_element(element).ok())
        .collect();

    if problems.is_empty() {
        return Err(anyhow!(
            "could not pull out samples for a single problem. AtCoder's HTML may have changed"
        ));
    }
    Ok(problems)
}

/// 個別の問題ページ（`/contests/{c}/tasks/{screen_name}`）。tasks_print が使えないときの経路。
pub fn parse_task_page(html_text: &str) -> Result<ProblemPage> {
    let document = Html::parse_document(html_text);
    let container = selector("div.col-sm-12");
    document
        .select(&container)
        .find(|element| element.select(&selector("span.h2")).next().is_some())
        .ok_or_else(|| anyhow!("could not recognise the structure of the problem page"))
        .and_then(|element| parse_problem_element(element))
}

fn parse_problem_element(element: ElementRef) -> Result<ProblemPage> {
    let heading = element
        .select(&selector("span.h2"))
        .next()
        .ok_or_else(|| anyhow!("could not find the problem heading"))?;
    // 個別ページの見出しには Editorial へのリンクがぶら下がるので、直下のテキストだけを使う。
    let heading_text = direct_text_of(heading);
    let (label, title) = split_heading(&heading_text);

    let timelimit_ms = element
        .select(&selector("p"))
        .map(|p| text_of(p))
        .find_map(|text| parse_time_limit(&text));

    // 日本語版があればそれを、無ければ全体を対象にする（古い問題は lang 分割が無い）。
    let statement = element
        .select(&selector("span.lang-ja"))
        .next()
        .or_else(|| element.select(&selector("div#task-statement")).next())
        .unwrap_or(element);

    let samples = collect_samples(statement);
    let statement_text = text_of(statement);

    Ok(ProblemPage {
        label,
        title,
        timelimit_ms,
        interactive: looks_interactive(&statement_text),
        float: detect_float_tolerance(&statement_text),
        samples,
    })
}

/// `A - I'm a teapot` を `("A", "I'm a teapot")` に割る。
fn split_heading(heading: &str) -> (String, String) {
    match heading.split_once(" - ") {
        Some((label, title)) => (label.trim().to_owned(), title.trim().to_owned()),
        None => (heading.trim().to_owned(), String::new()),
    }
}

/// `div.part > section` を走査し、h3 の番号で入力例と出力例をペアにする。
///
/// 順序に依存しないのは、部分点付きの問題などで `<pre>` が余分に出ることがあるため。
fn collect_samples(statement: ElementRef) -> Vec<Sample> {
    let section = selector("div.part section");
    let heading = selector("h3");
    let pre = selector("pre");

    let mut inputs: Vec<(u32, String)> = Vec::new();
    let mut outputs: Vec<(u32, String)> = Vec::new();

    for section in statement.select(&section) {
        let Some(h3) = section.select(&heading).next() else {
            continue;
        };
        let Some((kind, number)) = classify_sample_heading(&text_of(h3)) else {
            continue;
        };
        // 解説の <p> が後ろに付くことがあるので、最初の <pre> だけを取る。
        let Some(pre) = section.select(&pre).next() else {
            continue;
        };
        let data = normalize_sample(&pre.inner_html());
        match kind {
            SampleKind::Input => inputs.push((number, data)),
            SampleKind::Output => outputs.push((number, data)),
        }
    }

    inputs.sort_by_key(|(number, _)| *number);
    inputs
        .into_iter()
        .filter_map(|(number, input)| {
            let output = outputs
                .iter()
                .find(|(n, _)| *n == number)
                .map(|(_, output)| output.clone())?;
            Some(Sample { input, output })
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
enum SampleKind {
    Input,
    Output,
}

/// `入力例 1` / `Sample Input 1` を見分ける。`入力`（書式の説明）は拾わない。
fn classify_sample_heading(heading: &str) -> Option<(SampleKind, u32)> {
    static INPUT: OnceLock<regex::Regex> = OnceLock::new();
    static OUTPUT: OnceLock<regex::Regex> = OnceLock::new();
    let input = INPUT.get_or_init(|| {
        regex::Regex::new(r"(?:入力例|Sample\s+Input)\s*(\d+)").expect("valid regex")
    });
    let output = OUTPUT.get_or_init(|| {
        regex::Regex::new(r"(?:出力例|Sample\s+Output)\s*(\d+)").expect("valid regex")
    });

    if let Some(c) = input.captures(heading) {
        return Some((SampleKind::Input, c[1].parse().ok()?));
    }
    if let Some(c) = output.captures(heading) {
        return Some((SampleKind::Output, c[1].parse().ok()?));
    }
    None
}

/// `<pre>` の中身をテストケースの生データに戻す。
///
/// タグを剥がし、実体参照を戻し、CRLF を LF にし、末尾の改行を1つに揃える。
fn normalize_sample(inner_html: &str) -> String {
    let text = html::decode_entities(&strip_tags(inner_html));
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = text.trim_end_matches('\n');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    }
}

fn strip_tags(html_text: &str) -> String {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"(?s)<[^>]*>").expect("valid regex"));
    re.replace_all(html_text, "").into_owned()
}

/// インタラクティブ問題の判定（設計 §3.3）。
fn looks_interactive(statement: &str) -> bool {
    [
        "インタラクティブ",
        "対話型",
        "interactive problem",
        "Interactive Problem",
    ]
    .iter()
    .any(|needle| statement.contains(needle))
}

/// 誤差ジャッジの判定と許容誤差の取り出し。
///
/// 許容誤差まで読むのは、問題ごとに `10^{-2}` から `10^{-10}` まで幅があるため。
/// 一律に厳しい値を使うと、正しい解答が手元でだけ WA になる。
fn detect_float_tolerance(statement: &str) -> Option<FloatTolerance> {
    let mentions_error = statement.contains("誤差")
        || statement.contains("absolute or relative error")
        || statement.contains("absolute error")
        || statement.contains("relative error");
    if !mentions_error {
        return None;
    }

    // 英語は「absolute or relative error」と1つにまとめて書くので、
    // 「absolute error」を探すだけでは絶対誤差を取りこぼす。
    let both_in_english = statement.contains("absolute or relative error")
        || statement.contains("relative or absolute error");
    let absolute =
        statement.contains("絶対誤差") || statement.contains("absolute error") || both_in_english;
    let relative =
        statement.contains("相対誤差") || statement.contains("relative error") || both_in_english;
    let tolerance = extract_tolerance(statement).unwrap_or(1e-9);

    // どちらとも書いていないが「誤差」はある、という書き方もあるので両方に効かせる。
    let (relative, absolute) = if relative || absolute {
        (relative, absolute)
    } else {
        (true, true)
    };

    Some(FloatTolerance {
        relative: relative.then_some(tolerance),
        absolute: absolute.then_some(tolerance),
    })
}

/// `10^{-6}` / `10^{-6}` / `1e-6` から `1e-6` を取り出す。
fn extract_tolerance(statement: &str) -> Option<f64> {
    static POWER: OnceLock<regex::Regex> = OnceLock::new();
    static SCIENTIFIC: OnceLock<regex::Regex> = OnceLock::new();
    let power = POWER.get_or_init(|| {
        regex::Regex::new(r"10\s*\^\s*\{?\s*-\s*(\d+)\s*\}?").expect("valid regex")
    });
    let scientific =
        SCIENTIFIC.get_or_init(|| regex::Regex::new(r"1\s*[eE]\s*-\s*(\d+)").expect("valid regex"));

    // 「誤差」の周辺に出てくる指数を優先する。制約の 10^{-9} などを拾わないため。
    let window = error_window(statement).unwrap_or(statement);
    let exponent = power
        .captures(window)
        .or_else(|| scientific.captures(window))
        .and_then(|c| c[1].parse::<i32>().ok())?;
    Some(10f64.powi(-exponent))
}

/// 「誤差」を含む文だけを切り出す。
fn error_window(statement: &str) -> Option<&str> {
    let start = statement
        .find("誤差")
        .or_else(|| statement.find("absolute or relative error"))
        .or_else(|| statement.find("absolute error"))
        .or_else(|| statement.find("relative error"))?;
    let rest = &statement[start..];
    let end = rest
        .find('。')
        .map(|i| i + '。'.len_utf8())
        .unwrap_or_else(|| rest.len().min(200));
    Some(&rest[..end.min(rest.len())])
}

/// `Time Limit: 2 sec / Memory Limit: 1024 MiB` から 2000 を取り出す。
fn parse_time_limit(text: &str) -> Option<u64> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"Time\s*Limit\s*:\s*([0-9.]+)\s*(m?sec|ms|s)").expect("valid regex")
    });
    let captures = re.captures(text)?;
    to_millis(&captures[1], &captures[2])
}

/// `2 sec` / `2.5 sec` / `500 msec` を秒→ミリ秒で読む。
fn parse_duration(text: &str) -> Option<u64> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re =
        RE.get_or_init(|| regex::Regex::new(r"([0-9.]+)\s*(m?sec|ms|s)\b").expect("valid regex"));
    let captures = re.captures(text)?;
    to_millis(&captures[1], &captures[2])
}

fn to_millis(value: &str, unit: &str) -> Option<u64> {
    let value: f64 = value.parse().ok()?;
    let millis = match unit {
        "msec" | "ms" => value,
        _ => value * 1000.0,
    };
    if millis <= 0.0 {
        return None;
    }
    Some(millis.round() as u64)
}

/// 子孫まで含めたテキスト。空白は畳む。
fn text_of(element: ElementRef) -> String {
    element
        .text()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// 直下のテキストノードだけ。入れ子のリンク（Editorial など）を巻き込まない。
fn direct_text_of(element: ElementRef) -> String {
    element
        .children()
        .filter_map(|node| node.value().as_text().map(|t| t.to_string()))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
