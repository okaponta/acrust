//! Storing the session (the `REVEL_SESSION` cookie).
//!
//! It lives at `~/.local/share/acrust/session.json` on both macOS and Linux, and
//! `ACRUST_SESSION_FILE` overrides that. The cookie is the only thing written —
//! never a password — and the file is always 0600: cargo-compete left the
//! equivalent world-readable at 0644.

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const COOKIE_NAME: &str = "REVEL_SESSION";
const ENV_OVERRIDE: &str = "ACRUST_SESSION_FILE";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// The AtCoder session cookie. As good as a password on its own.
    pub revel_session: String,
    /// When it was saved, in UNIX seconds. Only ever displayed.
    #[serde(default)]
    pub saved_at: u64,
    /// The user name confirmed at login. Only ever displayed.
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

/// Where the session file lives, honouring `ACRUST_SESSION_FILE`.
pub fn session_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os(ENV_OVERRIDE) {
        return Ok(PathBuf::from(path));
    }
    Ok(data_dir()?.join("session.json"))
}

/// `~/.local/share/acrust`, on macOS as well: one path is easier to explain, and
/// easier to tell someone to delete.
pub fn data_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join(".local").join("share").join("acrust"))
}

/// `~/.cache/acrust`, where the language id cache lives.
#[allow(dead_code)]
pub fn cache_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join(".cache").join("acrust"))
}

fn home_dir() -> Result<PathBuf> {
    let dirs = directories::BaseDirs::new().context("could not work out your home directory")?;
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
        .with_context(|| format!("could not read {}", path.display()))?;
    let session: Session = serde_json::from_str(&text)
        .with_context(|| format!("{} is corrupted. Run `acrust login` again", path.display()))?;
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
            .with_context(|| format!("could not create {}", parent.display()))?;
        restrict_dir(parent)?;
    }
    let text = serde_json::to_string_pretty(session)? + "\n";
    write_private(path, &text).with_context(|| format!("could not write {}", path.display()))?;
    Ok(())
}

pub fn discard() -> Result<Option<PathBuf>> {
    let path = session_path()?;
    if !path.exists() {
        return Ok(None);
    }
    std::fs::remove_file(&path).with_context(|| format!("could not delete {}", path.display()))?;
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
    // `mode` applies only when the file is created, so an existing file has to
    // be tightened explicitly.
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
            "could not set the permissions of {} to {mode:o}",
            path.display()
        )
    })
}

/// A session file other users can read is a leaked credential, so it is reported
/// and tightened rather than merely complained about.
#[cfg(unix)]
pub fn warn_and_fix_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    let mode = std::fs::metadata(path)
        .with_context(|| format!("could not stat {}", path.display()))?
        .permissions()
        .mode()
        & 0o777;
    if mode & 0o077 != 0 {
        // Telling the user to run `chmod` leaves the window open until they
        // read the message, which may be never.
        crate::ui::warn(&format!(
            "the session file was readable by other users ({mode:o})"
        ));
        crate::ui::warn_detail(&format!(
            "it is as good as a password, so it is now 600: {}",
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

/// For `status`. `None` where the platform has no such thing.
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
        // Put a loosely-permissioned file there first.
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
        // The shape of the path when ACRUST_SESSION_FILE is not set.
        let dir = data_dir().unwrap();
        assert!(dir.ends_with(".local/share/acrust"), "{}", dir.display());
    }
}
