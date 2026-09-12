//! Reading the problem list and the sample cases.
//!
//! The main route is `/contests/{contest}/tasks_print`, which carries the samples
//! for every problem on one page; together with `/tasks` that is two requests per
//! contest.
//!
//! The parsing is deliberately loose. AtCoder's markup drifts with the years, and
//! all of these exist in the archive: problems with no `span.lang-ja`, problems
//! with no Japanese version at all, and problems whose explanatory `<p>` trails
//! after the `<pre>`. None of them should break a fetch of the rest.

use crate::atcoder::html;
use anyhow::{anyhow, Result};
use scraper::{ElementRef, Html, Selector};
use std::sync::OnceLock;

/// One row of `/tasks`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskEntry {
    /// The first column: `A`, `Ex`, `001`.
    pub label: String,
    /// The lowercased label, which becomes `src/bin/{alias}.rs`.
    pub alias: String,
    /// The id in the URL. There is no rule that derives it: problem C of `abc042`
    /// is `arc058_a`, so it has to be read off the page.
    pub screen_name: String,
    pub title: String,
    pub timelimit_ms: Option<u64>,
}

/// One problem, as read out of `tasks_print`.
#[derive(Debug, Clone, PartialEq)]
pub struct ProblemPage {
    pub label: String,
    pub title: String,
    pub timelimit_ms: Option<u64>,
    pub samples: Vec<Sample>,
    /// Interactive problems cannot be sample-tested at all.
    pub interactive: bool,
    /// Set when the problem is judged with a tolerance.
    pub float: Option<FloatTolerance>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample {
    pub input: String,
    pub output: String,
}

/// A condition such as "absolute or relative error at most `10^{-6}`".
///
/// The two are held separately because plenty of problems name only one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloatTolerance {
    pub relative: Option<f64>,
    pub absolute: Option<f64>,
}

fn selector(s: &'static str) -> Selector {
    Selector::parse(s).expect("static selector is valid")
}

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

/// `A` -> `a`, `Ex` -> `ex`, `001` -> `001`.
fn alias_for(label: &str) -> String {
    label.trim().to_lowercase()
}

/// Parses `tasks_print`, which holds every problem of the contest.
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

/// A single problem page, for the contests where `tasks_print` is not usable.
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
    // On a single problem page the heading has an Editorial link hanging off it,
    // so only the text directly under it counts.
    let heading_text = direct_text_of(heading);
    let (label, title) = split_heading(&heading_text);

    let timelimit_ms = element
        .select(&selector("p"))
        .map(|p| text_of(p))
        .find_map(|text| parse_time_limit(&text));

    // Prefer the Japanese half; older problems are not split by language at all.
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

/// Splits `A - I'm a teapot` into `("A", "I'm a teapot")`.
fn split_heading(heading: &str) -> (String, String) {
    match heading.split_once(" - ") {
        Some((label, title)) => (label.trim().to_owned(), title.trim().to_owned()),
        None => (heading.trim().to_owned(), String::new()),
    }
}

/// Pairs inputs with outputs by the number in the `h3`, never by document order:
/// problems with partial scoring carry extra `<pre>` blocks in between.
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
        // Only the first <pre>; an explanatory <p> can follow it in the section.
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

/// Tells `入力例 1` / `Sample Input 1` apart. A bare `入力` is the input *format*
/// section, and must not be mistaken for a case.
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

/// Turns the contents of a `<pre>` back into the raw bytes of a test case: tags
/// off, entities undone, CRLF to LF, exactly one trailing newline.
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

/// Whether the problem is judged with a tolerance, and how much.
///
/// The figure is read rather than assumed because problems range from `10^{-2}`
/// to `10^{-10}`. Picking one strict value everywhere would fail correct answers
/// locally that the judge accepts.
fn detect_float_tolerance(statement: &str) -> Option<FloatTolerance> {
    let mentions_error = statement.contains("誤差")
        || statement.contains("absolute or relative error")
        || statement.contains("absolute error")
        || statement.contains("relative error");
    if !mentions_error {
        return None;
    }

    // The English text folds both into "absolute or relative error", so looking
    // for "absolute error" alone misses half the problems.
    let both_in_english = statement.contains("absolute or relative error")
        || statement.contains("relative or absolute error");
    let absolute =
        statement.contains("絶対誤差") || statement.contains("absolute error") || both_in_english;
    let relative =
        statement.contains("相対誤差") || statement.contains("relative error") || both_in_english;
    let tolerance = extract_tolerance(statement).unwrap_or(1e-9);

    // Some statements say only "誤差" without naming which; honour both.
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

/// Pulls `1e-6` out of `10^{-6}` or `1e-6`.
fn extract_tolerance(statement: &str) -> Option<f64> {
    static POWER: OnceLock<regex::Regex> = OnceLock::new();
    static SCIENTIFIC: OnceLock<regex::Regex> = OnceLock::new();
    let power = POWER.get_or_init(|| {
        regex::Regex::new(r"10\s*\^\s*\{?\s*-\s*(\d+)\s*\}?").expect("valid regex")
    });
    let scientific =
        SCIENTIFIC.get_or_init(|| regex::Regex::new(r"1\s*[eE]\s*-\s*(\d+)").expect("valid regex"));

    // Prefer an exponent near the word for "error": the constraints section has
    // powers of ten of its own that have nothing to do with the tolerance.
    let window = error_window(statement).unwrap_or(statement);
    let exponent = power
        .captures(window)
        .or_else(|| scientific.captures(window))
        .and_then(|c| c[1].parse::<i32>().ok())?;
    Some(10f64.powi(-exponent))
}

/// The one sentence that mentions the error bound.
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

/// Reads 2000 out of `Time Limit: 2 sec / Memory Limit: 1024 MiB`.
fn parse_time_limit(text: &str) -> Option<u64> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"Time\s*Limit\s*:\s*([0-9.]+)\s*(m?sec|ms|s)").expect("valid regex")
    });
    let captures = re.captures(text)?;
    to_millis(&captures[1], &captures[2])
}

/// Reads `2 sec`, `2.5 sec` or `500 msec` as milliseconds.
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

fn text_of(element: ElementRef) -> String {
    element
        .text()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Only the direct text nodes, so a nested link is not dragged in.
fn direct_text_of(element: ElementRef) -> String {
    element
        .children()
        .filter_map(|node| node.value().as_text().map(|t| t.to_string()))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
