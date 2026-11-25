//! Development tasks for retro-term. Never shipped.

use std::process::ExitCode;

const USAGE: &str = "\
Usage: cargo xtask <command>

Commands:
  help    Show this message

No development commands are implemented yet. They arrive with task 02.";

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        None | Some("help" | "-h" | "--help") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(command) => {
            eprintln!("error: unknown command `{command}`\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
