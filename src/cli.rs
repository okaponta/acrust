//! The command surface. One entry point, `acrust`, and no others.

use crate::commands;
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(
    name = "acrust",
    version,
    about = "A competitive programming CLI built for AtCoder and Rust"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Log in to AtCoder (imports the session cookie from your browser)
    Login {
        /// The REVEL_SESSION value. Prompts for it (hidden) when omitted
        #[arg(long, value_name = "REVEL_SESSION")]
        cookie: Option<String>,
        /// Do not open AtCoder in a browser
        #[arg(long)]
        no_open: bool,
    },
    /// Discard the saved session
    Logout,
    /// Show the login state, where the config lives, and the judge environment
    Status {
        /// Show only local information, without asking AtCoder
        #[arg(long)]
        offline: bool,
    },
    /// Initialize. Generates .acrust/ and rust-toolchain.toml
    Init {
        /// Target directory (default: the current directory)
        #[arg(long, value_name = "DIR")]
        path: Option<PathBuf>,
        /// Overwrite existing files
        #[arg(long)]
        force: bool,
    },
    /// Migrate a cargo-compete repository to the acrust layout
    Migrate {
        /// Actually write (dry-run by default)
        #[arg(long)]
        write: bool,
        /// Run even when the git working tree is dirty
        #[arg(long)]
        allow_dirty: bool,
    },
    /// Create the package for a contest and fetch its samples
    New {
        /// Contest ID (e.g. abc474)
        contest: String,
    },
    /// Fetch the samples again
    Fetch {
        /// Contest ID (defaults to the current package)
        contest: Option<String>,
        /// Replace everything with what was fetched, dropping hand-added cases and hand-edited match
        #[arg(long)]
        overwrite: bool,
    },
    /// Build and run the sample tests
    Test {
        /// Problem (e.g. a). Defaults to the most recently modified one
        problem: Option<String>,
        /// Build with the release profile
        #[arg(long)]
        release: bool,
    },
    /// Run the solution with stdin passed straight through
    Run {
        /// Problem (e.g. a). Defaults to the most recently modified one
        problem: Option<String>,
        /// Build with the release profile
        #[arg(long)]
        release: bool,
    },
    /// Test, then submit and follow the result
    Submit {
        /// Problem (e.g. a). Inferred with a y/N confirmation when omitted
        problem: Option<String>,
        /// Skip the sample tests before submitting
        #[arg(short, long)]
        force: bool,
        /// Do not follow the result after submitting
        #[arg(long)]
        no_watch: bool,
    },
    /// Copy the solution to the clipboard
    Copy {
        /// Problem (e.g. a). Defaults to the most recently modified one
        problem: Option<String>,
    },
    /// Open the problem in a browser
    Open {
        /// Problem (e.g. a). Defaults to every problem
        problem: Option<String>,
    },
    /// Keep up with AtCoder's judge environment
    Env {
        #[command(subcommand)]
        command: EnvCommand,
    },
}

#[derive(Debug, Subcommand)]
enum EnvCommand {
    /// Look up the rustc, edition and crates the judge uses, and match the config to them
    Update {
        /// URL of the language list page (use it to point at a newer language update)
        #[arg(long, value_name = "URL")]
        language_list: Option<String>,
        /// Write without asking
        #[arg(long)]
        yes: bool,
    },
}

pub fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        Command::Login { cookie, no_open } => commands::auth::login(cookie, no_open)?,
        Command::Logout => commands::auth::logout()?,
        Command::Status { offline } => commands::auth::status(offline)?,
        Command::Init { path, force } => commands::init::run(path, force)?,
        Command::Migrate { write, allow_dirty } => commands::migrate::run(write, allow_dirty)?,
        Command::New { contest } => commands::contest::new(&contest)?,
        Command::Fetch { contest, overwrite } => commands::contest::fetch(contest, overwrite)?,
        Command::Test { problem, release } => return commands::test::run(problem, release),
        Command::Run { problem, release } => return commands::run::run(problem, release),
        Command::Submit {
            problem,
            force,
            no_watch,
        } => return commands::submit::run(problem, force, no_watch),
        Command::Copy { problem } => commands::copy::run(problem)?,
        Command::Open { problem } => commands::open::run(problem)?,
        Command::Env { command } => match command {
            EnvCommand::Update { language_list, yes } => commands::env::update(language_list, yes)?,
        },
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory as _;

    #[test]
    fn the_cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn submit_takes_the_documented_flags() {
        let cli = Cli::try_parse_from(["acrust", "submit", "c", "-f", "--no-watch"]).unwrap();
        match cli.command {
            Command::Submit {
                problem,
                force,
                no_watch,
            } => {
                assert_eq!(problem.as_deref(), Some("c"));
                assert!(force);
                assert!(no_watch);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn the_problem_argument_is_optional_everywhere_it_is_inferred() {
        for command in ["test", "run", "submit", "copy", "open"] {
            Cli::try_parse_from(["acrust", command]).unwrap_or_else(|e| panic!("{command}: {e}"));
        }
    }

    #[test]
    fn env_update_is_a_nested_subcommand() {
        let cli = Cli::try_parse_from(["acrust", "env", "update"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Env {
                command: EnvCommand::Update { .. }
            }
        ));
    }
}
