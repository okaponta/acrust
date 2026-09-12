//! コンテストパッケージと問題の解決（設計 §4.6）。
//!
//! 問題URL（task screen name）は `{contest}_{alias}` から導出できない（設計 §4.8）ため、
//! `Cargo.toml` の `[package.metadata.acrust.tasks]` を唯一の正とする。

use crate::config::ResolveMode;
use anyhow::{anyhow, bail, Context as _, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// `acrust new` 直後かどうかを判定する mtime の許容差。
const FRESH_MTIME_SPREAD: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bin {
    /// `[[bin]] name`（例: `abc474-c`）。
    pub name: String,
    /// `src/bin/{alias}.rs` のファイル名（例: `c`）。
    pub alias: String,
    pub src_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Package {
    pub dir: PathBuf,
    pub manifest_path: PathBuf,
    pub name: String,
    /// `[package.metadata.acrust] contest`。
    pub contest: String,
    /// alias -> task screen name（例: `c` -> `arc058_a`）。
    pub tasks: BTreeMap<String, String>,
    pub bins: Vec<Bin>,
}

impl Package {
    /// `start` から上に辿って `Cargo.toml` を探し、acrust のパッケージとして読み込む。
    pub fn find_from(start: &Path) -> Result<Self> {
        let manifest_path = start
            .ancestors()
            .map(|dir| dir.join("Cargo.toml"))
            .find(|p| p.is_file())
            .ok_or_else(|| {
                anyhow!(
                    "Cargo.toml not found (looked upwards from {}). \
                     Run this inside a contest directory",
                    start.display()
                )
            })?;
        Self::load(&manifest_path)
    }

    pub fn find() -> Result<Self> {
        let cwd = std::env::current_dir().context("could not get the current directory")?;
        Self::find_from(&cwd)
    }

    pub fn load(manifest_path: &Path) -> Result<Self> {
        let metadata = cargo_metadata::MetadataCommand::new()
            .manifest_path(manifest_path)
            .no_deps()
            .exec()
            .with_context(|| format!("could not read {}", manifest_path.display()))?;
        let package = metadata
            .root_package()
            .ok_or_else(|| anyhow!("{} has no package", manifest_path.display()))?;

        let dir = manifest_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();

        let acrust = package.metadata.get("acrust");
        if acrust.is_none() {
            if package.metadata.get("cargo-compete").is_some() {
                bail!(
                    "{} is still in the cargo-compete layout. Run `acrust migrate`",
                    dir.display()
                );
            }
            bail!(
                "{} has no [package.metadata.acrust]. \
                 Either acrust did not create it, or it still needs `acrust migrate`",
                dir.display()
            );
        }
        let acrust = acrust.expect("checked above");

        let contest = acrust
            .get("contest")
            .and_then(|v| v.as_str())
            .unwrap_or(package.name.as_str())
            .to_owned();

        let mut tasks = BTreeMap::new();
        if let Some(table) = acrust.get("tasks").and_then(|v| v.as_object()) {
            for (alias, screen_name) in table {
                let screen_name = screen_name.as_str().ok_or_else(|| {
                    anyhow!(
                        "{alias} in [package.metadata.acrust.tasks] is not a string ({})",
                        manifest_path.display()
                    )
                })?;
                tasks.insert(alias.clone(), screen_name.to_owned());
            }
        }

        let mut bins = package
            .targets
            .iter()
            .filter(|t| t.is_bin())
            .map(|t| {
                let src_path = PathBuf::from(t.src_path.as_str());
                let alias = src_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(t.name.as_str())
                    .to_owned();
                Bin {
                    name: t.name.clone(),
                    alias,
                    src_path,
                }
            })
            .collect::<Vec<_>>();
        bins.sort_by(|a, b| a.alias.cmp(&b.alias));

        Ok(Self {
            dir,
            manifest_path: manifest_path.to_path_buf(),
            name: package.name.to_string(),
            contest,
            tasks,
            bins,
        })
    }

    pub fn find_bin(&self, query: &str) -> Option<&Bin> {
        let query = query.trim();
        // "abc474-c" / "abc474_c" のような指定も alias 部分だけ見れば足りる。
        let alias = query
            .rsplit(['-', '_'])
            .next()
            .unwrap_or(query)
            .to_ascii_lowercase();
        self.bins
            .iter()
            .find(|b| b.name.eq_ignore_ascii_case(query))
            .or_else(|| {
                self.bins
                    .iter()
                    .find(|b| b.alias.eq_ignore_ascii_case(query))
            })
            .or_else(|| {
                self.bins
                    .iter()
                    .find(|b| b.alias.eq_ignore_ascii_case(&alias))
            })
    }

    /// 問題の URL。screen name はメタデータからしか引けない（設計 §4.8）。
    // fetch / submit / open（M2 以降）が使う。
    #[allow(dead_code)]
    pub fn task_url(&self, alias: &str) -> Result<String> {
        let screen_name = self.tasks.get(alias).ok_or_else(|| {
            anyhow!(
                "{} has no task screen name for problem {alias} in {}. \
                 Run `acrust fetch` to fill the metadata in",
                self.contest,
                self.manifest_path.display()
            )
        })?;
        Ok(format!(
            "https://atcoder.jp/contests/{}/tasks/{screen_name}",
            self.contest
        ))
    }
}

/// 問題をどうやって決めたか。`submit` は推定時だけ y/N 確認を入れる（決定 D7）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Explicit,
    Inferred,
}

#[derive(Debug, Clone)]
pub struct Resolved {
    pub bin: Bin,
    pub origin: Origin,
}

impl Resolved {
    /// `abc474 c (src/bin/c.rs)` のような表示。推定したときは必ず出す。
    pub fn describe(&self, package: &Package) -> String {
        let src = self
            .bin
            .src_path
            .strip_prefix(&package.dir)
            .unwrap_or(&self.bin.src_path);
        format!("{} {} ({})", package.contest, self.bin.alias, src.display())
    }
}

/// 引数省略時の問題推定（設計 §4.6）。
pub fn resolve_problem(
    package: &Package,
    query: Option<&str>,
    mode: ResolveMode,
    template_src: Option<&str>,
) -> Result<Resolved> {
    if let Some(query) = query {
        let bin = package.find_bin(query).ok_or_else(|| {
            anyhow!(
                "{} has no problem {query}. Candidates: {}",
                package.name,
                aliases(package)
            )
        })?;
        return Ok(Resolved {
            bin: bin.clone(),
            origin: Origin::Explicit,
        });
    }

    if package.bins.is_empty() {
        bail!("{} has no bin targets", package.name);
    }
    if package.bins.len() == 1 {
        return Ok(Resolved {
            bin: package.bins[0].clone(),
            origin: Origin::Inferred,
        });
    }

    match mode {
        ResolveMode::Never => bail!(
            "name a problem ([test] resolve = \"never\"). Candidates: {}",
            aliases(package)
        ),
        ResolveMode::Single => bail!(
            "there are {} bins, so name a problem ([test] resolve = \"single\"). Candidates: {}",
            package.bins.len(),
            aliases(package)
        ),
        ResolveMode::Mtime => resolve_by_mtime(package, template_src),
    }
}

/// `acrust new` 直後は 7 ファイルの mtime がほぼ同時刻になるため、推定を拒否する（設計 §4.6）。
fn resolve_by_mtime(package: &Package, template_src: Option<&str>) -> Result<Resolved> {
    let mut stats = Vec::with_capacity(package.bins.len());
    for bin in &package.bins {
        let mtime = std::fs::metadata(&bin.src_path)
            .and_then(|m| m.modified())
            .with_context(|| format!("could not get the mtime of {}", bin.src_path.display()))?;
        let source = std::fs::read_to_string(&bin.src_path)
            .with_context(|| format!("could not read {}", bin.src_path.display()))?;
        stats.push((bin, mtime, source));
    }

    if is_freshly_generated(&stats, template_src) {
        bail!(
            "cannot tell which problem you mean (everything is still the template). \
             Name one. Candidates: {}",
            aliases(package)
        );
    }

    let newest = stats
        .iter()
        .map(|(_, mtime, _)| *mtime)
        .max()
        .expect("bins is non-empty");
    let newest_bins: Vec<&Bin> = stats
        .iter()
        .filter(|(_, mtime, _)| *mtime == newest)
        .map(|(bin, _, _)| *bin)
        .collect();

    // mtime が同着のときにどれかを黙って選ぶと、submit で別の問題を提出しかねない。
    // 誤推定のコストが非対称なので（決定 D7）、決められないときは決められないと言う。
    if newest_bins.len() > 1 {
        bail!(
            "cannot tell which problem you mean ({} share an mtime). Name one",
            newest_bins
                .iter()
                .map(|b| b.alias.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    Ok(Resolved {
        bin: newest_bins[0].clone(),
        origin: Origin::Inferred,
    })
}

fn is_freshly_generated(stats: &[(&Bin, SystemTime, String)], template_src: Option<&str>) -> bool {
    let Some(oldest) = stats.iter().map(|(_, m, _)| *m).min() else {
        return false;
    };
    let Some(newest) = stats.iter().map(|(_, m, _)| *m).max() else {
        return false;
    };
    let within_spread = newest
        .duration_since(oldest)
        .map(|d| d <= FRESH_MTIME_SPREAD)
        .unwrap_or(false);
    if !within_spread {
        return false;
    }
    match template_src {
        // テンプレートが読めるなら「全部テンプレートのまま」を条件にする。
        Some(template) => stats.iter().all(|(_, _, src)| src == template),
        // 読めないときは「全部同じ内容」で代用する。
        None => stats.windows(2).all(|w| w[0].2 == w[1].2),
    }
}

fn aliases(package: &Package) -> String {
    package
        .bins
        .iter()
        .map(|b| b.alias.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bin(alias: &str) -> Bin {
        Bin {
            name: format!("abc474-{alias}"),
            alias: alias.to_owned(),
            src_path: PathBuf::from(format!("/repo/abc474/src/bin/{alias}.rs")),
        }
    }

    fn package() -> Package {
        Package {
            dir: PathBuf::from("/repo/abc474"),
            manifest_path: PathBuf::from("/repo/abc474/Cargo.toml"),
            name: "abc474".to_owned(),
            contest: "abc474".to_owned(),
            tasks: [("a", "abc474_a"), ("c", "arc058_a")]
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            bins: vec![bin("a"), bin("b"), bin("c")],
        }
    }

    #[test]
    fn finds_a_bin_by_alias_bin_name_or_qualified_name() {
        let package = package();
        assert_eq!(package.find_bin("c").unwrap().alias, "c");
        assert_eq!(package.find_bin("C").unwrap().alias, "c");
        assert_eq!(package.find_bin("abc474-c").unwrap().alias, "c");
        assert_eq!(package.find_bin("abc474_c").unwrap().alias, "c");
        assert!(package.find_bin("z").is_none());
    }

    #[test]
    fn task_url_comes_from_metadata_not_from_the_alias() {
        let package = package();
        // abc474 の C が arc058_a を指す、という導出できない対応を保てている。
        assert_eq!(
            package.task_url("c").unwrap(),
            "https://atcoder.jp/contests/abc474/tasks/arc058_a"
        );
        assert!(package
            .task_url("b")
            .unwrap_err()
            .to_string()
            .contains("acrust fetch"));
    }

    #[test]
    fn an_explicit_argument_wins_and_is_marked_explicit() {
        let package = package();
        let resolved = resolve_problem(&package, Some("b"), ResolveMode::Never, None).unwrap();
        assert_eq!(resolved.bin.alias, "b");
        assert_eq!(resolved.origin, Origin::Explicit);
    }

    #[test]
    fn a_single_bin_needs_no_inference_mode() {
        let mut package = package();
        package.bins = vec![bin("a")];
        let resolved = resolve_problem(&package, None, ResolveMode::Never, None).unwrap();
        assert_eq!(resolved.bin.alias, "a");
        assert_eq!(resolved.origin, Origin::Inferred);
    }

    #[test]
    fn never_and_single_refuse_to_guess_and_list_the_candidates() {
        let package = package();
        for mode in [ResolveMode::Never, ResolveMode::Single] {
            let err = resolve_problem(&package, None, mode, None)
                .unwrap_err()
                .to_string();
            assert!(err.contains("a, b, c"), "{err}");
        }
    }

    #[test]
    fn freshly_generated_packages_are_ambiguous() {
        let now = SystemTime::now();
        let bins = [bin("a"), bin("b"), bin("c")];
        let stats: Vec<_> = bins
            .iter()
            .map(|b| (b, now, "TEMPLATE\n".to_owned()))
            .collect();
        assert!(is_freshly_generated(&stats, Some("TEMPLATE\n")));
        assert!(is_freshly_generated(&stats, None));

        // 1 つでも編集されていれば推定してよい。
        let mut edited = stats.clone();
        edited[1].2 = "solved\n".to_owned();
        assert!(!is_freshly_generated(&edited, Some("TEMPLATE\n")));
        assert!(!is_freshly_generated(&edited, None));
    }

    #[test]
    fn a_wide_mtime_spread_is_not_fresh_even_if_untouched() {
        let now = SystemTime::now();
        let bins = [bin("a"), bin("b")];
        let stats = vec![
            (&bins[0], now - Duration::from_secs(3600), "T".to_owned()),
            (&bins[1], now, "T".to_owned()),
        ];
        assert!(!is_freshly_generated(&stats, Some("T")));
    }
}
