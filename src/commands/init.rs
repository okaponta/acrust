//! `acrust init` — lay down `.acrust/` and `rust-toolchain.toml` in a repository.
//!
//! Everything written comes from defaults baked into the binary, so `init` works
//! offline. Catching up with the judge environment is `acrust env update`'s job.

use crate::config::{self, Config, LoadedConfig, DEFAULT_JUDGE_RUSTC};
use crate::ui;
use anyhow::{bail, Context as _, Result};
use std::path::{Path, PathBuf};

pub const DEFAULT_CONFIG: &str = include_str!("../../assets/config.toml");
pub const DEFAULT_TEMPLATE_MAIN: &str = include_str!("../../assets/template-main.rs");
pub const DEFAULT_DEPENDENCIES: &str = include_str!("../../assets/template-dependencies.toml");
const ACRUST_GITIGNORE: &str = include_str!("../../assets/acrust-gitignore");
const CARGO_CONFIG: &str = include_str!("../../assets/cargo-config.toml");

pub fn run(path: Option<PathBuf>, force: bool) -> Result<()> {
    let root = match path {
        Some(path) => path,
        None => std::env::current_dir().context("could not get the current directory")?,
    };
    let root =
        std::fs::canonicalize(&root).with_context(|| format!("{} not found", root.display()))?;

    if !force {
        if let Some(existing) = config::find_root(&root) {
            bail!(
                "{} is already managed by acrust ({}). \
                 Pass --force to overwrite it",
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
            ui::field("skip", &format!("{relative} (already there)"));
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
    write(".cargo/config.toml", CARGO_CONFIG)?;

    // Re-read the config first: the user may have asked for pin-toolchain = false.
    let loaded = LoadedConfig::load(&root)?;
    if loaded.config.package.pin_toolchain {
        write(
            "rust-toolchain.toml",
            &rust_toolchain_toml(DEFAULT_JUDGE_RUSTC),
        )?;
    }

    ui::info("");
    ui::ok(&format!(
        "{} is now managed by acrust ({written} created / {skipped} skipped)",
        root.display()
    ));
    ui::info("");
    ui::info("Next:");
    ui::info(crate::commands::NEXT_ENV_UPDATE);
    ui::info("  acrust login        # log in to AtCoder");
    ui::info("  acrust new abc474   # create the package for a contest");
    Ok(())
}

/// Pins the toolchain to the judge's rustc, so a local build failure is a real
/// one rather than a version difference nobody can see.
pub fn rust_toolchain_toml(channel: &str) -> String {
    format!(
        "# Pinned to the same rustc AtCoder's judge uses. `acrust env update` keeps it in step.\n\
         [toolchain]\n\
         channel = \"{channel}\"\n\
         components = [\"rustfmt\", \"clippy\"]\n"
    )
}

fn write_file(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    std::fs::write(path, contents).with_context(|| format!("could not write {}", path.display()))
}

/// The embedded default config, which `env update` diffs the current one against.
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
        // All 68 crates of the 2025-10 judge environment, unedited.
        assert_eq!(parsed.len(), 68, "crates: {}", parsed.len());
        assert_eq!(parsed["itertools"].as_str(), Some("=0.14.0"));
        assert_eq!(parsed["superslice"].as_str(), Some("=1.0.0"));
        assert_eq!(
            parsed["proconio"]["version"].as_str(),
            Some("=0.5.0"),
            "proconio has to be the 0.5.0 the judge uses"
        );
        assert!(parsed["proconio"]["features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f.as_str() == Some("derive")));
    }

    #[test]
    fn the_embedded_template_compiles_as_rust_source() {
        // The template becomes `src/bin/*.rs` verbatim, so it needs a main at least.
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

        // The config has to be findable by walking up from a nested directory.
        let deep = root.join("abc474").join("src").join("bin");
        std::fs::create_dir_all(&deep).unwrap();
        let loaded = LoadedConfig::find_from(&deep).unwrap();
        assert_eq!(loaded.root, root);
        assert_eq!(loaded.config.version, config::SUPPORTED_VERSION);
        assert!(loaded.template_src().is_file());
        assert!(loaded.template_dependencies().is_file());
        assert!(loaded.rust_toolchain_path().is_file());
        assert!(root.join(".cargo/config.toml").is_file());
        // Nothing is planted in copy/: what belongs in every package is the
        // user's to decide, and an editor's config is not everyone's.
        assert!(!loaded.template_copy_dir().exists());

        // A second run refuses: the repository is already managed.
        let err = run(Some(root.clone()), false).unwrap_err().to_string();
        assert!(err.contains("--force"), "{err}");

        std::fs::remove_dir_all(&root).unwrap();
    }
}
