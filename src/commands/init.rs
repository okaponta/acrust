//! `acrust init` — リポジトリに `.acrust/` と `rust-toolchain.toml` を用意する。
//!
//! 生成物はすべてバイナリに埋め込んだ既定値から作る（ネットワーク不要）。
//! ジャッジ環境への追従は `acrust env update` の仕事。

use crate::config::{self, Config, LoadedConfig, DEFAULT_JUDGE_RUSTC};
use crate::ui;
use anyhow::{bail, Context as _, Result};
use std::path::{Path, PathBuf};

pub const DEFAULT_CONFIG: &str = include_str!("../../assets/config.toml");
pub const DEFAULT_TEMPLATE_MAIN: &str = include_str!("../../assets/template-main.rs");
pub const DEFAULT_DEPENDENCIES: &str = include_str!("../../assets/template-dependencies.toml");
const DEFAULT_LAUNCH_JSON: &str = include_str!("../../assets/template-launch.json");
const ACRUST_GITIGNORE: &str = include_str!("../../assets/acrust-gitignore");
const CARGO_CONFIG: &str = include_str!("../../assets/cargo-config.toml");

pub fn run(path: Option<PathBuf>, force: bool) -> Result<()> {
    let root = match path {
        Some(path) => path,
        None => std::env::current_dir().context("カレントディレクトリを取得できませんでした")?,
    };
    let root = std::fs::canonicalize(&root)
        .with_context(|| format!("{} が見つかりません", root.display()))?;

    if !force {
        if let Some(existing) = config::find_root(&root) {
            bail!(
                "{} は既に acrust の管理下です（{}）。\
                 上書きするなら --force を付けてください",
                root.display(),
                config::config_path(&existing).display()
            );
        }
    }

    let mut written = 0;
    let mut skipped = 0;
    let mut write = |relative: &str, contents: &str| -> Result<()> {
        let path = root.join(relative);
        if path.exists() && !force {
            ui::field("skip", &format!("{relative}（既にあります）"));
            skipped += 1;
            return Ok(());
        }
        write_file(&path, contents)?;
        ui::field("create", relative);
        written += 1;
        Ok(())
    };

    write(".acrust/config.toml", DEFAULT_CONFIG)?;
    write(".acrust/.gitignore", ACRUST_GITIGNORE)?;
    write(".acrust/template/main.rs", DEFAULT_TEMPLATE_MAIN)?;
    write(".acrust/template/dependencies.toml", DEFAULT_DEPENDENCIES)?;
    write(
        ".acrust/template/copy/.vscode/launch.json",
        DEFAULT_LAUNCH_JSON,
    )?;
    write(".cargo/config.toml", CARGO_CONFIG)?;

    // config を読み直してから rust-toolchain.toml を判断する（pin-toolchain = false を尊重するため）。
    let loaded = LoadedConfig::load(&root)?;
    if loaded.config.package.pin_toolchain {
        write(
            "rust-toolchain.toml",
            &rust_toolchain_toml(DEFAULT_JUDGE_RUSTC),
        )?;
    }

    ui::info("");
    ui::ok(&format!(
        "{} を acrust の管理下にしました（作成 {written} / スキップ {skipped}）",
        root.display()
    ));
    ui::info("");
    ui::info("次にやること:");
    ui::info("  acrust env update   # ジャッジ環境の依存・Cargo.lock・rustc を取得して固定する");
    ui::info("  acrust login        # AtCoder にログインする");
    ui::info("  acrust new abc474   # コンテストのパッケージを作る");
    Ok(())
}

/// ジャッジと同じ rustc に固定する（決定 D11）。
pub fn rust_toolchain_toml(channel: &str) -> String {
    format!(
        "# AtCoder のジャッジと同じ rustc に固定する。`acrust env update` が追従させる。\n\
         [toolchain]\n\
         channel = \"{channel}\"\n\
         components = [\"rustfmt\", \"clippy\"]\n"
    )
}

fn write_file(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("{} を作れませんでした", parent.display()))?;
    }
    std::fs::write(path, contents).with_context(|| format!("{} に書けませんでした", path.display()))
}

/// 埋め込んだ既定の設定。`env update`（M5）が現行値との diff に使う。
#[allow(dead_code)]
pub fn default_config() -> Result<Config> {
    Config::parse(DEFAULT_CONFIG)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_dependencies_are_the_judge_environment() {
        let parsed: toml::Table = toml::from_str(DEFAULT_DEPENDENCIES).unwrap();
        // ジャッジ環境（2025-10）の 68 クレートがそのまま入っていること。
        assert_eq!(parsed.len(), 68, "crates: {}", parsed.len());
        assert_eq!(parsed["itertools"].as_str(), Some("=0.14.0"));
        assert_eq!(parsed["superslice"].as_str(), Some("=1.0.0"));
        assert_eq!(
            parsed["proconio"]["version"].as_str(),
            Some("=0.5.0"),
            "proconio はジャッジと同じ 0.5.0 でなければならない"
        );
        assert!(parsed["proconio"]["features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f.as_str() == Some("derive")));
    }

    #[test]
    fn the_embedded_template_compiles_as_rust_source() {
        // テンプレートは丸ごと `src/bin/*.rs` になるので、少なくとも main が要る。
        assert!(DEFAULT_TEMPLATE_MAIN.contains("fn main()"));
        assert!(DEFAULT_TEMPLATE_MAIN.contains("proconio"));
    }

    #[test]
    fn the_toolchain_file_pins_the_judge_rustc() {
        let toml_text = rust_toolchain_toml(DEFAULT_JUDGE_RUSTC);
        let parsed: toml::Table = toml::from_str(&toml_text).unwrap();
        assert_eq!(parsed["toolchain"]["channel"].as_str(), Some("1.89.0"));
    }

    #[test]
    fn init_creates_a_readable_repository() {
        let root = std::env::temp_dir().join(format!("acrust-init-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        run(Some(root.clone()), false).unwrap();

        // 深いところから上に辿って設定を見つけられること。
        let deep = root.join("abc474").join("src").join("bin");
        std::fs::create_dir_all(&deep).unwrap();
        let loaded = LoadedConfig::find_from(&deep).unwrap();
        assert_eq!(loaded.root, root);
        assert_eq!(loaded.config.version, config::SUPPORTED_VERSION);
        assert!(loaded.template_src().is_file());
        assert!(loaded.template_dependencies().is_file());
        assert!(loaded.rust_toolchain_path().is_file());
        assert!(root.join(".cargo/config.toml").is_file());
        assert!(root
            .join(".acrust/template/copy/.vscode/launch.json")
            .is_file());

        // 2 回目は既に管理下なので断る。
        let err = run(Some(root.clone()), false).unwrap_err().to_string();
        assert!(err.contains("--force"), "{err}");

        std::fs::remove_dir_all(&root).unwrap();
    }
}
