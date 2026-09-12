//! Reading AtCoder's judge environment.
//!
//! With every language update AtCoder publishes an install script that embeds the
//! exact `Cargo.toml` the judge builds with. Dependencies, `Cargo.lock`, edition
//! and rustc version all come straight from there.
//!
//! Keeping these in step by hand is what goes wrong otherwise: cargo-compete
//! users routinely end up compiling against a 2023 environment while the judge
//! has moved on to 2025.

use anyhow::{anyhow, Context as _, Result};
use scraper::{Html, Selector};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// The judge environment, as read out of an install script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JudgeEnvironment {
    /// `Rust (rustc 1.89.0)`, the name shown in the submit page's language list.
    pub display: String,
    /// `1.89.0`.
    pub rustc: String,
    /// `2024`.
    pub edition: String,
    /// Everything from `[dependencies]` on, kept as text, comments included.
    pub dependencies: String,
    /// Where to fetch the `Cargo.lock` the judge builds with.
    pub cargo_lock_url: Option<String>,
}

impl JudgeEnvironment {
    /// Crate name -> requirement (`"=0.14.0"`, or a table with features).
    pub fn dependency_table(&self) -> Result<toml::Table> {
        let table: toml::Table =
            toml::from_str(&self.dependencies).context("[dependencies] is not valid TOML")?;
        match table.get("dependencies") {
            Some(toml::Value::Table(inner)) => Ok(inner.clone()),
            _ => Ok(table),
        }
    }
}

/// Finds the install script for a language on the language-list page.
pub fn find_install_script(html_text: &str, pattern: &str) -> Result<String> {
    let regex = regex::Regex::new(pattern)
        .map_err(|e| anyhow!("[submit] language-pattern is not a valid regex: {e}"))?;
    let document = Html::parse_document(html_text);
    let details = Selector::parse("details").expect("static selector");
    let summary = Selector::parse("summary").expect("static selector");
    let link = Selector::parse("a").expect("static selector");

    for section in document.select(&details) {
        let title = section
            .select(&summary)
            .next()
            .map(|s| s.text().collect::<String>().trim().to_owned())
            .unwrap_or_default();
        if !regex.is_match(&title) {
            continue;
        }
        let script = section
            .select(&link)
            .filter_map(|a| a.value().attr("href"))
            .find(|href| href.ends_with(".toml"));
        if let Some(script) = script {
            return Ok(script.to_owned());
        }
    }
    Err(anyhow!(
        "no install script in the language list matches {pattern}"
    ))
}

pub fn parse_install_script(text: &str) -> Result<JudgeEnvironment> {
    let script: toml::Table =
        toml::from_str(text).context("the install script is not valid TOML")?;
    let display = script
        .get("display")
        .and_then(|v| v.as_str())
        .context("the install script has no display")?
        .to_owned();
    let install = script
        .get("install")
        .and_then(|v| v.as_str())
        .context("the install script has no install")?;

    let rustc = capture(install, rust_version_re())
        .context("could not read the rustc version from the install script")?;
    let manifest = heredoc(install, "./Cargo.toml")
        .context("the install script does not seem to generate a Cargo.toml")?;

    let parsed: toml::Table =
        toml::from_str(&manifest).context("the judge's Cargo.toml is not valid TOML")?;
    let edition = parsed
        .get("package")
        .and_then(|p| p.get("edition"))
        .and_then(|v| v.as_str())
        .context("the judge's Cargo.toml has no edition")?
        .to_owned();

    // Kept as text rather than round-tripped through TOML: AtCoder annotates the
    // versions with comments saying which language update introduced them, and
    // re-serialising would throw those away.
    let start = manifest
        .find("[dependencies]")
        .context("the judge's Cargo.toml has no [dependencies]")?;
    let dependencies = manifest[start..].trim_end().to_owned() + "\n";

    Ok(JudgeEnvironment {
        display,
        rustc,
        edition,
        dependencies,
        cargo_lock_url: capture(install, cargo_lock_re()),
    })
}

fn rust_version_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"rust_version\s*=\s*([0-9][^\s]*)").expect("valid regex"))
}

fn cargo_lock_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"(https://\S+/Cargo\.lock)").expect("valid regex"))
}

fn capture(text: &str, regex: &regex::Regex) -> Option<String> {
    Some(regex.captures(text)?[1].to_owned())
}

/// The body of `cat > {path} << EOF`, up to the terminating `EOF`.
fn heredoc(script: &str, path: &str) -> Option<String> {
    let marker = format!("cat > {path} << EOF");
    let start = script.find(&marker)? + marker.len();
    let rest = &script[start..];
    let rest = rest.strip_prefix('\n').unwrap_or(rest);
    let end = rest
        .lines()
        .scan(0usize, |offset, line| {
            let at = *offset;
            *offset += line.len() + 1;
            Some((at, line))
        })
        .find(|(_, line)| line.trim_end() == "EOF")
        .map(|(at, _)| at)?;
    Some(rest[..end].to_owned())
}

/// What changed between two dependency tables, shown before `env update` writes.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DependencyDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    /// Crate name, then before and after.
    pub changed: Vec<(String, String, String)>,
}

impl DependencyDiff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
}

pub fn diff_dependencies(before: &toml::Table, after: &toml::Table) -> DependencyDiff {
    let describe = |value: &toml::Value| match value.as_str() {
        Some(version) => version.to_owned(),
        None => value
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_owned(),
    };
    let before: BTreeMap<&String, String> = before.iter().map(|(k, v)| (k, describe(v))).collect();
    let after: BTreeMap<&String, String> = after.iter().map(|(k, v)| (k, describe(v))).collect();

    let mut diff = DependencyDiff::default();
    for (name, version) in &after {
        match before.get(name) {
            None => diff.added.push(format!("{name} {version}")),
            Some(old) if old != version => {
                diff.changed
                    .push(((*name).clone(), old.clone(), version.clone()));
            }
            Some(_) => {}
        }
    }
    for name in before.keys() {
        if !after.contains_key(*name) {
            diff.removed.push((*name).clone());
        }
    }
    diff
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shaped like the real page: one `<details>` per language, holding a table
    /// whose row links to that language's install script.
    const LANGUAGE_LIST: &str = r#"
<html><body>
  <details><summary><code class="font-mono">Ruby (CRuby 3.3.6)</code></summary>
    <table><tbody>
      <tr><th>インストールスクリプト</th>
        <td><a href="https://img.atcoder.jp/file/language-update/2025-10/087-3-3_ruby-3-3-6.toml">087-3-3_ruby-3-3-6.toml</a></td></tr>
    </tbody></table>
  </details>
  <details><summary><code class="font-mono">Rust (rustc 1.89.0)</code></summary>
    <table><tbody>
      <tr><th>インストールスクリプト</th>
        <td><a href="https://img.atcoder.jp/file/language-update/2025-10/088-1-82-0_rustc.toml">088-1-82-0_rustc.toml</a></td></tr>
    </tbody></table>
  </details>
</body></html>
"#;

    /// A minimal install script shaped like the real one.
    const INSTALL_SCRIPT: &str = r#"
language = 'Rust'
display = 'Rust (rustc 1.89.0)'

install = '''
set -e
rust_version=1.89.0
rust_channel=1.89.0

cat > ./Cargo.toml << EOF
[profile.release]
lto = true

[package]
name = "main"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
# 202411から:
thiserror = "=2.0.16"
# 202301から:
itertools = "=0.14.0"
proconio = { version = "=0.5.0", features = ["derive"] }
EOF

curl https://raw.githubusercontent.com/rust-lang-ja/atcoder-proposal/7a724cd/Cargo.lock -fO

cargo build -vv --release
'''

compile = 'cargo build --release --quiet --offline'
"#;

    #[test]
    fn the_install_script_is_found_by_the_language_pattern() {
        let url = find_install_script(LANGUAGE_LIST, r"^Rust \(rustc").unwrap();
        assert_eq!(
            url,
            "https://img.atcoder.jp/file/language-update/2025-10/088-1-82-0_rustc.toml"
        );
    }

    #[test]
    fn a_language_that_is_not_there_says_so() {
        let err = find_install_script(LANGUAGE_LIST, r"^COBOL")
            .unwrap_err()
            .to_string();
        assert!(err.contains("COBOL"), "{err}");
    }

    #[test]
    fn the_judge_environment_comes_out_whole() {
        let environment = parse_install_script(INSTALL_SCRIPT).unwrap();
        assert_eq!(environment.display, "Rust (rustc 1.89.0)");
        assert_eq!(environment.rustc, "1.89.0");
        assert_eq!(environment.edition, "2024");
        assert_eq!(
            environment.cargo_lock_url.as_deref(),
            Some("https://raw.githubusercontent.com/rust-lang-ja/atcoder-proposal/7a724cd/Cargo.lock")
        );

        // The comments explaining where a version came from survive.
        assert!(environment.dependencies.starts_with("[dependencies]"));
        assert!(environment.dependencies.contains("# 202411から:"));
        // Nothing past the end of the heredoc is dragged in.
        assert!(!environment.dependencies.contains("cargo build"));

        let table = environment.dependency_table().unwrap();
        assert_eq!(table.len(), 3);
        assert_eq!(table["itertools"].as_str(), Some("=0.14.0"));
        assert_eq!(table["proconio"]["version"].as_str(), Some("=0.5.0"));
    }

    #[test]
    fn the_diff_names_what_changed() {
        let before: toml::Table = toml::from_str(
            r#"itertools = "=0.11.0"
proconio = { version = "=0.4.5", features = ["derive"] }
alga = "=0.9.3"
"#,
        )
        .unwrap();
        let after = parse_install_script(INSTALL_SCRIPT)
            .unwrap()
            .dependency_table()
            .unwrap();

        let diff = diff_dependencies(&before, &after);
        assert!(!diff.is_empty());
        assert_eq!(diff.added, ["thiserror =2.0.16"]);
        assert_eq!(diff.removed, ["alga"]);
        assert_eq!(
            diff.changed,
            [
                (
                    "itertools".to_owned(),
                    "=0.11.0".to_owned(),
                    "=0.14.0".to_owned()
                ),
                (
                    "proconio".to_owned(),
                    "=0.4.5".to_owned(),
                    "=0.5.0".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn an_unchanged_environment_produces_an_empty_diff() {
        let table = parse_install_script(INSTALL_SCRIPT)
            .unwrap()
            .dependency_table()
            .unwrap();
        assert!(diff_dependencies(&table, &table).is_empty());
    }
}
