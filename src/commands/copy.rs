//! `acrust copy` — put a solution on the clipboard.
//!
//! Cloudflare Turnstile guards the submit form of a contest that has ended, so
//! practice submissions cannot go through `submit` (see `commands::submit`).
//! This is the way out: copy, then paste into the browser.
//!
//! What is copied is `src/bin/{alias}.rs` byte for byte. Anything else and the
//! promise that what you submitted is what the repository holds stops being true.

use crate::clipboard;
use crate::config::LoadedConfig;
use crate::ui;
use crate::workspace::{self, Package};
use anyhow::{bail, Context as _, Result};

pub fn run(problem: Option<String>) -> Result<()> {
    let config = LoadedConfig::find()?;
    let package = Package::find()?;
    let resolved = workspace::resolve_problem(
        &package,
        problem.as_deref(),
        config.config.test.resolve,
        std::fs::read_to_string(config.template_src())
            .ok()
            .as_deref(),
    )?;
    // Always name what was copied: a mix-up is invisible until after the paste.
    ui::arrow(&resolved.describe(&package));

    let source = std::fs::read_to_string(&resolved.bin.src_path)
        .with_context(|| format!("could not read {}", resolved.bin.src_path.display()))?;
    if source.trim().is_empty() {
        bail!("{} is empty", resolved.bin.src_path.display());
    }

    let via = clipboard::copy(&source)?;
    ui::ok(&format!(
        "copied ({} lines / {} bytes, via {via})",
        source.lines().count(),
        source.len()
    ));

    let task = package.task_url(&resolved.bin.alias)?;
    ui::info("");
    ui::info("Paste it here to submit:");
    ui::info(&format!("  {task}"));
    ui::info(&format!(
        "  or run `acrust open {}` to open it",
        resolved.bin.alias
    ));
    Ok(())
}
