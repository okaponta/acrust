//! Copying to the clipboard, for `acrust copy`.
//!
//! An external command rather than a crate like `arboard`: on Linux those pull in
//! X11 / Wayland development packages, which is a lot to ask of anyone installing
//! a CLI for AtCoder. Same trade as `browser`.

use anyhow::{bail, Context as _, Result};
use std::io::Write as _;
use std::process::{Command, Stdio};

/// Tried in order; the first one that runs wins.
fn candidates() -> &'static [(&'static str, &'static [&'static str])] {
    if cfg!(target_os = "macos") {
        &[("pbcopy", &[])]
    } else if cfg!(target_os = "windows") {
        &[("clip", &[])]
    } else {
        // Wayland before X11; a given desktop usually has only one of them.
        &[
            ("wl-copy", &[]),
            ("xclip", &["-selection", "clipboard"]),
            ("xsel", &["--clipboard", "--input"]),
        ]
    }
}

/// Puts `text` on the clipboard, returning the command that did it.
pub fn copy(text: &str) -> Result<&'static str> {
    for (command, args) in candidates() {
        if run(command, args, text)? {
            return Ok(command);
        }
    }
    let tried: Vec<&str> = candidates().iter().map(|(command, _)| *command).collect();
    bail!(
        "could not copy to the clipboard (none of {} were found)",
        tried.join(" / ")
    )
}

/// `Ok(false)` when the command is not installed; an error when it is and failed.
fn run(command: &str, args: &[&str], text: &str) -> Result<bool> {
    let mut child = match Command::new(command)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        // Not installed is not a failure; try the next candidate.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e).with_context(|| format!("could not start {command}")),
    };

    {
        let mut stdin = child.stdin.take().context("could not take stdin")?;
        stdin
            .write_all(text.as_bytes())
            .with_context(|| format!("could not write to {command}"))?;
        // Dropped here so the child sees EOF; without it, it never stops reading.
    }

    let status = child
        .wait()
        .with_context(|| format!("could not wait for {command} to finish"))?;
    if !status.success() {
        bail!("{command} exited with {status}");
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_platform_has_at_least_one_way_to_copy() {
        assert!(!candidates().is_empty());
    }

    /// A command that is not installed moves on to the next, it does not fail.
    #[test]
    fn a_missing_command_is_not_an_error() {
        assert!(!run("acrust-no-such-clipboard-command", &[], "x").unwrap());
    }

    /// A real round trip through the system clipboard.
    ///
    /// Behind `live` because it overwrites whatever the user had copied, and
    /// losing that on every `cargo test` would be its own small disaster.
    #[cfg(all(target_os = "macos", feature = "live"))]
    #[test]
    fn macos_copies_through_pbcopy() {
        let text = "fn main() { println!(\"テスト\"); }\n";
        assert_eq!(copy(text).unwrap(), "pbcopy");
        let pasted = Command::new("pbpaste").output().unwrap().stdout;
        assert_eq!(String::from_utf8(pasted).unwrap(), text);
    }
}
