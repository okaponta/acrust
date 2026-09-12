//! `.acrust/config.toml` の読み込み。
//!
//! 設定はリポジトリのルート（`.acrust/config.toml` を持つ最も近い祖先ディレクトリ）に置く。
//! `acrust status` は読み込んだ config の絶対パスを必ず表示する。

// 設定項目とパス解決は M2〜M5 のコマンドが読む。M0/M1 では未参照のものがある。
#![allow(dead_code)]

use anyhow::{bail, Context as _, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// `.acrust/` 直下に置くもの。
pub const CONFIG_DIR: &str = ".acrust";
pub const CONFIG_FILE: &str = "config.toml";

/// このバイナリが理解できる `config.toml` の最大バージョン。
pub const SUPPORTED_VERSION: u32 = 1;

/// AtCoder のジャッジ環境（2025-10）の rustc。`acrust env update` が追従させる。
pub const DEFAULT_JUDGE_RUSTC: &str = "1.89.0";
/// 同上の edition。
pub const DEFAULT_JUDGE_EDITION: &str = "2024";
/// 同上の言語一覧ページ。
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
    /// コンテストパッケージの配置先。`{contest}` が展開される。
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
    /// 生成する `Cargo.toml` の edition。`env update` がジャッジ環境に追従させる。
    #[serde(default = "default_edition")]
    pub edition: String,
    /// `rust-toolchain.toml` を生成・追従させるか。
    #[serde(default = "default_true")]
    pub pin_toolchain: bool,
    /// 生成する `Cargo.toml` に差し込む `[profile.*]`（生の TOML）。
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
    /// `{package}`（リポジトリルートからのパッケージ相対パス）と `{problem}`（alias）が展開される。
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

/// 引数省略時の問題推定（決定 D7）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResolveMode {
    /// `src/bin/*.rs` のうち mtime が最新のものを使う。
    Mtime,
    /// bin が 1 つのときだけ推定する。
    Single,
    /// 推定しない。
    Never,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TestConfig {
    #[serde(default = "default_test_profile")]
    pub profile: Profile,
    /// 0 = 論理コア数。
    #[serde(default)]
    pub jobs: usize,
    /// 問題の TL に掛ける倍率。
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
    /// 空なら提出ページの `<select>` から自動判定する（推奨）。
    #[serde(default)]
    pub language_id: String,
    /// 自動判定に使う正規表現。
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
    /// AtCoder の言語アップデート情報のページ。`env update` の入口。
    #[serde(default = "default_language_list")]
    pub language_list: String,
    /// リクエスト間隔の下限。AtCoder は実際に 429 を返してくる。
    #[serde(default = "default_request_interval")]
    pub request_interval_ms: u64,
    /// 429 / 5xx のリトライ回数。
    #[serde(default = "default_retry")]
    pub retry: u32,
    /// `{version}` がこのバイナリのバージョンに展開される。
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
    /// `{version}` を展開した User-Agent。素性を明示するために必ず付ける。
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

/// 読み込み済みの設定と、その出どころ。
#[derive(Debug, Clone)]
pub struct LoadedConfig {
    /// `.acrust/` を持つディレクトリ（＝リポジトリのルート）。
    pub root: PathBuf,
    /// 読み込んだ `config.toml` の絶対パス。
    pub path: PathBuf,
    pub config: Config,
}

impl LoadedConfig {
    /// `start` から上に辿って `.acrust/config.toml` を探し、読み込む。
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

    /// 現在のディレクトリから探す。
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

    /// リポジトリルートからの相対パスを絶対パスにする。
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

    /// コンテスト `contest` のパッケージディレクトリ。
    pub fn contest_dir(&self, contest: &str) -> PathBuf {
        self.root.join(normalize_relative(
            &self.config.contest.path.replace("{contest}", contest),
        ))
    }

    /// パッケージ `package_rel`（ルートからの相対パス）の問題 `problem` のテストケースファイル。
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

/// `./foo` の先頭 `./` を落とす。`Path::join` は `./` があっても動くが、表示が汚くなるため。
fn normalize_relative(s: &str) -> String {
    s.strip_prefix("./").unwrap_or(s).to_owned()
}

pub fn config_path(root: &Path) -> PathBuf {
    root.join(CONFIG_DIR).join(CONFIG_FILE)
}

/// `start` から上に辿って `.acrust/config.toml` を持つディレクトリを探す。
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
