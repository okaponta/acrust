//! `acrust migrate`（設計 §4.8・決定 D4）。
//!
//! cargo-compete 形式から acrust 形式への**一度きり**の変換。
//! cargo-compete 形式を読むのはこのサブコマンドの中だけで、`test` や `submit` は
//! acrust 形式しか見ない。互換シムがコード全体に散らばらず、将来 migrate ごと消せる。
//!
//! 変換は情報の名前替えにすぎず、失われる情報が無い。それを毎回確かめるために
//! **往復検証**を行い、1 件でも合わなければ何も書かずに中断する。

use crate::commands::init;
use crate::package;
use crate::snowchains::{self, BinEntry, CompeteConfig};
use crate::testcases::TestSuite;
use crate::ui;
use anyhow::{anyhow, bail, Context as _, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const COMPETE_FILE: &str = "compete.toml";

pub fn run(write: bool, allow_dirty: bool) -> Result<()> {
    let root = find_root()?;
    ui::field("target", &root.display().to_string());

    if !allow_dirty {
        ensure_clean(&root)?;
    }
    let summary = migrate_at(&root, write)?;

    if !write {
        ui::info("");
        ui::info("This was a dry run. Pass --write to actually change things");
        return Ok(());
    }
    let _ = summary;
    ui::info("");
    ui::ok("migrated");
    ui::info("");
    ui::info("Next:");
    ui::info(crate::commands::NEXT_ENV_UPDATE);
    ui::info("  acrust test a       # check that one problem still works");
    ui::info("");
    ui::info("To undo this before committing, run `git checkout .`");
    Ok(())
}

/// 移行の規模。表示とテストに使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    pub packages: usize,
    pub bins: usize,
    pub files: usize,
    pub cases: usize,
}

/// `root` の cargo-compete リポジトリを移行する。git の状態は見ない（呼び出し側の責任）。
///
/// 往復検証は `write` が false でも必ず行う。1 件でも合わなければ何も書かずにエラーを返す。
pub fn migrate_at(root: &Path, write: bool) -> Result<Summary> {
    let compete = std::fs::read_to_string(root.join(COMPETE_FILE))
        .with_context(|| format!("could not read {}", root.join(COMPETE_FILE).display()))?;
    let compete = snowchains::parse_compete_config(&compete)?;

    let packages = collect_packages(root)?;
    if packages.is_empty() {
        bail!("found no cargo-compete packages at all");
    }

    let plan = Plan::build(root, packages)?;
    plan.report(&compete, write);
    let summary = plan.summary();

    if write {
        plan.apply(root, &compete)?;
    }
    Ok(summary)
}

/// `compete.toml` を持つディレクトリ。
fn find_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("could not get the current directory")?;
    cwd.ancestors()
        .find(|dir| dir.join(COMPETE_FILE).is_file())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            anyhow!(
                "{COMPETE_FILE} not found (looked upwards from {}). \
                 Run this inside a cargo-compete repository",
                cwd.display()
            )
        })
}

/// 取り消せる状態であることを確かめる。移行は多数のファイルを書き換えるため。
fn ensure_clean(root: &Path) -> Result<()> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["status", "--porcelain"])
        .output()
        .context("could not start git")?;
    if !output.status.success() {
        bail!(
            "{} does not look like a git repository. Pass --allow-dirty to go ahead anyway",
            root.display()
        );
    }
    if !output.stdout.is_empty() {
        bail!("there are uncommitted changes. Commit them first, or pass --allow-dirty");
    }
    Ok(())
}

/// 1 パッケージぶんの変換内容。
struct PackagePlan {
    dir: PathBuf,
    contest: String,
    /// alias -> task screen name。
    tasks: BTreeMap<String, String>,
    /// (yml のパス, toml のパス, 変換結果)。
    suites: Vec<(PathBuf, PathBuf, TestSuite)>,
    bins: Vec<BinEntry>,
}

struct Plan {
    packages: Vec<PackagePlan>,
    cases: usize,
}

impl Plan {
    fn build(root: &Path, manifests: Vec<PathBuf>) -> Result<Self> {
        let mut packages = Vec::new();
        let mut cases = 0;

        for manifest_path in manifests {
            let dir = manifest_path.parent().unwrap_or(root).to_path_buf();
            let text = std::fs::read_to_string(&manifest_path)
                .with_context(|| format!("could not read {}", manifest_path.display()))?;
            let bins = snowchains::parse_bins(&text)
                .with_context(|| format!("could not read {}", manifest_path.display()))?;
            if bins.is_empty() {
                continue;
            }

            let mut contest: Option<String> = None;
            let mut tasks = BTreeMap::new();
            for bin in &bins {
                let (bin_contest, task) = bin.contest_and_task().with_context(|| {
                    format!("could not read {} in {}", manifest_path.display(), bin.name)
                })?;
                match &contest {
                    None => contest = Some(bin_contest),
                    Some(existing) if existing == &bin_contest => {}
                    Some(existing) => bail!(
                        "{} mixes contests ({existing} and {bin_contest})",
                        manifest_path.display()
                    ),
                }
                tasks.insert(bin.alias.clone(), task);
            }
            let contest = contest.expect("bins is non-empty");

            // ここで往復検証その1: メタデータから組み直した URL が元と一致すること。
            for bin in &bins {
                let rebuilt = snowchains::rebuild_problem_url(&contest, &tasks, &bin.alias)
                    .ok_or_else(|| {
                        anyhow!(
                            "cannot rebuild {} in {}",
                            manifest_path.display(),
                            bin.alias
                        )
                    })?;
                if rebuilt != bin.problem {
                    bail!(
                        "the problem URL for {1} in {0} does not match:\n  before: {2}\n  after:  {rebuilt}",
                        manifest_path.display(),
                        bin.alias,
                        bin.problem
                    );
                }
            }

            let suites = convert_testcases(&dir)?;
            cases += suites.iter().map(|(_, _, s)| s.cases.len()).sum::<usize>();

            packages.push(PackagePlan {
                dir,
                contest,
                tasks,
                suites,
                bins,
            });
        }

        Ok(Self { packages, cases })
    }

    fn summary(&self) -> Summary {
        Summary {
            packages: self.packages.len(),
            bins: self.packages.iter().map(|p| p.bins.len()).sum(),
            files: self.packages.iter().map(|p| p.suites.len()).sum(),
            cases: self.cases,
        }
    }

    /// 書き換えの内容は `--write` のときだけ出す。
    /// 下見では 1 バイトも触らないので、何を作る・消すと並べても読む意味がない。
    fn report(&self, compete: &CompeteConfig, write: bool) {
        let bins: usize = self.packages.iter().map(|p| p.bins.len()).sum();
        let files: usize = self.packages.iter().map(|p| p.suites.len()).sum();
        ui::info("");
        ui::section("What will be migrated");
        ui::field("packages", &format!("{}", self.packages.len()));
        ui::field("bins", &format!("{bins}"));
        ui::field(
            "test cases",
            &format!("{files} files / {} cases", self.cases),
        );
        if write {
            ui::field(
                "config",
                "creates .acrust/config.toml and .acrust/template/",
            );
            if compete.template_src.is_some() {
                ui::field(
                    "template",
                    "moves the src in compete.toml out to template/main.rs",
                );
            }
            if compete.cargo_lock.is_some() {
                ui::field(
                    "Cargo.lock",
                    "turns template-cargo-lock.toml into template/Cargo.lock",
                );
            }
            ui::field(
                "removes",
                "compete.toml / template-cargo-lock.toml / testcases/*.yml",
            );
        }
        if let Some(language_id) = &compete.language_id {
            ui::info("");
            ui::warn(&format!(
                "not carrying over language_id = \"{language_id}\" from compete.toml. \
                 acrust reads it off the submit page instead"
            ));
        }
    }

    fn apply(&self, root: &Path, compete: &CompeteConfig) -> Result<()> {
        write_settings(root, compete)?;

        for package in &self.packages {
            rewrite_manifest(package)?;
            for (yaml_path, toml_path, suite) in &package.suites {
                suite.save(toml_path)?;
                std::fs::remove_file(yaml_path)
                    .with_context(|| format!("could not delete {}", yaml_path.display()))?;
            }
        }

        for name in [COMPETE_FILE, "template-cargo-lock.toml"] {
            let path = root.join(name);
            if path.is_file() {
                std::fs::remove_file(&path)
                    .with_context(|| format!("could not delete {}", path.display()))?;
            }
        }
        Ok(())
    }
}

/// `{contest}/testcases/*.yml` を acrust の形に変換する（まだ書かない）。
fn convert_testcases(package_dir: &Path) -> Result<Vec<(PathBuf, PathBuf, TestSuite)>> {
    let dir = package_dir.join("testcases");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .with_context(|| format!("could not read {}", dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("yml"))
        .collect();
    entries.sort();

    entries
        .into_iter()
        .map(|yaml_path| {
            let yaml = std::fs::read_to_string(&yaml_path)
                .with_context(|| format!("could not read {}", yaml_path.display()))?;
            let suite = snowchains::parse_test_suite(&yaml)
                .with_context(|| format!("could not read {}", yaml_path.display()))?;
            verify_round_trip(&suite, &yaml_path)?;
            let toml_path = yaml_path.with_extension("toml");
            Ok((yaml_path, toml_path, suite))
        })
        .collect()
}

/// 往復検証その2: 書き出した TOML を読み直して、入出力がバイト単位で一致すること。
fn verify_round_trip(suite: &TestSuite, source: &Path) -> Result<()> {
    let text = suite
        .to_toml()
        .with_context(|| format!("could not turn {} into TOML", source.display()))?;
    let back = TestSuite::parse(&text)
        .with_context(|| format!("could not read back the conversion of {}", source.display()))?;
    if &back != suite {
        bail!(
            "converting {} changed its contents. Stopping without writing anything",
            source.display()
        );
    }
    Ok(())
}

/// `[package.metadata.cargo-compete]` を `[package.metadata.acrust]` に置き換える。
///
/// 他の項目（手で足した依存やコメント）はそのまま残す。
fn rewrite_manifest(package: &PackagePlan) -> Result<()> {
    let path = package.dir.join("Cargo.toml");
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("could not read {}", path.display()))?;
    let mut document: toml_edit::DocumentMut = text
        .parse()
        .with_context(|| format!("{} is not valid TOML", path.display()))?;

    let metadata = document["package"]["metadata"]
        .as_table_mut()
        .context("[package.metadata] is not a table")?;
    metadata.remove("cargo-compete");

    let acrust = metadata
        .entry("acrust")
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .expect("acrust is a table");
    acrust["contest"] = toml_edit::value(package.contest.as_str());

    let tasks = acrust
        .entry("tasks")
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .expect("tasks is a table");
    for (alias, screen_name) in &package.tasks {
        tasks[alias.as_str()] = toml_edit::value(screen_name.as_str());
    }

    std::fs::write(&path, document.to_string())
        .with_context(|| format!("could not write {}", path.display()))
}

/// `.acrust/` 以下を作る。テンプレートは compete.toml から持ち越す。
fn write_settings(root: &Path, compete: &CompeteConfig) -> Result<()> {
    let mut config: toml_edit::DocumentMut = init::DEFAULT_CONFIG
        .parse()
        .context("the built-in config.toml is broken")?;
    if let Some(edition) = &compete.edition {
        config["package"]["edition"] = toml_edit::value(edition.as_str());
    }
    package::write_new_file(&root.join(".acrust/config.toml"), &config.to_string())?;
    package::write_new_file(
        &root.join(".acrust/.gitignore"),
        include_str!("../../assets/acrust-gitignore"),
    )?;
    package::write_new_file(
        &root.join(".acrust/template/main.rs"),
        compete
            .template_src
            .as_deref()
            .unwrap_or(init::DEFAULT_TEMPLATE_MAIN),
    )?;
    package::write_new_file(
        &root.join(".acrust/template/dependencies.toml"),
        compete
            .dependencies
            .as_deref()
            .unwrap_or(init::DEFAULT_DEPENDENCIES),
    )?;
    package::write_new_file(
        &root.join(".acrust/template/copy/.vscode/launch.json"),
        include_str!("../../assets/template-launch.json"),
    )?;

    if let Some(source) = &compete.cargo_lock {
        let source = root.join(source.trim_start_matches("./"));
        if source.is_file() {
            let destination = root.join(".acrust/template/Cargo.lock");
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&source, &destination)
                .with_context(|| format!("could not move {}", source.display()))?;
        }
    }
    Ok(())
}

/// `[package.metadata.cargo-compete.bin]` を持つ `Cargo.toml` を集める。
fn collect_packages(root: &Path) -> Result<Vec<PathBuf>> {
    let mut manifests = Vec::new();
    for entry in std::fs::read_dir(root)
        .with_context(|| format!("could not read {}", root.display()))?
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest = path.join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(&manifest).unwrap_or_default();
        if text.contains("[package.metadata.cargo-compete") {
            manifests.push(manifest);
        }
    }
    manifests.sort();
    Ok(manifests)
}
