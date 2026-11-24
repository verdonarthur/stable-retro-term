//! Process helpers. Commands always take argument arrays, never shell strings.

use std::env;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::Failure;

/// Returns the first executable named `name` on `PATH`.
pub fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// Runs `program args` in `dir`, writes stdout and stderr to `log`, and returns the exit code.
/// A process killed by a signal returns `None`.
pub fn run_logged(
    program: &str,
    args: &[&str],
    dir: &Path,
    log: &Path,
) -> Result<Option<i32>, Failure> {
    if let Some(parent) = log.parent() {
        fs::create_dir_all(parent).map_err(|e| io_failure(parent, &e))?;
    }
    let out = File::create(log).map_err(|e| io_failure(log, &e))?;
    let err = out.try_clone().map_err(|e| io_failure(log, &e))?;
    let status = Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .status()
        .map_err(|e| Failure::refused(format!("cannot start `{program}`: {e}")))?;
    Ok(status.code())
}

/// Runs `program args` in `dir` and returns trimmed stdout. A non-zero exit becomes a failure.
pub fn output(program: &str, args: &[&str], dir: &Path) -> Result<String, Failure> {
    let out = Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| Failure::refused(format!("cannot start `{program}`: {e}")))?;
    if !out.status.success() {
        return Err(Failure::refused(format!(
            "`{program} {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Returns true when `program args` exits with status 0.
pub fn succeeds(program: &str, args: &[&str], dir: &Path) -> bool {
    Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub fn io_failure(path: &Path, error: &std::io::Error) -> Failure {
    Failure::refused(format!("{}: {error}", path.display()))
}

/// Returns the nearest ancestor of the current directory whose `Cargo.toml` declares `[workspace]`.
pub fn workspace_root() -> Result<PathBuf, Failure> {
    let cwd = env::current_dir().map_err(|e| io_failure(Path::new("."), &e))?;
    cwd.ancestors()
        .find(|dir| {
            fs::read_to_string(dir.join("Cargo.toml"))
                .is_ok_and(|manifest| manifest.lines().any(|line| line.trim() == "[workspace]"))
        })
        .map(Path::to_path_buf)
        .ok_or_else(|| Failure::refused("no Cargo workspace found above the current directory"))
}

/// Directory for xtask logs and summaries in this worktree.
pub fn artifacts_dir(root: &Path) -> PathBuf {
    root.join("target").join("xtask")
}
