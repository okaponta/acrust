//! セッション（`REVEL_SESSION` クッキー）の保存（決定 D8）。
//!
//! - 保存先は macOS / Linux とも `~/.local/share/acrust/session.json`
//! - `ACRUST_SESSION_FILE` で上書きできる
//! - **パーミッションは 0600 必須**。cargo-compete は 0644 で保存していた（設計 §3.8）
//! - 保存するのはセッションクッキーだけで、パスワードは保存しない

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const COOKIE_NAME: &str = "REVEL_SESSION";
const ENV_OVERRIDE: &str = "ACRUST_SESSION_FILE";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// AtCoder のセッションクッキー。これ自体がパスワード同等の資格情報。
    pub revel_session: String,
    /// 保存時刻（UNIX 秒）。`status` の表示にだけ使う。
    #[serde(default)]
    pub saved_at: u64,
    /// ログイン時に確認できたユーザー名。表示用。
    #[serde(default)]
    pub user_screen_name: String,
}

impl Session {
    pub fn new(revel_session: String, user_screen_name: String) -> Self {
        Self {
            revel_session,
            saved_at: now_unix(),
            user_screen_name,
        }
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// セッションファイルの場所。`ACRUST_SESSION_FILE` があればそれを使う。
pub fn session_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os(ENV_OVERRIDE) {
        return Ok(PathBuf::from(path));
    }
    Ok(data_dir()?.join("session.json"))
}

/// `~/.local/share/acrust`。macOS でも同じパスに統一する（決定 D8）。
pub fn data_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join(".local").join("share").join("acrust"))
}

/// `~/.cache/acrust`。language ID などのキャッシュ置き場。
// language ID のキャッシュ（M4）で使う。
#[allow(dead_code)]
pub fn cache_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join(".cache").join("acrust"))
}

fn home_dir() -> Result<PathBuf> {
    let dirs = directories::BaseDirs::new().context("ホームディレクトリを特定できませんでした")?;
    Ok(dirs.home_dir().to_path_buf())
}

pub fn load() -> Result<Option<Session>> {
    let path = session_path()?;
    load_from(&path)
}

pub fn load_from(path: &Path) -> Result<Option<Session>> {
    if !path.exists() {
        return Ok(None);
    }
    warn_and_fix_permissions(path)?;
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("{} を読めませんでした", path.display()))?;
    let session: Session = serde_json::from_str(&text).with_context(|| {
        format!(
            "{} の内容が壊れています。`acrust login` をやり直してください",
            path.display()
        )
    })?;
    Ok(Some(session))
}

pub fn save(session: &Session) -> Result<PathBuf> {
    let path = session_path()?;
    save_to(&path, session)?;
    Ok(path)
}

pub fn save_to(path: &Path, session: &Session) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("{} を作れませんでした", parent.display()))?;
        restrict_dir(parent)?;
    }
    let text = serde_json::to_string_pretty(session)? + "\n";
    write_private(path, &text).with_context(|| format!("{} に書けませんでした", path.display()))?;
    Ok(())
}

/// セッションを破棄する。消したら true。
pub fn discard() -> Result<Option<PathBuf>> {
    let path = session_path()?;
    if !path.exists() {
        return Ok(None);
    }
    std::fs::remove_file(&path)
        .with_context(|| format!("{} を消せませんでした", path.display()))?;
    Ok(Some(path))
}

#[cfg(unix)]
fn write_private(path: &Path, contents: &str) -> Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    // 既存ファイルを開いた場合 `mode` は無視されるので、明示的に締め直す。
    set_mode(path, 0o600)?;
    Ok(())
}

#[cfg(not(unix))]
fn write_private(path: &Path, contents: &str) -> Result<()> {
    std::fs::write(path, contents)?;
    Ok(())
}

#[cfg(unix)]
fn restrict_dir(dir: &Path) -> Result<()> {
    set_mode(dir, 0o700)
}

#[cfg(not(unix))]
fn restrict_dir(_dir: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).with_context(|| {
        format!(
            "{} のパーミッションを {mode:o} にできませんでした",
            path.display()
        )
    })
}

/// 他ユーザーから読めるセッションファイルは資格情報の漏洩なので、警告して締め直す。
#[cfg(unix)]
pub fn warn_and_fix_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    let mode = std::fs::metadata(path)
        .with_context(|| format!("{} の情報を取得できませんでした", path.display()))?
        .permissions()
        .mode()
        & 0o777;
    if mode & 0o077 != 0 {
        crate::ui::warn(&format!(
            "{} のパーミッションが {mode:04o} でした。セッションクッキーはパスワード同等なので 0600 に直します",
            path.display()
        ));
        set_mode(path, 0o600)?;
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn warn_and_fix_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

/// `status` 表示用。取得できないプラットフォームでは `None`。
#[cfg(unix)]
pub fn mode_of(path: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::metadata(path)
        .ok()
        .map(|m| m.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
pub fn mode_of(_path: &Path) -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("acrust-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn saves_with_mode_0600_even_over_a_world_readable_file() {
        let dir = temp_dir("session");
        let path = dir.join("session.json");
        // 先に緩いパーミッションのファイルを置いておく。
        std::fs::write(&path, "{}").unwrap();
        #[cfg(unix)]
        set_mode(&path, 0o644).unwrap();

        let session = Session::new("dummy-cookie".to_owned(), "okaponta".to_owned());
        save_to(&path, &session).unwrap();

        #[cfg(unix)]
        assert_eq!(mode_of(&path), Some(0o600));

        let loaded = load_from(&path).unwrap().unwrap();
        assert_eq!(loaded.revel_session, "dummy-cookie");
        assert_eq!(loaded.user_screen_name, "okaponta");
        assert!(loaded.saved_at > 0);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_file_is_simply_not_logged_in() {
        let dir = temp_dir("missing");
        assert!(load_from(&dir.join("nope.json")).unwrap().is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_default_path_is_the_same_on_every_unix() {
        // ACRUST_SESSION_FILE が無い状態のパス形をチェックする。
        let dir = data_dir().unwrap();
        assert!(dir.ends_with(".local/share/acrust"), "{}", dir.display());
    }
}
