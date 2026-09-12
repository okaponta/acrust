//! 提出まわりのパース（設計 §3.4 / §4.9）。
//!
//! 言語 ID はハードコードしない。実測でも、cargo-compete が書き込む `5054` に対して
//! 現在の AtCoder は `6088` を使っており、言語アップデートのたびに変わる。

use crate::atcoder::html;
use anyhow::{anyhow, Result};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// 提出ページの `<select>` に並ぶ言語。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Language {
    pub id: String,
    pub name: String,
}

fn selector(s: &'static str) -> Selector {
    Selector::parse(s).expect("static selector is valid")
}

/// 提出ページから言語の一覧を取り出す。
///
/// 言語の `<select>` は問題ごとに `div#select-lang-{screen_name}` として繰り返される。
/// 問題が分かっていればそこに絞る。
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

/// 提出フォームの隠しフィールドを、ページに書かれている通りに全部集める。
///
/// `csrf_token` だけを拾うのではなくフォームごと写すのは、AtCoder が隠しフィールドを
/// 増やしたときに黙って弾かれないようにするため（ブラウザは当然それも送る）。
/// フォームが見つからなければ空を返すので、呼び出し側が `csrf_token` だけで組める。
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

/// `^Rust \(rustc` のような正規表現で言語を選ぶ。
pub fn pick_language(languages: &[Language], pattern: &str) -> Result<Language> {
    let regex = regex::Regex::new(pattern)
        .map_err(|e| anyhow!("[submit] language-pattern が正規表現として読めません: {e}"))?;
    let mut matched: Vec<&Language> = languages
        .iter()
        .filter(|language| regex.is_match(&language.name))
        .collect();
    if matched.is_empty() {
        return Err(anyhow!(
            "提出ページに {pattern} に一致する言語がありません（候補 {} 件）。\
             [submit] language-id で直接指定できます",
            languages.len()
        ));
    }
    // 複数一致したら、新しいバージョンほど後ろに並ぶので最後を採る。
    Ok(matched.pop().expect("checked above").clone())
}

/// 提出一覧の行。POST 直後に自分の提出を見つけるために使う。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submission {
    pub id: String,
    /// `abc418_c` のような task screen name。
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

/// `/contests/{c}/submissions/me` の最初の行（＝いちばん新しい提出）。
pub fn parse_latest_submission(html_text: &str) -> Option<Submission> {
    let document = Html::parse_document(html_text);
    let row = selector("table tbody tr");
    let link = selector("a");
    let label = selector("span.label");
    let cell = selector("td");

    for tr in document.select(&row) {
        // 提出へのリンクが無い行（見出しなど）は飛ばす。
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
                // 判定中は label が無く、セルに `3/32` のような進捗が入る。
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

/// ジャッジの途中経過。AtCoder のページ自身が叩く JSON API の形。
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

/// `/contests/{c}/submissions/me/status/json?sids[]={id}` の応答を読む。
///
/// ページ全体（実測 27KB）ではなくこちらを使う。AtCoder のページ自身がこれを
/// ポーリングしており、実測 651 バイトで済む。
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

/// `<td>…</td><td>…</td>` を中身のテキストに割る。
fn split_cells(fragment: &str) -> Vec<String> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"(?s)<td[^>]*>(.*?)</td>").expect("valid regex"));
    re.captures_iter(fragment)
        .map(|c| html::to_text(&c[1]))
        .collect()
}

/// ジャッジが確定したか。確定したら追跡を即やめる（設計 §4.9）。
pub fn is_final(verdict: &str) -> bool {
    const FINAL: [&str; 9] = ["AC", "WA", "TLE", "MLE", "RE", "CE", "OLE", "QLE", "IE"];
    let verdict = verdict.trim();
    FINAL.contains(&verdict)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 実物と同じ形。言語の `<select>` が問題ごとに繰り返される。
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
        // 問題ごとの div が見つからなくても、ページ全体から拾えれば足りる。
        let languages = parse_languages(SUBMIT_PAGE, Some("no-such-task"));
        assert!(pick_language(&languages, r"^Rust \(rustc").is_ok());
        // 重複は畳む（同じ言語が問題の数だけ並ぶため）。
        assert_eq!(languages.iter().filter(|l| l.id == "6088").count(), 1);
    }

    #[test]
    fn the_hidden_fields_of_the_submit_form_are_copied_verbatim() {
        let hidden = hidden_inputs(SUBMIT_PAGE, "dummy001");
        assert_eq!(hidden, [("csrf_token".to_owned(), "tok+en/=".to_owned())]);
        // 別のコンテストのフォームを拾わない。
        assert!(hidden_inputs(SUBMIT_PAGE, "dummy002").is_empty());
    }

    /// 将来 AtCoder が隠しフィールドを増やしても、そのまま送れること。
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

    /// 判定済みの行。
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

    /// 実物と同じ形（AtCoder のページ自身がポーリングしている API）。
    const STATUS_JSON: &str = r#"{"Result":{"72649417":{"Html":"<td class='text-center'><span class='label label-success' title=\"正解\">AC</span></td><td class='text-right'>95 ms</td><td class='text-right'>13344 KiB</td>","Score":"350"}}}"#;

    #[test]
    fn the_status_api_gives_the_verdict_time_and_memory() {
        let status = parse_status_json(STATUS_JSON, "72649417").unwrap();
        assert_eq!(status.verdict, "AC");
        assert_eq!(status.time.as_deref(), Some("95 ms"));
        assert_eq!(status.memory.as_deref(), Some("13344 KiB"));
        assert_eq!(status.score.as_deref(), Some("350"));
        // 別の提出 ID を渡しても混ざらない。
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
