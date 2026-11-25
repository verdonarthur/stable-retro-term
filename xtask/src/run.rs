use std::process::ExitCode;

use crate::process::{artifacts_dir, run_logged, workspace_root};
use crate::{Failure, Outcome};

pub(crate) fn run() -> Outcome {
    let root = workspace_root()?;
    let log = artifacts_dir(&root).join("logs/run.log");
    println!("Development diagnostics: {}", log.display());
    let code = run_logged(
        "cargo",
        &[
            "run",
            "--locked",
            "-p",
            "retro-app",
            "--bin",
            "retro-term",
            "--",
            "--development",
        ],
        &root,
        &log,
    )?;
    match code.and_then(|code| u8::try_from(code).ok()) {
        Some(code) => Ok(ExitCode::from(code)),
        None => Err(Failure::refused(format!(
            "terminal process ended without an exit code; see {}",
            log.display()
        ))),
    }
}
