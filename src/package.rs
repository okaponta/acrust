//! コンテストパッケージの生成（設計 §4.3 / §4.8）。
//!
//! 生成する `Cargo.toml` には **規則から導けない情報だけ**をメタデータとして書く。
//! alias とファイル名と bin 名は互いに導けるが、task screen name だけは導けないので
//! `[package.metadata.acrust.tasks]` に必ず残す（`abc042` の C が `arc058_a` になる類）。

use crate::config::LoadedConfig;
use anyhow::{bail, Context as _, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 1問ぶんの、パッケージ生成に必要な情報。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProblemSpec {
    pub alias: String,
    /// 取れていないときは `None`（コンテスト開始前にスケルトンだけ作った場合）。
    pub screen_name: Option<String>,
}

/// `Cargo.toml` の中身を組み立てる。
pub fn render_manifest(
    contest: &str,
    problems: &[ProblemSpec],
    edition: &str,
    profile: &str,
    dependencies: &str,
) -> Result<String> {
    let mut out = String::new();
    out.push_str("[package]\n");
    out.push_str(&format!("name = {}\n", toml_string(contest)));
    out.push_str("version = \"0.1.0\"\n");
    out.push_str(&format!("edition = {}\n", toml_string(edition)));

    out.push_str("\n[package.metadata.acrust]\n");
    out.push_str(&format!("contest = {}\n", toml_string(contest)));

    let tasks: BTreeMap<&str, &str> = problems
        .iter()
        .filter_map(|p| Some((p.alias.as_str(), p.screen_name.as_deref()?)))
        .collect();
    if !tasks.is_empty() {
        out.push_str(
            "\n# 問題URLは alias から導けない（abc042 の c は arc058_a）。消さないこと。\n",
        );
        out.push_str("[package.metadata.acrust.tasks]\n");
        for (alias, screen_name) in &tasks {
            out.push_str(&format!(
                "{} = {}\n",
                bare_key(alias),
                toml_string(screen_name)
            ));
        }
    }

    for problem in problems {
        out.push_str("\n[[bin]]\n");
        out.push_str(&format!(
            "name = {}\n",
            toml_string(&bin_name(contest, &problem.alias))
        ));
        out.push_str(&format!(
            "path = {}\n",
            toml_string(&format!("src/bin/{}.rs", problem.alias))
        ));
    }

    if !profile.trim().is_empty() {
        out.push('\n');
        out.push_str(&render_profile(profile)?);
    }

    out.push_str("\n[dependencies]\n");
    out.push_str(dependencies.trim_end());
    out.push('\n');
    Ok(out)
}

pub fn bin_name(contest: &str, alias: &str) -> String {
    format!("{contest}-{alias}")
}

/// 設定の `[package] profile`（`[dev]` 始まりの生 TOML）を `[profile.dev]` に直す。
fn render_profile(profile: &str) -> Result<String> {
    let table: toml::Table =
        toml::from_str(profile).context("[package] profile が TOML として読めません")?;
    let wrapped = toml::Table::from_iter([("profile".to_owned(), toml::Value::Table(table))]);
    toml::to_string(&wrapped).context("[profile] を組み立てられませんでした")
}

/// alias は英数字だけなので裸のキーで書けるが、念のため確認する。
fn bare_key(key: &str) -> String {
    if !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        key.to_owned()
    } else {
        toml_string(key)
    }
}

fn toml_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// 生成したファイルの記録。表示にだけ使う。
#[derive(Debug, Default)]
pub struct Written {
    pub created: Vec<String>,
    pub updated: Vec<String>,
    pub kept: Vec<String>,
}

impl Written {
    pub fn is_empty(&self) -> bool {
        self.created.is_empty() && self.updated.is_empty()
    }
}

/// `src/bin/{alias}.rs` をテンプレートから作る。**既にあるものは絶対に上書きしない**（解答が入っている）。
pub fn write_sources(
    package_dir: &Path,
    problems: &[ProblemSpec],
    template: &str,
    written: &mut Written,
) -> Result<()> {
    for problem in problems {
        let path = package_dir.join(format!("src/bin/{}.rs", problem.alias));
        if path.exists() {
            written.kept.push(relative(package_dir, &path));
            continue;
        }
        write_new_file(&path, template)?;
        written.created.push(relative(package_dir, &path));
    }
    Ok(())
}

/// テンプレートの `copy/` 以下をパッケージ直下へ複製する。既存ファイルは残す。
pub fn copy_template_dir(from: &Path, package_dir: &Path, written: &mut Written) -> Result<()> {
    if !from.is_dir() {
        return Ok(());
    }
    copy_dir_recursive(from, package_dir, package_dir, written)
}

fn copy_dir_recursive(
    from: &Path,
    to: &Path,
    package_dir: &Path,
    written: &mut Written,
) -> Result<()> {
    std::fs::create_dir_all(to).with_context(|| format!("{} を作れませんでした", to.display()))?;
    let entries = std::fs::read_dir(from)
        .with_context(|| format!("{} を読めませんでした", from.display()))?;
    for entry in entries {
        let entry = entry?;
        let source = entry.path();
        let destination = to.join(entry.file_name());
        if source.is_dir() {
            copy_dir_recursive(&source, &destination, package_dir, written)?;
        } else if destination.exists() {
            written.kept.push(relative(package_dir, &destination));
        } else {
            std::fs::copy(&source, &destination).with_context(|| {
                format!(
                    "{} を {} にコピーできませんでした",
                    source.display(),
                    destination.display()
                )
            })?;
            written.created.push(relative(package_dir, &destination));
        }
    }
    Ok(())
}

/// ジャッジと同じ `Cargo.lock` を置く。無いときは警告だけして続ける。
pub fn copy_cargo_lock(
    config: &LoadedConfig,
    package_dir: &Path,
    written: &mut Written,
) -> Result<()> {
    let source = config.template_cargo_lock();
    let destination = package_dir.join("Cargo.lock");
    if destination.exists() {
        written.kept.push("Cargo.lock".to_owned());
        return Ok(());
    }
    if !source.is_file() {
        crate::ui::warn(&format!(
            "{} がありません。`acrust env update` でジャッジと同じ Cargo.lock を取得してください",
            source.display()
        ));
        return Ok(());
    }
    std::fs::copy(&source, &destination)
        .with_context(|| format!("{} を置けませんでした", destination.display()))?;
    written.created.push("Cargo.lock".to_owned());
    Ok(())
}

pub fn write_new_file(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("{} を作れませんでした", parent.display()))?;
    }
    std::fs::write(path, contents).with_context(|| format!("{} に書けませんでした", path.display()))
}

pub fn relative(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// テンプレートの `[dependencies]` を読む。無ければ何が足りないかを言う。
pub fn read_dependencies(config: &LoadedConfig) -> Result<String> {
    let path = config.template_dependencies();
    if !path.is_file() {
        bail!(
            "{} がありません。`acrust init` か `acrust env update` を実行してください",
            path.display()
        );
    }
    std::fs::read_to_string(&path).with_context(|| format!("{} を読めませんでした", path.display()))
}

/// テンプレートの `main.rs` を読む。無ければ空のテンプレートで代用する。
pub fn read_template_source(config: &LoadedConfig) -> Result<String> {
    let path = config.template_src();
    if !path.is_file() {
        crate::ui::warn(&format!(
            "{} がありません。空の main() で作ります",
            path.display()
        ));
        return Ok("fn main() {\n}\n".to_owned());
    }
    std::fs::read_to_string(&path).with_context(|| format!("{} を読めませんでした", path.display()))
}

pub fn manifest_path(package_dir: &Path) -> PathBuf {
    package_dir.join("Cargo.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problems() -> Vec<ProblemSpec> {
        [
            ("a", Some("abc042_a")),
            ("c", Some("arc058_a")),
            ("d", None),
        ]
        .iter()
        .map(|(alias, screen_name)| ProblemSpec {
            alias: (*alias).to_owned(),
            screen_name: screen_name.map(str::to_owned),
        })
        .collect()
    }

    #[test]
    fn the_manifest_parses_and_keeps_the_screen_names() {
        let manifest = render_manifest(
            "abc042",
            &problems(),
            "2024",
            "[dev]\nopt-level = 3\n",
            "proconio = \"=0.5.0\"\n",
        )
        .unwrap();
        let parsed: toml::Table = toml::from_str(&manifest).unwrap();

        assert_eq!(parsed["package"]["name"].as_str(), Some("abc042"));
        assert_eq!(parsed["package"]["edition"].as_str(), Some("2024"));

        let acrust = &parsed["package"]["metadata"]["acrust"];
        assert_eq!(acrust["contest"].as_str(), Some("abc042"));
        assert_eq!(acrust["tasks"]["c"].as_str(), Some("arc058_a"));
        // 取れなかった問題は tasks に載せない（あとで fetch が埋める）。
        assert!(acrust["tasks"].get("d").is_none());

        let bins = parsed["bin"].as_array().unwrap();
        assert_eq!(bins.len(), 3);
        assert_eq!(bins[0]["name"].as_str(), Some("abc042-a"));
        assert_eq!(bins[0]["path"].as_str(), Some("src/bin/a.rs"));

        // [dev] は [profile.dev] にならないと Cargo が読まない。
        assert_eq!(parsed["profile"]["dev"]["opt-level"].as_integer(), Some(3));
        assert_eq!(parsed["dependencies"]["proconio"].as_str(), Some("=0.5.0"));
    }

    #[test]
    fn a_contest_before_it_starts_has_no_tasks_table() {
        let problems: Vec<ProblemSpec> = ["a", "b"]
            .iter()
            .map(|alias| ProblemSpec {
                alias: (*alias).to_owned(),
                screen_name: None,
            })
            .collect();
        let manifest = render_manifest("abc999", &problems, "2024", "", "").unwrap();
        let parsed: toml::Table = toml::from_str(&manifest).unwrap();
        assert!(parsed["package"]["metadata"]["acrust"]
            .get("tasks")
            .is_none());
        assert_eq!(parsed["bin"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn existing_sources_are_never_overwritten() {
        let dir = std::env::temp_dir().join(format!("acrust-pkg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src/bin")).unwrap();
        std::fs::write(dir.join("src/bin/a.rs"), "// 解答\n").unwrap();

        let mut written = Written::default();
        write_sources(&dir, &problems(), "TEMPLATE\n", &mut written).unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.join("src/bin/a.rs")).unwrap(),
            "// 解答\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("src/bin/c.rs")).unwrap(),
            "TEMPLATE\n"
        );
        assert_eq!(written.kept, ["src/bin/a.rs"]);
        assert_eq!(written.created.len(), 2);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
