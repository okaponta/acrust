//! Reading `.acrust/config.toml`.
//!
//! The config lives at the root of the repository — the nearest ancestor holding
//! `.acrust/config.toml` — and `acrust status` always prints the absolute path of
//! the one it read, so there is never a question of which file is in effect.

// Not every field is read by every command.
#![allow(dead_code)]

use anyhow::{bail, Context as _, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// What lives directly under `.acrust/`.
pub const CONFIG_DIR: &str = ".acrust";
pub const CONFIG_FILE: &str = "config.toml";

/// The newest `config.toml` this binary understands.
pub const SUPPORTED_VERSION: u32 = 1;

/// The rustc of AtCoder's 2025-10 judge environment. `acrust env update` moves
/// these three on when AtCoder does.
pub const DEFAULT_JUDGE_RUSTC: &str = "1.89.0";
pub const DEFAULT_JUDGE_EDITION: &str = "2024";
pub const DEFAULT_LANGUAGE_LIST: &str =
    "https://img.atcoder.jp/file/language-update/2025-10/language-list.html";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub contest: ContestConfig,
    #[serde(default)]
    pub package: PackageConfig,
    #[serde(default)]
    pub template: TemplateConfig,
    #[serde(default)]
    pub testcases: TestcasesConfig,
    #[serde(default)]
    pub test: TestConfig,
    #[serde(default)]
    pub submit: SubmitConfig,
    #[serde(default)]
    pub atcoder: AtcoderConfig,
}

fn default_version() -> u32 {
    SUPPORTED_VERSION
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ContestConfig {
    /// Where contest packages go. `{contest}` is substituted.
    #[serde(default = "default_contest_path")]
    pub path: String,
}

fn default_contest_path() -> String {
    "./{contest}".to_owned()
}

impl Default for ContestConfig {
    fn default() -> Self {
        Self {
            path: default_contest_path(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PackageConfig {
    /// The edition of a generated `Cargo.toml`, kept in step by `env update`.
    #[serde(default = "default_edition")]
    pub edition: String,
    /// Whether to write and maintain `rust-toolchain.toml`.
    #[serde(default = "default_true")]
    pub pin_toolchain: bool,
    /// Raw TOML spliced in as `[profile.*]`.
    #[serde(default = "default_profile")]
    pub profile: String,
}

fn default_edition() -> String {
    DEFAULT_JUDGE_EDITION.to_owned()
}

fn default_true() -> bool {
    true
}

fn default_profile() -> String {
    "[dev]\nopt-level = 3\ndebug-assertions = true\noverflow-checks = true\n".to_owned()
}

impl Default for PackageConfig {
    fn default() -> Self {
        Self {
            edition: default_edition(),
            pin_toolchain: default_true(),
            profile: default_profile(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TemplateConfig {
    #[serde(default = "default_template_src")]
    pub src: String,
    #[serde(default = "default_template_deps")]
    pub dependencies: String,
    #[serde(default = "default_template_lock")]
    pub cargo_lock: String,
    #[serde(default = "default_template_copy")]
    pub copy_dir: String,
}

fn default_template_src() -> String {
    ".acrust/template/main.rs".to_owned()
}
fn default_template_deps() -> String {
    ".acrust/template/dependencies.toml".to_owned()
}
fn default_template_lock() -> String {
    ".acrust/template/Cargo.lock".to_owned()
}
fn default_template_copy() -> String {
    ".acrust/template/copy".to_owned()
}

impl Default for TemplateConfig {
    fn default() -> Self {
        Self {
            src: default_template_src(),
            dependencies: default_template_deps(),
            cargo_lock: default_template_lock(),
            copy_dir: default_template_copy(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TestcasesConfig {
    /// `{package}` (the package's path from the root) and `{problem}` (the alias)
    /// are substituted.
    #[serde(default = "default_testcases_path")]
    pub path: String,
}

fn default_testcases_path() -> String {
    "{package}/testcases/{problem}.toml".to_owned()
}

impl Default for TestcasesConfig {
    fn default() -> Self {
        Self {
            path: default_testcases_path(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Profile {
    Dev,
    Release,
}

/// How to guess the problem when the argument is left off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResolveMode {
    /// The most recently modified `src/bin/*.rs`.
    Mtime,
    /// Only when there is exactly one bin.
    Single,
    /// Never guess.
    Never,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TestConfig {
    #[serde(default = "default_test_profile")]
    pub profile: Profile,
    /// 0 means one per logical core.
    #[serde(default)]
    pub jobs: usize,
    /// Multiplier on the problem's time limit.
    #[serde(default = "default_timeout_margin")]
    pub timeout_margin: f64,
    #[serde(default = "default_resolve")]
    pub resolve: ResolveMode,
}

fn default_test_profile() -> Profile {
    Profile::Dev
}
fn default_timeout_margin() -> f64 {
    1.5
}
fn default_resolve() -> ResolveMode {
    ResolveMode::Mtime
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            profile: default_test_profile(),
            jobs: 0,
            timeout_margin: default_timeout_margin(),
            resolve: default_resolve(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SubmitConfig {
    /// Empty reads the id off the submit page, which is the recommendation.
    #[serde(default)]
    pub language_id: String,
    /// The regex that picks the language.
    #[serde(default = "default_language_pattern")]
    pub language_pattern: String,
    #[serde(default = "default_true")]
    pub test_before_submit: bool,
    #[serde(default = "default_true")]
    pub watch: bool,
    #[serde(default = "default_watch_interval")]
    pub watch_interval_ms: u64,
    #[serde(default = "default_watch_timeout")]
    pub watch_timeout_s: u64,
}

fn default_language_pattern() -> String {
    r"^Rust \(rustc".to_owned()
}
fn default_watch_interval() -> u64 {
    2000
}
fn default_watch_timeout() -> u64 {
    60
}

impl Default for SubmitConfig {
    fn default() -> Self {
        Self {
            language_id: String::new(),
            language_pattern: default_language_pattern(),
            test_before_submit: true,
            watch: true,
            watch_interval_ms: default_watch_interval(),
            watch_timeout_s: default_watch_timeout(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AtcoderConfig {
    /// AtCoder's language update page, where `env update` starts.
    #[serde(default = "default_language_list")]
    pub language_list: String,
    /// Floor on the gap between requests. AtCoder does return 429.
    #[serde(default = "default_request_interval")]
    pub request_interval_ms: u64,
    /// How many times to retry a 429 or 5xx.
    #[serde(default = "default_retry")]
    pub retry: u32,
    /// `{version}` becomes this binary's version.
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
}

fn default_language_list() -> String {
    DEFAULT_LANGUAGE_LIST.to_owned()
}
fn default_request_interval() -> u64 {
    1000
}
fn default_retry() -> u32 {
    3
}
fn default_user_agent() -> String {
    "acrust/{version} (+https://github.com/okaponta/acrust)".to_owned()
}

impl Default for AtcoderConfig {
    fn default() -> Self {
        Self {
            language_list: default_language_list(),
            request_interval_ms: default_request_interval(),
            retry: default_retry(),
            user_agent: default_user_agent(),
        }
    }
}

impl AtcoderConfig {
    /// The User-Agent actually sent. acrust always says what it is.
    pub fn resolved_user_agent(&self) -> String {
        self.user_agent
            .replace("{version}", env!("CARGO_PKG_VERSION"))
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: SUPPORTED_VERSION,
            contest: ContestConfig::default(),
            package: PackageConfig::default(),
            template: TemplateConfig::default(),
            testcases: TestcasesConfig::default(),
            test: TestConfig::default(),
            submit: SubmitConfig::default(),
            atcoder: AtcoderConfig::default(),
        }
    }
}

impl Config {
    pub fn parse(s: &str) -> Result<Self> {
        let config: Config = toml::from_str(s).context("could not parse config.toml")?;
        if config.version > SUPPORTED_VERSION {
            bail!(
                "config.toml has version = {}, which is newer than the version = {} this acrust ({}) understands. \
                 Update acrust",
                config.version,
                env!("CARGO_PKG_VERSION"),
                SUPPORTED_VERSION
            );
        }
        Ok(config)
    }
}

/// A loaded config, and where it came from.
#[derive(Debug, Clone)]
pub struct LoadedConfig {
    /// The directory holding `.acrust/`, i.e. the root of the repository.
    pub root: PathBuf,
    /// The absolute path of the `config.toml` that was read.
    pub path: PathBuf,
    pub config: Config,
}

impl LoadedConfig {
    /// Walks up from `start` looking for `.acrust/config.toml`.
    pub fn find_from(start: &Path) -> Result<Self> {
        let root = find_root(start).with_context(|| {
            format!(
                "{}/{} not found (looked upwards from {}). \
                 Run `acrust init` at the root of your repository",
                CONFIG_DIR,
                CONFIG_FILE,
                start.display()
            )
        })?;
        Self::load(&root)
    }

    pub fn find() -> Result<Self> {
        let cwd = std::env::current_dir().context("could not get the current directory")?;
        Self::find_from(&cwd)
    }

    pub fn load(root: &Path) -> Result<Self> {
        let path = config_path(root);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("could not read {}", path.display()))?;
        let config =
            Config::parse(&text).with_context(|| format!("could not load {}", path.display()))?;
        Ok(Self {
            root: root.to_path_buf(),
            path,
            config,
        })
    }

    /// Makes a path relative to the repository root absolute.
    pub fn resolve_path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    pub fn template_src(&self) -> PathBuf {
        self.resolve_path(&self.config.template.src)
    }

    pub fn template_dependencies(&self) -> PathBuf {
        self.resolve_path(&self.config.template.dependencies)
    }

    pub fn template_cargo_lock(&self) -> PathBuf {
        self.resolve_path(&self.config.template.cargo_lock)
    }

    pub fn template_copy_dir(&self) -> PathBuf {
        self.resolve_path(&self.config.template.copy_dir)
    }

    pub fn rust_toolchain_path(&self) -> PathBuf {
        self.root.join("rust-toolchain.toml")
    }

    /// The package directory for a contest.
    pub fn contest_dir(&self, contest: &str) -> PathBuf {
        self.root.join(normalize_relative(
            &self.config.contest.path.replace("{contest}", contest),
        ))
    }

    /// The test-case file for one problem of one package.
    pub fn testcases_path(&self, package_rel: &str, problem: &str) -> PathBuf {
        self.root.join(normalize_relative(
            &self
                .config
                .testcases
                .path
                .replace("{package}", package_rel)
                .replace("{problem}", problem),
        ))
    }
}

/// Strips a leading `./`. `Path::join` copes with it; the paths acrust prints
/// look better without it.
fn normalize_relative(s: &str) -> String {
    s.strip_prefix("./").unwrap_or(s).to_owned()
}

pub fn config_path(root: &Path) -> PathBuf {
    root.join(CONFIG_DIR).join(CONFIG_FILE)
}

/// The nearest ancestor of `start` holding `.acrust/config.toml`.
pub fn find_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| config_path(dir).is_file())
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_documented_ones() {
        let config = Config::parse("version = 1").unwrap();
        assert_eq!(config.contest.path, "./{contest}");
        assert_eq!(config.package.edition, "2024");
        assert!(config.package.pin_toolchain);
        assert_eq!(config.test.resolve, ResolveMode::Mtime);
        assert_eq!(config.test.profile, Profile::Dev);
        assert!(config.submit.watch);
        assert!(config.submit.language_id.is_empty());
        assert_eq!(config.atcoder.request_interval_ms, 1000);
    }

    #[test]
    fn parses_the_shipped_default_config() {
        let config = Config::parse(crate::commands::init::DEFAULT_CONFIG).unwrap();
        assert_eq!(config.version, SUPPORTED_VERSION);
        assert_eq!(config.package.edition, DEFAULT_JUDGE_EDITION);
        assert_eq!(config.atcoder.language_list, DEFAULT_LANGUAGE_LIST);
        assert_eq!(config.test.timeout_margin, 1.5);
    }

    #[test]
    fn rejects_a_newer_config_version() {
        let err = Config::parse("version = 99").unwrap_err().to_string();
        assert!(err.contains("Update acrust"), "{err}");
    }

    #[test]
    fn user_agent_carries_the_version() {
        let ua = AtcoderConfig::default().resolved_user_agent();
        assert!(
            ua.starts_with(&format!("acrust/{}", env!("CARGO_PKG_VERSION"))),
            "{ua}"
        );
        assert!(!ua.contains("{version}"), "{ua}");
    }

    #[test]
    fn paths_expand_the_placeholders() {
        let loaded = LoadedConfig {
            root: PathBuf::from("/repo"),
            path: PathBuf::from("/repo/.acrust/config.toml"),
            config: Config::default(),
        };
        assert_eq!(loaded.contest_dir("abc474"), PathBuf::from("/repo/abc474"));
        assert_eq!(
            loaded.testcases_path("abc474", "c"),
            PathBuf::from("/repo/abc474/testcases/c.toml")
        );
        assert_eq!(
            loaded.template_src(),
            PathBuf::from("/repo/.acrust/template/main.rs")
        );
    }
}
