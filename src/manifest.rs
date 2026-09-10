//! 既存の `Cargo.toml` に、足りない項目だけを足す。
//!
//! `toml_edit` を使うのは、ユーザーが手で足した依存やコメントを保つため
//! （設計 D3 で TOML を選んだ理由そのもの）。丸ごと書き直すと、
//! 開始前に作ったパッケージへ `fetch` で問題を足すたびに手編集が消える。

use crate::package::{bin_name, ProblemSpec};
use anyhow::{Context as _, Result};
use std::path::Path;
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table};

/// 変更があれば書き戻して `true` を返す。
pub fn merge(path: &Path, contest: &str, problems: &[ProblemSpec]) -> Result<bool> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("{} を読めませんでした", path.display()))?;
    let mut document: DocumentMut = text
        .parse()
        .with_context(|| format!("{} が TOML として読めません", path.display()))?;

    let mut changed = false;
    changed |= set_contest(&mut document, contest);
    changed |= set_tasks(&mut document, problems);
    changed |= add_missing_bins(&mut document, contest, problems);

    if changed {
        std::fs::write(path, document.to_string())
            .with_context(|| format!("{} に書けませんでした", path.display()))?;
    }
    Ok(changed)
}

/// `[package.metadata.acrust]` を（無ければ作って）返す。
fn acrust_table(document: &mut DocumentMut) -> &mut Table {
    let package = document
        .entry("package")
        .or_insert_with(|| Item::Table(Table::new()))
        .as_table_mut()
        .expect("package is a table");
    let metadata = package
        .entry("metadata")
        .or_insert_with(|| {
            let mut table = Table::new();
            table.set_implicit(true);
            Item::Table(table)
        })
        .as_table_mut()
        .expect("metadata is a table");
    metadata.set_implicit(true);
    metadata
        .entry("acrust")
        .or_insert_with(|| Item::Table(Table::new()))
        .as_table_mut()
        .expect("acrust is a table")
}

fn set_contest(document: &mut DocumentMut, contest: &str) -> bool {
    let acrust = acrust_table(document);
    if acrust.get("contest").and_then(|v| v.as_str()) == Some(contest) {
        return false;
    }
    acrust["contest"] = toml_edit::value(contest);
    true
}

/// task screen name を足す。既にある対応は上書きしない（取り違えていたら手で直せる）。
fn set_tasks(document: &mut DocumentMut, problems: &[ProblemSpec]) -> bool {
    let known: Vec<(&str, &str)> = problems
        .iter()
        .filter_map(|p| Some((p.alias.as_str(), p.screen_name.as_deref()?)))
        .collect();
    if known.is_empty() {
        return false;
    }

    let acrust = acrust_table(document);
    let tasks = acrust
        .entry("tasks")
        .or_insert_with(|| Item::Table(Table::new()))
        .as_table_mut()
        .expect("tasks is a table");

    let mut changed = false;
    for (alias, screen_name) in known {
        if tasks.get(alias).and_then(|v| v.as_str()) == Some(screen_name) {
            continue;
        }
        tasks[alias] = toml_edit::value(screen_name);
        changed = true;
    }
    changed
}

/// 未登録の問題ぶんだけ `[[bin]]` を足す。既存の順序と書式はそのまま。
fn add_missing_bins(document: &mut DocumentMut, contest: &str, problems: &[ProblemSpec]) -> bool {
    let bins = document
        .entry("bin")
        .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()));
    let Some(bins) = bins.as_array_of_tables_mut() else {
        return false;
    };

    let existing: Vec<String> = bins
        .iter()
        .filter_map(|table| table.get("path")?.as_str().map(str::to_owned))
        .collect();

    let mut changed = false;
    for problem in problems {
        let path = format!("src/bin/{}.rs", problem.alias);
        if existing.iter().any(|p| p == &path) {
            continue;
        }
        let mut table = Table::new();
        table["name"] = toml_edit::value(bin_name(contest, &problem.alias));
        table["path"] = toml_edit::value(path);
        bins.push(table);
        changed = true;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problems(pairs: &[(&str, Option<&str>)]) -> Vec<ProblemSpec> {
        pairs
            .iter()
            .map(|(alias, screen_name)| ProblemSpec {
                alias: (*alias).to_owned(),
                screen_name: screen_name.map(str::to_owned),
            })
            .collect()
    }

    fn write(name: &str, contents: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("acrust-manifest-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Cargo.toml");
        std::fs::write(&path, contents).unwrap();
        path
    }

    const SKELETON: &str = r#"[package]
name = "abc999"
version = "0.1.0"
edition = "2024"

[package.metadata.acrust]
contest = "abc999"

[[bin]]
name = "abc999-a"
path = "src/bin/a.rs"

[dependencies]
# 手で足した依存
my-helper = "1.0"
"#;

    #[test]
    fn fills_in_the_screen_names_a_skeleton_could_not_know() {
        let path = write("skeleton", SKELETON);
        let changed = merge(
            &path,
            "abc999",
            &problems(&[("a", Some("abc999_a")), ("b", Some("abc999_b"))]),
        )
        .unwrap();
        assert!(changed);

        let text = std::fs::read_to_string(&path).unwrap();
        let parsed: toml::Table = toml::from_str(&text).unwrap();
        let acrust = &parsed["package"]["metadata"]["acrust"];
        assert_eq!(acrust["tasks"]["a"].as_str(), Some("abc999_a"));
        assert_eq!(acrust["tasks"]["b"].as_str(), Some("abc999_b"));

        // 開始前に無かった b の [[bin]] が足されている。
        let bins = parsed["bin"].as_array().unwrap();
        assert_eq!(bins.len(), 2);
        assert_eq!(bins[1]["name"].as_str(), Some("abc999-b"));
        assert_eq!(bins[1]["path"].as_str(), Some("src/bin/b.rs"));

        // 手で足した依存とコメントが残っている。
        assert!(text.contains("# 手で足した依存"), "{text}");
        assert!(text.contains("my-helper = \"1.0\""), "{text}");

        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_second_run_changes_nothing() {
        let path = write("idempotent", SKELETON);
        let specs = problems(&[("a", Some("abc999_a"))]);
        merge(&path, "abc999", &specs).unwrap();
        let after_first = std::fs::read_to_string(&path).unwrap();

        assert!(!merge(&path, "abc999", &specs).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), after_first);

        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn an_existing_screen_name_is_not_second_guessed() {
        let path = write("keep", SKELETON);
        merge(&path, "abc999", &problems(&[("a", Some("abc999_a"))])).unwrap();
        // 取り違えを手で直した状態を再現する。
        let text = std::fs::read_to_string(&path)
            .unwrap()
            .replace("abc999_a", "arc058_a");
        std::fs::write(&path, text).unwrap();

        merge(&path, "abc999", &problems(&[("a", Some("abc999_a"))])).unwrap();
        let parsed: toml::Table = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            parsed["package"]["metadata"]["acrust"]["tasks"]["a"].as_str(),
            Some("abc999_a"),
            "取得できた値が正なので上書きしてよい"
        );

        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
