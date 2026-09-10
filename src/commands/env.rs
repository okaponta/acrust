//! `acrust env update`（設計 §3.5）。
//!
//! AtCoder の言語アップデート情報から、依存クレート・`Cargo.lock`・`edition`・
//! rustc のバージョンを丸ごと取り直す。書き換える前に必ず差分を見せる（決定 D6）。

use crate::atcoder::env::{self, JudgeEnvironment};
use crate::atcoder::AtCoderClient;
use crate::config::LoadedConfig;
use crate::ui;
use anyhow::{Context as _, Result};
use std::io::Write as _;
use std::path::Path;

pub fn update(language_list: Option<String>, yes: bool) -> Result<()> {
    let config = LoadedConfig::find()?;
    let list_url = language_list
        .clone()
        .unwrap_or_else(|| config.config.atcoder.language_list.clone());
    let pattern = config.config.submit.language_pattern.clone();

    let client = AtCoderClient::new(&config.config.atcoder)?;
    ui::arrow(&format!("言語一覧: {list_url}"));
    let list = client.get(&list_url)?;
    list.error_for_status()?;

    let script_url = env::find_install_script(&list.body, &pattern)?;
    ui::arrow(&format!("インストールスクリプト: {script_url}"));
    let script = client.get(&script_url)?;
    script.error_for_status()?;
    let environment = env::parse_install_script(&script.body)?;

    let lock = match &environment.cargo_lock_url {
        Some(url) => {
            ui::arrow(&format!("Cargo.lock: {url}"));
            let response = client.get(url)?;
            response.error_for_status()?;
            Some(response.body)
        }
        None => {
            ui::warn("インストールスクリプトに Cargo.lock の取得元がありません");
            None
        }
    };

    let plan = Plan::new(
        &config,
        &environment,
        lock.as_deref(),
        &list_url,
        language_list,
    )?;
    if plan.is_empty() {
        ui::ok(&format!("すでに最新です（{}）", environment.display));
        return Ok(());
    }

    plan.show(&environment);
    if !yes && !confirm()? {
        ui::info("何も書き換えませんでした");
        return Ok(());
    }
    let dependencies_changed = !plan.dependencies.is_empty();
    plan.apply(&config, &environment, lock.as_deref())?;
    ui::ok(&format!("{} に合わせました", environment.display));
    if dependencies_changed {
        ui::info("");
        ui::info("依存が変わったので、次のビルドは一度だけ時間がかかります（実測 30 秒ほど）");
    }
    Ok(())
}

/// 何を書き換えることになるか。
struct Plan {
    dependencies: env::DependencyDiff,
    edition: Option<(String, String)>,
    toolchain: Option<(String, String)>,
    lock_changes: bool,
    language_list: Option<String>,
}

impl Plan {
    fn new(
        config: &LoadedConfig,
        environment: &JudgeEnvironment,
        lock: Option<&str>,
        list_url: &str,
        requested_list: Option<String>,
    ) -> Result<Self> {
        let current = std::fs::read_to_string(config.template_dependencies())
            .ok()
            .and_then(|text| toml::from_str::<toml::Table>(&text).ok())
            .unwrap_or_default();
        let dependencies = env::diff_dependencies(&current, &environment.dependency_table()?);

        let edition = (config.config.package.edition != environment.edition).then(|| {
            (
                config.config.package.edition.clone(),
                environment.edition.clone(),
            )
        });

        let pinned = current_toolchain(&config.rust_toolchain_path());
        let toolchain = (config.config.package.pin_toolchain
            && pinned.as_deref() != Some(environment.rustc.as_str()))
        .then(|| {
            (
                pinned.unwrap_or_else(|| "(固定なし)".to_owned()),
                environment.rustc.clone(),
            )
        });

        let lock_changes = match lock {
            Some(lock) => {
                std::fs::read_to_string(config.template_cargo_lock())
                    .ok()
                    .as_deref()
                    != Some(lock)
            }
            None => false,
        };

        let language_list =
            requested_list.filter(|_| config.config.atcoder.language_list != list_url);

        Ok(Self {
            dependencies,
            edition,
            toolchain,
            lock_changes,
            language_list,
        })
    }

    fn is_empty(&self) -> bool {
        self.dependencies.is_empty()
            && self.edition.is_none()
            && self.toolchain.is_none()
            && !self.lock_changes
            && self.language_list.is_none()
    }

    fn show(&self, environment: &JudgeEnvironment) {
        ui::info("");
        ui::section(&format!("{} との差分", environment.display));
        if let Some((before, after)) = &self.toolchain {
            ui::field("rustc", &format!("{before} → {after}"));
        }
        if let Some((before, after)) = &self.edition {
            ui::field("edition", &format!("{before} → {after}"));
        }
        if self.lock_changes {
            ui::field("Cargo.lock", "更新あり");
        }
        if let Some(url) = &self.language_list {
            ui::field("language-list", url);
        }

        let diff = &self.dependencies;
        if diff.is_empty() {
            ui::field("依存クレート", "変更なし");
            return;
        }
        ui::field(
            "依存クレート",
            &format!(
                "追加 {} / 削除 {} / 変更 {}",
                diff.added.len(),
                diff.removed.len(),
                diff.changed.len()
            ),
        );
        for added in &diff.added {
            ui::info(&format!("    + {added}"));
        }
        for removed in &diff.removed {
            ui::info(&format!("    - {removed}"));
        }
        for (name, before, after) in &diff.changed {
            ui::info(&format!("    ~ {name} {before} → {after}"));
        }
    }

    fn apply(
        &self,
        config: &LoadedConfig,
        environment: &JudgeEnvironment,
        lock: Option<&str>,
    ) -> Result<()> {
        if !self.dependencies.is_empty() {
            let path = config.template_dependencies();
            write(&path, &render_dependencies(environment, config))?;
            ui::field("更新", &display(config, &path));
        }
        if self.lock_changes {
            if let Some(lock) = lock {
                let path = config.template_cargo_lock();
                write(&path, lock)?;
                ui::field("更新", &display(config, &path));
            }
        }
        if self.toolchain.is_some() {
            let path = config.rust_toolchain_path();
            write(
                &path,
                &crate::commands::init::rust_toolchain_toml(&environment.rustc),
            )?;
            ui::field("更新", &display(config, &path));
        }
        if self.edition.is_some() || self.language_list.is_some() {
            update_config(config, &environment.edition, self.language_list.as_deref())?;
            ui::field("更新", &display(config, &config.path));
        }
        Ok(())
    }
}

/// 出どころが分かるヘッダを付けて `[dependencies]` を書く。
fn render_dependencies(environment: &JudgeEnvironment, config: &LoadedConfig) -> String {
    format!(
        "# AtCoder のジャッジ環境と完全に一致する [dependencies]（決定 D6）。\n\
         #\n\
         # このファイルは `acrust env update` が自動生成する。\n\
         # 手で削っても構わない（env update は上書き前に diff を見せる）。\n\
         #\n\
         # 環境: {}\n\
         # 出典: {}\n\n{}",
        environment.display,
        config.config.atcoder.language_list,
        // テンプレートは [dependencies] の「中身」。見出しは Cargo.toml を組み立てる側が書く。
        crate::package::strip_dependencies_header(&environment.dependencies)
    )
}

/// 設定は `toml_edit` で書き換える。コメントも書式も保つ（決定 D3）。
fn update_config(config: &LoadedConfig, edition: &str, language_list: Option<&str>) -> Result<()> {
    let text = std::fs::read_to_string(&config.path)
        .with_context(|| format!("{} を読めませんでした", config.path.display()))?;
    let mut document: toml_edit::DocumentMut = text
        .parse()
        .with_context(|| format!("{} が TOML として読めません", config.path.display()))?;

    document["package"]["edition"] = toml_edit::value(edition);
    if let Some(url) = language_list {
        document["atcoder"]["language-list"] = toml_edit::value(url);
    }
    std::fs::write(&config.path, document.to_string())
        .with_context(|| format!("{} に書けませんでした", config.path.display()))
}

fn current_toolchain(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let table: toml::Table = toml::from_str(&text).ok()?;
    Some(table.get("toolchain")?.get("channel")?.as_str()?.to_owned())
}

fn write(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("{} を作れませんでした", parent.display()))?;
    }
    std::fs::write(path, contents).with_context(|| format!("{} に書けませんでした", path.display()))
}

fn display(config: &LoadedConfig, path: &Path) -> String {
    crate::package::relative(&config.root, path)
}

fn confirm() -> Result<bool> {
    use std::io::IsTerminal as _;

    if !std::io::stdin().is_terminal() {
        anyhow::bail!(
            "書き換えてよいか確認できません（対話端末ではありません）。--yes を付けてください"
        );
    }
    ui::info("");
    print!("この内容で書き換えますか？ [y/N]: ");
    std::io::stdout().flush().ok();
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .context("標準入力を読めませんでした")?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "Yes"))
}
