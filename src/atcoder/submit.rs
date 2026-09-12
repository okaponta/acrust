//! Parsing for the submit flow.
//!
//! The language id is never hardcoded: AtCoder currently answers `6088` for Rust,
//! while cargo-compete still writes the long-dead `5054`. The number moves with
//! every language update, so it is read off the submit page each time.

use crate::atcoder::html;
use anyhow::{anyhow, Result};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// One entry of the language `<select>` on the submit page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Language {
    pub id: String,
    pub name: String,
}

fn selector(s: &'static str) -> Selector {
    Selector::parse(s).expect("static selector is valid")
}

/// The languages offered on the submit page.
///
/// The `<select>` is repeated per problem as `div#select-lang-{screen_name}`, so
/// a known problem narrows the search; otherwise the whole page is scanned and
/// the duplicates folded away.
pub fn parse_languages(html_text: &str, screen_name: Option<&str>) -> Vec<Language> {
    let document = Html::parse_document(html_text);
    let scoped = screen_name
        .and_then(|name| Selector::parse(&format!("div#select-lang-{name}")).ok())
        .and_then(|selector| document.select(&selector).next().map(|e| e.html()));

    let scoped_document = scoped.as_ref().map(|html| Html::parse_fragment(html));
    let root = scoped_document.as_ref().unwrap_or(&document);

    let option = selector("option");
    let mut seen = std::collections::BTreeSet::new();
    root.select(&option)
        .filter_map(|element| {
            let id = element.value().attr("value")?.trim().to_owned();
            if id.is_empty() {
                return None;
            }
            let name = element.text().collect::<String>().trim().to_owned();
            if name.is_empty() {
                return None;
            }
            Some(Language { id, name })
        })
        .filter(|language| seen.insert((language.id.clone(), language.name.clone())))
        .collect()
}

/// Every hidden field of the submit form, exactly as the page writes it.
///
/// Copying the whole form rather than just `csrf_token` is what keeps acrust
/// working the day AtCoder adds another hidden field: a browser would send it too,
/// and a submission missing it is rejected with nothing useful said about why.
/// An empty result lets the caller fall back to `csrf_token` alone.
pub fn hidden_inputs(page: &str, contest: &str) -> Vec<(String, String)> {
    let document = Html::parse_document(page);
    let form = selector("form");
    let hidden = selector(r#"input[type="hidden"]"#);
    let action = format!("/contests/{contest}/submit");

    document
        .select(&form)
        .find(|element| {
            element
                .value()
                .attr("action")
                .is_some_and(|value| value.ends_with(&action))
        })
        .map(|form| {
            form.select(&hidden)
                .filter_map(|input| {
                    let name = input.value().attr("name")?.trim();
                    if name.is_empty() {
                        return None;
                    }
                    Some((
                        name.to_owned(),
                        input.value().attr("value").unwrap_or_default().to_owned(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Picks a language by regex, e.g. `^Rust \(rustc`.
pub fn pick_language(languages: &[Language], pattern: &str) -> Result<Language> {
    let regex = regex::Regex::new(pattern)
        .map_err(|e| anyhow!("[submit] language-pattern is not a valid regex: {e}"))?;
    let mut matched: Vec<&Language> = languages
        .iter()
        .filter(|language| regex.is_match(&language.name))
        .collect();
    if matched.is_empty() {
        return Err(anyhow!(
            "no language on the submit page matches {pattern} ({} to choose from). \
             You can set one directly with [submit] language-id",
            languages.len()
        ));
    }
    // On a tie take the last: AtCoder lists newer versions further down.
    Ok(matched.pop().expect("checked above").clone())
}

/// A row of the submission list, used to find our own submission after the POST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submission {
    pub id: String,
    /// The task screen name, e.g. `abc418_c`.
    pub task: String,
    pub verdict: String,
}

impl Submission {
    pub fn url(&self, contest: &str) -> String {
        format!(
            "https://atcoder.jp/contests/{contest}/submissions/{}",
            self.id
        )
    }
}

/// The first row of `/contests/{c}/submissions/me`, i.e. the newest submission.
pub fn parse_latest_submission(html_text: &str) -> Option<Submission> {
    let document = Html::parse_document(html_text);
    let row = selector("table tbody tr");
    let link = selector("a");
    let label = selector("span.label");
    let cell = selector("td");

    for tr in document.select(&row) {
        // Rows with no submission link are headings or "nothing here" notices.
        let Some(id) = tr
            .select(&link)
            .filter_map(|a| a.value().attr("href"))
            .find_map(submission_id_in)
        else {
            continue;
        };
        let task = tr
            .select(&link)
            .filter_map(|a| a.value().attr("href"))
            .find_map(task_screen_name_in)
            .unwrap_or_default();
        let verdict = tr
            .select(&label)
            .next()
            .map(|span| span.text().collect::<String>().trim().to_owned())
            .or_else(|| {
                // While judging there is no label, just a `3/32` progress cell.
                tr.select(&cell)
                    .map(|td| td.text().collect::<String>().trim().to_owned())
                    .find(|text| looks_like_progress(text))
            })
            .unwrap_or_else(|| "WJ".to_owned());
        return Some(Submission { id, task, verdict });
    }
    None
}

fn submission_id_in(href: &str) -> Option<String> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"/contests/[^/]+/submissions/(\d+)").expect("valid regex")
    });
    Some(re.captures(href)?[1].to_owned())
}

fn task_screen_name_in(href: &str) -> Option<String> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"/contests/[^/]+/tasks/([^/?#]+)").expect("valid regex")
    });
    Some(re.captures(href)?[1].to_owned())
}

fn looks_like_progress(text: &str) -> bool {
    let mut parts = text.split('/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(a), Some(b), None) => {
            !a.is_empty()
                && !b.is_empty()
                && a.chars().all(|c| c.is_ascii_digit())
                && b.chars().all(|c| c.is_ascii_digit())
        }
        _ => false,
    }
}

/// A judging status, shaped after the JSON API the AtCoder page itself polls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub verdict: String,
    pub time: Option<String>,
    pub memory: Option<String>,
    pub score: Option<String>,
}

#[derive(Deserialize)]
struct StatusResponse {
    #[serde(rename = "Result")]
    result: std::collections::BTreeMap<String, StatusEntry>,
}

#[derive(Deserialize)]
struct StatusEntry {
    #[serde(rename = "Html")]
    html: String,
    #[serde(rename = "Score")]
    score: Option<String>,
}

/// Reads `/contests/{c}/submissions/me/status/json?sids[]={id}`.
///
/// This is what the AtCoder page polls, and it measured 651 bytes against 27KB
/// for re-fetching the submission list — worth it when polling every few seconds.
pub fn parse_status_json(body: &str, id: &str) -> Option<Status> {
    let response: StatusResponse = serde_json::from_str(body).ok()?;
    let entry = response.result.get(id)?;
    let cells = split_cells(&entry.html);
    let verdict = cells.first()?.clone();
    Some(Status {
        verdict,
        time: cells.get(1).cloned().filter(|s| !s.is_empty()),
        memory: cells.get(2).cloned().filter(|s| !s.is_empty()),
        score: entry.score.clone().filter(|s| !s.is_empty()),
    })
}

/// Splits `<td>…</td><td>…</td>` into the text of each cell.
fn split_cells(fragment: &str) -> Vec<String> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"(?s)<td[^>]*>(.*?)</td>").expect("valid regex"));
    re.captures_iter(fragment)
        .map(|c| html::to_text(&c[1]))
        .collect()
}

/// Whether the verdict has settled, which is when polling stops.
pub fn is_final(verdict: &str) -> bool {
    const FINAL: [&str; 9] = ["AC", "WA", "TLE", "MLE", "RE", "CE", "OLE", "QLE", "IE"];
    let verdict = verdict.trim();
    FINAL.contains(&verdict)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shaped like the real page, with the language `<select>` repeated per problem.
    const SUBMIT_PAGE: &str = r#"
<form action="/contests/dummy001/submit" method="POST">
  <input type="hidden" name="csrf_token" value="tok+en/=" />
  <select id="select-task" name="data.TaskScreenName">
    <option value="dummy001_a">A - Alpha</option>
    <option value="dummy001_b">B - Bravo</option>
  </select>
  <div id="select-lang" data-name="data.LanguageId" style="display:none;">
    <div id="select-lang-dummy001_a">
      <select class="form-control">
        <option></option>
        <option value="6006" data-ace-mode="text">AWK (GNU awk 5.2.1)</option>
        <option value="6088" data-ace-mode="rust">Rust (rustc 1.89.0)</option>
        <option value="6090" data-ace-mode="text">Ruby (CRuby 3.4.1)</option>
      </select>
    </div>
    <div id="select-lang-dummy001_b">
      <select class="form-control">
        <option></option>
        <option value="6088" data-ace-mode="rust">Rust (rustc 1.89.0)</option>
      </select>
    </div>
  </div>
</form>
"#;

    #[test]
    fn languages_are_read_from_the_submit_page() {
        let languages = parse_languages(SUBMIT_PAGE, Some("dummy001_a"));
        assert_eq!(languages.len(), 3, "{languages:?}");
        let rust = pick_language(&languages, r"^Rust \(rustc").unwrap();
        assert_eq!(rust.id, "6088");
        assert_eq!(rust.name, "Rust (rustc 1.89.0)");
    }

    #[test]
    fn the_language_list_falls_back_to_the_whole_page() {
        // Missing the per-problem div is fine as long as the page still answers.
        let languages = parse_languages(SUBMIT_PAGE, Some("no-such-task"));
        assert!(pick_language(&languages, r"^Rust \(rustc").is_ok());
        // Folded: the same language appears once per problem.
        assert_eq!(languages.iter().filter(|l| l.id == "6088").count(), 1);
    }

    #[test]
    fn the_hidden_fields_of_the_submit_form_are_copied_verbatim() {
        let hidden = hidden_inputs(SUBMIT_PAGE, "dummy001");
        assert_eq!(hidden, [("csrf_token".to_owned(), "tok+en/=".to_owned())]);
        // A form belonging to another contest is not picked up.
        assert!(hidden_inputs(SUBMIT_PAGE, "dummy002").is_empty());
    }

    /// AtCoder HTML-escapes the value of a hidden input: the same token reads
    /// `ykoqlSD+jmy3…` in `var csrfToken` but `ykoqlSD&#43;jmy3…` in the form,
    /// because Go's template escapes the `+`. Send it without decoding and the
    /// token no longer matches, so the submission is turned away.
    #[test]
    fn an_html_escaped_hidden_value_comes_back_decoded() {
        let page = r#"<form action="/contests/abc418/submit">
          <input type="hidden" name="csrf_token" value="ykoqlSD&#43;jmy3Jss0=" /></form>"#;
        let hidden = hidden_inputs(page, "abc418");
        assert_eq!(hidden[0].1, "ykoqlSD+jmy3Jss0=");
    }

    /// A hidden field AtCoder has not invented yet still has to be carried along.
    #[test]
    fn an_extra_hidden_field_is_carried_along() {
        let page = SUBMIT_PAGE.replace(
            r#"<input type="hidden" name="csrf_token" value="tok+en/=" />"#,
            r#"<input type="hidden" name="csrf_token" value="tok+en/=" />
               <input type="hidden" name="data.SomethingNew" value="42" />"#,
        );
        let hidden = hidden_inputs(&page, "dummy001");
        assert_eq!(hidden.len(), 2, "{hidden:?}");
        assert_eq!(hidden[1].0, "data.SomethingNew");
        assert_eq!(hidden[1].1, "42");
    }

    #[test]
    fn a_missing_language_says_what_to_do() {
        let languages = parse_languages(SUBMIT_PAGE, None);
        let err = pick_language(&languages, r"^COBOL")
            .unwrap_err()
            .to_string();
        assert!(err.contains("language-id"), "{err}");
    }

    /// A row that has already been judged.
    const JUDGED_ROW: &str = r#"
<table><tbody>
  <tr>
    <td><time>2026-01-23 07:22:15+0900</time></td>
    <td><a href="/contests/abc418/tasks/abc418_c">C - Flush</a></td>
    <td><a href="/users/okaponta">okaponta</a></td>
    <td><a href="/contests/abc418/submissions/me?f.Language=6088">Rust (rustc 1.89.0)</a></td>
    <td class="text-right submission-score" data-id="72649417">350</td>
    <td class="text-right">543 Byte</td>
    <td class='text-center'><span class='label label-success' title="正解">AC</span></td>
    <td class='text-right'>95 ms</td>
    <td class='text-right'>13344 KiB</td>
    <td class="text-center"><a href="/contests/abc418/submissions/72649417">詳細</a></td>
  </tr>
</tbody></table>
"#;

    #[test]
    fn the_newest_submission_is_found_with_its_task_and_verdict() {
        let submission = parse_latest_submission(JUDGED_ROW).unwrap();
        assert_eq!(submission.id, "72649417");
        assert_eq!(submission.task, "abc418_c");
        assert_eq!(submission.verdict, "AC");
        assert_eq!(
            submission.url("abc418"),
            "https://atcoder.jp/contests/abc418/submissions/72649417"
        );
    }

    #[test]
    fn a_submission_still_being_judged_reports_its_progress() {
        let html = JUDGED_ROW
            .replace(
                r#"<td class='text-center'><span class='label label-success' title="正解">AC</span></td>"#,
                r#"<td class="text-center" id="judge-status-72649417">3/32</td>"#,
            )
            .replace("<td class='text-right'>95 ms</td>", "")
            .replace("<td class='text-right'>13344 KiB</td>", "");
        let submission = parse_latest_submission(&html).unwrap();
        assert_eq!(submission.id, "72649417");
        assert_eq!(submission.verdict, "3/32");
    }

    #[test]
    fn an_empty_table_yields_nothing() {
        assert!(parse_latest_submission("<table><tbody></tbody></table>").is_none());
    }

    #[test]
    fn rows_without_a_submission_link_are_skipped() {
        let html = JUDGED_ROW.replace(
            "<tbody>",
            "<tbody><tr><td colspan=\"10\">該当する提出はありません</td></tr>",
        );
        let submission = parse_latest_submission(&html).unwrap();
        assert_eq!(submission.id, "72649417");
    }

    /// Shaped like the real response from the API the AtCoder page polls.
    const STATUS_JSON: &str = r#"{"Result":{"72649417":{"Html":"<td class='text-center'><span class='label label-success' title=\"正解\">AC</span></td><td class='text-right'>95 ms</td><td class='text-right'>13344 KiB</td>","Score":"350"}}}"#;

    #[test]
    fn the_status_api_gives_the_verdict_time_and_memory() {
        let status = parse_status_json(STATUS_JSON, "72649417").unwrap();
        assert_eq!(status.verdict, "AC");
        assert_eq!(status.time.as_deref(), Some("95 ms"));
        assert_eq!(status.memory.as_deref(), Some("13344 KiB"));
        assert_eq!(status.score.as_deref(), Some("350"));
        // Another submission id must not match this entry.
        assert!(parse_status_json(STATUS_JSON, "1").is_none());
    }

    #[test]
    fn a_pending_status_has_no_time_or_memory() {
        let json = r#"{"Result":{"1":{"Html":"<td class='text-center'><span class='label label-default'>WJ</span></td>","Score":"0"}}}"#;
        let status = parse_status_json(json, "1").unwrap();
        assert_eq!(status.verdict, "WJ");
        assert_eq!(status.time, None);
        assert_eq!(status.memory, None);
    }

    #[test]
    fn only_settled_verdicts_stop_the_watch() {
        for verdict in ["AC", "WA", "TLE", "MLE", "RE", "CE", "IE"] {
            assert!(is_final(verdict), "{verdict}");
        }
        for verdict in ["WJ", "WR", "3/32", "", "Judging"] {
            assert!(!is_final(verdict), "{verdict}");
        }
    }
}
