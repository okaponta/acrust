//! ブラウザで URL を開く。`open` と `login` が使う。

use anyhow::{bail, Context as _, Result};

/// OS 標準のハンドラで `url` を開く。
pub fn open(url: &str) -> Result<()> {
    let command = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "start"
    } else {
        "xdg-open"
    };
    let status = std::process::Command::new(command)
        .arg(url)
        .status()
        .with_context(|| format!("could not start {command}"))?;
    if !status.success() {
        bail!("{command} exited with {status}");
    }
    Ok(())
}
