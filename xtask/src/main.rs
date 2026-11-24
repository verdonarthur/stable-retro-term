//! Development tasks for retro-term. Never shipped.

mod check;
mod doctor;
mod process;
mod report;
mod worktree;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Development tasks for retro-term.
///
/// Exit codes: 0 success, 1 a step failed or a request was refused,
/// 2 usage error, 3 a required tool is missing.
#[derive(Parser)]
#[command(name = "cargo xtask", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Report missing or wrongly pinned tools. Installs nothing.
    Doctor(doctor::Args),
    /// Run the checks that CI runs.
    Check(check::Args),
    /// Create, list, and remove task worktrees under `.worktrees/`.
    #[command(subcommand)]
    Worktree(worktree::Command),
}

/// A failure with its process exit code.
#[derive(Debug)]
pub struct Failure {
    pub code: u8,
    pub message: String,
}

impl Failure {
    pub const FAILED: u8 = 1;
    pub const USAGE: u8 = 2;
    pub const MISSING_TOOL: u8 = 3;

    pub fn refused(message: impl Into<String>) -> Self {
        Self {
            code: Self::FAILED,
            message: message.into(),
        }
    }

    pub fn usage(message: impl Into<String>) -> Self {
        Self {
            code: Self::USAGE,
            message: message.into(),
        }
    }
}

pub type Outcome = Result<ExitCode, Failure>;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let outcome = match cli.command {
        Command::Doctor(args) => doctor::run(&args),
        Command::Check(args) => check::run(&args),
        Command::Worktree(command) => worktree::run(&command),
    };
    outcome.unwrap_or_else(|failure| {
        eprintln!("error: {}", failure.message);
        ExitCode::from(failure.code)
    })
}
