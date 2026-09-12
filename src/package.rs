//! Generating a contest package.
//!
//! The metadata in the generated `Cargo.toml` holds only what cannot be derived.
//! Alias, file name and bin name all follow from one another; the task screen
//! name does not — problem C of `abc042` is `arc058_a` — so it is written down in
//! `[package.metadata.acrust.tasks]` and kept.

use crate::config::LoadedConfig;
use anyhow::{bail, Context as _, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// What generating one problem needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProblemSpec {
    pub alias: String,
    /// `None` when it could not be read yet.
    pub screen_name: Option<String>,
}

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
            "\n# The problem URL cannot be derived from the alias (c in abc042 is arc058_a). Keep this.\n",
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
    out.push_str(strip_dependencies_header(dependencies).trim_end());
    out.push('\n');
    Ok(out)
}

/// Drops a leading `[dependencies]` heading from the template.
///
/// The template is the *body* of the section. With the heading left in, the
/// generated `Cargo.toml` carries it twice and Cargo refuses the redefinition.
pub fn strip_dependencies_header(dependencies: &str) -> &str {
    let trimmed = dependencies.trim_start();
    match trimmed.strip_prefix("[dependencies]") {
        Some(rest) => rest.trim_start_matches(['\r', '\n']),
        None => dependencies,
    }
}

pub fn bin_name(contest: &str, alias: &str) -> String {
    format!("{contest}-{alias}")
}

/// Turns the config's `[package] profile` (raw TOML starting at `[dev]`) into a
/// real `[profile.dev]`.
fn render_profile(profile: &str) -> Result<String> {
    let table: toml::Table =
        toml::from_str(profile).context("[package] profile is not valid TOML")?;
    let wrapped = toml::Table::from_iter([("profile".to_owned(), toml::Value::Table(table))]);
    toml::to_string(&wrapped).context("could not build [profile]")
}

/// An alias is alphanumeric and needs no quoting, but check rather than assume.
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

/// What was written, for the closing report.
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

/// Writes `src/bin/{alias}.rs` from the template. An existing file is never
/// touched — that is someone's solution.
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

/// Copies `copy/` from the template into the package, keeping existing files.
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
    std::fs::create_dir_all(to).with_context(|| format!("could not create {}", to.display()))?;
    let entries =
        std::fs::read_dir(from).with_context(|| format!("could not read {}", from.display()))?;
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
                    "could not copy {} to {}",
                    source.display(),
                    destination.display()
                )
            })?;
            written.created.push(relative(package_dir, &destination));
        }
    }
    Ok(())
}

/// Puts the judge's `Cargo.lock` in place, warning but carrying on without one.
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
            "{} is missing. Run `acrust env update` to get the same Cargo.lock the judge uses",
            source.display()
        ));
        return Ok(());
    }
    std::fs::copy(&source, &destination)
        .with_context(|| format!("could not put {} in place", destination.display()))?;
    written.created.push("Cargo.lock".to_owned());
    Ok(())
}

pub fn write_new_file(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    std::fs::write(path, contents).with_context(|| format!("could not write {}", path.display()))
}

pub fn relative(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// Reads the template's `[dependencies]`, saying what is missing if it is not there.
pub fn read_dependencies(config: &LoadedConfig) -> Result<String> {
    let path = config.template_dependencies();
    if !path.is_file() {
        bail!(
            "{} is missing. Run `acrust init` or `acrust env update`",
            path.display()
        );
    }
    std::fs::read_to_string(&path).with_context(|| format!("could not read {}", path.display()))
}

/// Reads the template's `main.rs`, falling back to an empty `main()`.
pub fn read_template_source(config: &LoadedConfig) -> Result<String> {
    let path = config.template_src();
    if !path.is_file() {
        crate::ui::warn(&format!(
            "{} is missing, so using an empty main()",
            path.display()
        ));
        return Ok("fn main() {\n}\n".to_owned());
    }
    std::fs::read_to_string(&path).with_context(|| format!("could not read {}", path.display()))
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
        // A problem whose screen name is unknown stays out of tasks; a later
        // fetch fills it in.
        assert!(acrust["tasks"].get("d").is_none());

        let bins = parsed["bin"].as_array().unwrap();
        assert_eq!(bins.len(), 3);
        assert_eq!(bins[0]["name"].as_str(), Some("abc042-a"));
        assert_eq!(bins[0]["path"].as_str(), Some("src/bin/a.rs"));

        // Cargo only reads this as [profile.dev], never as [dev].
        assert_eq!(parsed["profile"]["dev"]["opt-level"].as_integer(), Some(3));
        assert_eq!(parsed["dependencies"]["proconio"].as_str(), Some("=0.5.0"));
    }

    #[test]
    fn a_dependencies_header_in_the_template_is_not_duplicated() {
        // A template written by `acrust env update` can carry the heading, and
        // pasting that in would give the manifest two [dependencies] sections.
        let manifest = render_manifest(
            "abc474",
            &problems(),
            "2024",
            "",
            "[dependencies]\nproconio = \"=0.5.0\"\n",
        )
        .unwrap();
        assert_eq!(manifest.matches("[dependencies]").count(), 1, "{manifest}");
        let parsed: toml::Table = toml::from_str(&manifest).unwrap();
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
        std::fs::write(dir.join("src/bin/a.rs"), "// my solution\n").unwrap();

        let mut written = Written::default();
        write_sources(&dir, &problems(), "TEMPLATE\n", &mut written).unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.join("src/bin/a.rs")).unwrap(),
            "// my solution\n"
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
