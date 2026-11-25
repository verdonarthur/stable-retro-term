//! `cargo xtask worktree`: task worktrees under `.worktrees/<name>`.
//!
//! The helpers never delete branches and never discard task notes under `docs/AI/`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::process::{artifacts_dir, io_failure, output, succeeds};
use crate::{Failure, Outcome};

const NOTES_DIR: &str = "docs/AI";

#[derive(clap::Subcommand)]
pub enum Command {
    /// Create `.worktrees/<name>` on a new branch `<name>` from `--base`.
    Create {
        name: String,
        /// Commit, branch, or tag to start from. Required: no default base is assumed.
        #[arg(long)]
        base: String,
    },
    /// List worktrees with their branch and dirty state.
    List {
        /// Print JSON on stdout.
        #[arg(long)]
        json: bool,
    },
    /// Remove `.worktrees/<name>`. The branch is kept.
    Remove {
        name: String,
        /// Remove even with uncommitted or untracked files.
        /// Notes under `docs/AI/` are copied to `target/xtask/rescued/` first.
        #[arg(long)]
        force: bool,
    },
}

pub fn run(command: &Command) -> Outcome {
    let main_root = main_checkout()?;
    match command {
        Command::Create { name, base } => create(&main_root, name, base),
        Command::List { json } => list(&main_root, *json),
        Command::Remove { name, force } => remove(&main_root, name, *force),
    }
}

/// The main checkout, also when called from inside a task worktree.
fn main_checkout() -> Result<PathBuf, Failure> {
    let cwd = env::current_dir().map_err(|e| io_failure(Path::new("."), &e))?;
    let common = output(
        "git",
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        &cwd,
    )?;
    Path::new(&common)
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| Failure::refused(format!("unexpected git directory `{common}`")))
}

fn validate_name(name: &str) -> Result<(), Failure> {
    let valid = name
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && name
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
    if valid {
        Ok(())
    } else {
        Err(Failure::usage(format!(
            "invalid worktree name `{name}`: use lowercase letters, digits, and `-`, starting with a letter or digit"
        )))
    }
}

fn worktree_path(main_root: &Path, name: &str) -> PathBuf {
    main_root.join(".worktrees").join(name)
}

fn create(main_root: &Path, name: &str, base: &str) -> Outcome {
    validate_name(name)?;
    let path = worktree_path(main_root, name);
    if path.exists() {
        return Err(Failure::refused(format!(
            "{} already exists",
            path.display()
        )));
    }
    let branch_ref = format!("refs/heads/{name}");
    if succeeds(
        "git",
        &["show-ref", "--verify", "--quiet", &branch_ref],
        main_root,
    ) {
        return Err(Failure::refused(format!("branch `{name}` already exists")));
    }
    let base_commit = format!("{base}^{{commit}}");
    let base_sha = output(
        "git",
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            &base_commit,
        ],
        main_root,
    )
    .map_err(|_| Failure::refused(format!("base `{base}` is not a commit")))?;
    let path_arg = path.to_string_lossy();
    output(
        "git",
        &["worktree", "add", "-b", name, &path_arg, &base_sha],
        main_root,
    )?;
    println!(
        "created {} on branch `{name}` from {base} ({base_sha})",
        path.display()
    );
    Ok(ExitCode::SUCCESS)
}

#[derive(Serialize)]
struct Entry {
    path: PathBuf,
    branch: Option<String>,
    head: Option<String>,
    dirty: bool,
}

fn list(main_root: &Path, json: bool) -> Outcome {
    let porcelain = output("git", &["worktree", "list", "--porcelain"], main_root)?;
    let entries: Vec<Entry> = porcelain
        .split("\n\n")
        .filter_map(|block| {
            let field = |key: &str| {
                block
                    .lines()
                    .find_map(|line| line.strip_prefix(key))
                    .map(str::to_owned)
            };
            let path = PathBuf::from(field("worktree ")?);
            let dirty = path.is_dir() && !status(&path).unwrap_or_default().is_empty();
            Some(Entry {
                branch: field("branch refs/heads/"),
                head: field("HEAD "),
                dirty,
                path,
            })
        })
        .collect();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&entries).expect("entries serialize")
        );
    } else {
        for entry in &entries {
            println!(
                "{}  {}{}",
                entry.path.display(),
                entry.branch.as_deref().unwrap_or("(detached)"),
                if entry.dirty { "  dirty" } else { "" }
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn status(path: &Path) -> Result<String, Failure> {
    output(
        "git",
        &["status", "--porcelain", "--untracked-files=all"],
        path,
    )
}

/// Changed, untracked, and ignored files under `docs/AI/`.
fn unsaved_notes(path: &Path) -> Result<Vec<String>, Failure> {
    let files = output(
        "git",
        &["ls-files", "--modified", "--others", "--", NOTES_DIR],
        path,
    )?;
    let mut files: Vec<String> = files.lines().map(str::to_owned).collect();
    files.dedup();
    Ok(files)
}

fn remove(main_root: &Path, name: &str, force: bool) -> Outcome {
    validate_name(name)?;
    let path = worktree_path(main_root, name);
    if !path.is_dir() {
        return Err(Failure::refused(format!(
            "no worktree at {}",
            path.display()
        )));
    }
    let changes = status(&path)?;
    let notes = unsaved_notes(&path)?;
    if !force && (!changes.is_empty() || !notes.is_empty()) {
        let mut message = format!("refusing to remove {}:", path.display());
        for line in changes.lines() {
            message.push_str(&format!("\n  {line}"));
        }
        for note in &notes {
            message.push_str(&format!("\n  note {note}"));
        }
        message.push_str("\ncommit or move this work, or pass --force (notes are rescued first)");
        return Err(Failure::refused(message));
    }
    if !notes.is_empty() {
        let rescue = rescue_notes(main_root, name, &path, &notes)?;
        println!(
            "rescued {} note file(s) to {}",
            notes.len(),
            rescue.display()
        );
    }
    let path_arg = path.to_string_lossy();
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(&path_arg);
    output("git", &args, main_root)?;
    println!("removed {}; branch `{name}` is kept", path.display());
    Ok(ExitCode::SUCCESS)
}

fn rescue_notes(
    main_root: &Path,
    name: &str,
    worktree: &Path,
    notes: &[String],
) -> Result<PathBuf, Failure> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let rescue = artifacts_dir(main_root)
        .join("rescued")
        .join(format!("{name}-{stamp}"));
    for note in notes {
        let from = worktree.join(note);
        let to = rescue.join(note);
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(|e| io_failure(parent, &e))?;
        }
        fs::copy(&from, &to).map_err(|e| io_failure(&from, &e))?;
    }
    Ok(rescue)
}
