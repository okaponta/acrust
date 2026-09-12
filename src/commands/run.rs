//! `acrust run` — run a solution with stdin passed straight through.
//!
//! Not for testing; for typing input in by hand and watching what happens.

use crate::config::LoadedConfig;
use crate::runner;
use crate::ui;
use crate::workspace::{self, Origin, Package};
use anyhow::{Context as _, Result};
use std::process::{Command, ExitCode, Stdio};

pub fn run(problem: Option<String>, release: bool) -> Result<ExitCode> {
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
    if resolved.origin == Origin::Inferred {
        ui::arrow(&resolved.describe(&package));
    }

    let profile = if release {
        crate::config::Profile::Release
    } else {
        config.config.test.profile
    };
    let executable = runner::build(&package.manifest_path, &resolved.bin.name, profile)?;

    // Inherit all three streams. Being usable interactively is the whole point,
    // so nothing sits in between.
    let status = Command::new(&executable)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .env("RUST_BACKTRACE", "1")
        .status()
        .with_context(|| format!("could not start {}", executable.display()))?;

    if status.success() {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::FAILURE)
    }
}
