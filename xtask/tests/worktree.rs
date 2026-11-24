//! Worktree helper behaviour in temporary Git repositories.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.name=test", "-c", "user.email=test@example.com"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

fn xtask(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

/// A repository with one commit and an ignored `.worktrees/`.
fn repo() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--quiet", "--initial-branch=main"]);
    fs::write(dir.path().join(".gitignore"), "/.worktrees/\n/target/\n").unwrap();
    fs::create_dir_all(dir.path().join("docs/AI")).unwrap();
    fs::write(dir.path().join("docs/AI/plan.md"), "plan\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "--quiet", "-m", "init"]);
    dir
}

fn code(out: &Output) -> i32 {
    out.status.code().unwrap()
}

#[test]
fn create_makes_branch_and_tracked_plans_are_present() {
    let repo = repo();
    let out = xtask(
        repo.path(),
        &["worktree", "create", "task-1", "--base", "main"],
    );
    assert_eq!(code(&out), 0, "{out:?}");
    let path = repo.path().join(".worktrees/task-1");
    assert!(path.join("docs/AI/plan.md").is_file());
    assert_eq!(git(&path, &["branch", "--show-current"]).trim(), "task-1");
}

#[test]
fn create_requires_base() {
    let repo = repo();
    assert_eq!(
        code(&xtask(repo.path(), &["worktree", "create", "task-1"])),
        2
    );
}

#[test]
fn create_refuses_bad_name_and_bad_base() {
    let repo = repo();
    for name in ["Task", "-x", "a/b", "..", ""] {
        let out = xtask(repo.path(), &["worktree", "create", name, "--base", "main"]);
        assert_eq!(code(&out), 2, "name {name:?}");
    }
    let out = xtask(repo.path(), &["worktree", "create", "ok", "--base", "nope"]);
    assert_eq!(code(&out), 1);
    assert!(!repo.path().join(".worktrees/ok").exists());
}

#[test]
fn create_refuses_path_and_branch_collisions() {
    let repo = repo();
    git(repo.path(), &["branch", "taken"]);
    let out = xtask(
        repo.path(),
        &["worktree", "create", "taken", "--base", "main"],
    );
    assert_eq!(code(&out), 1);

    fs::create_dir_all(repo.path().join(".worktrees/busy")).unwrap();
    let out = xtask(
        repo.path(),
        &["worktree", "create", "busy", "--base", "main"],
    );
    assert_eq!(code(&out), 1);
}

#[test]
fn list_reports_dirty_state_as_json() {
    let repo = repo();
    xtask(
        repo.path(),
        &["worktree", "create", "task-1", "--base", "main"],
    );
    fs::write(repo.path().join(".worktrees/task-1/new.txt"), "x").unwrap();
    let out = xtask(repo.path(), &["worktree", "list", "--json"]);
    assert_eq!(code(&out), 0);
    let entries: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let task = entries
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["branch"] == "task-1")
        .unwrap();
    assert_eq!(task["dirty"], true);
}

#[test]
fn remove_refuses_dirty_untracked_and_ignored_notes() {
    let repo = repo();
    xtask(
        repo.path(),
        &["worktree", "create", "task-1", "--base", "main"],
    );
    let path = repo.path().join(".worktrees/task-1");

    fs::write(path.join("docs/AI/plan.md"), "changed\n").unwrap();
    assert_eq!(
        code(&xtask(repo.path(), &["worktree", "remove", "task-1"])),
        1
    );
    git(&path, &["checkout", "--", "docs/AI/plan.md"]);

    fs::write(path.join("untracked.txt"), "x").unwrap();
    assert_eq!(
        code(&xtask(repo.path(), &["worktree", "remove", "task-1"])),
        1
    );
    fs::remove_file(path.join("untracked.txt")).unwrap();

    fs::write(path.join(".gitignore"), "/.worktrees/\n/target/\n*.notes\n").unwrap();
    git(&path, &["commit", "--quiet", "-am", "ignore notes"]);
    fs::write(path.join("docs/AI/private.notes"), "keep me").unwrap();
    assert_eq!(
        code(&xtask(repo.path(), &["worktree", "remove", "task-1"])),
        1
    );
    assert!(path.exists());
}

#[test]
fn remove_clean_worktree_keeps_branch() {
    let repo = repo();
    xtask(
        repo.path(),
        &["worktree", "create", "task-1", "--base", "main"],
    );
    let out = xtask(repo.path(), &["worktree", "remove", "task-1"]);
    assert_eq!(code(&out), 0, "{out:?}");
    assert!(!repo.path().join(".worktrees/task-1").exists());
    git(repo.path(), &["show-ref", "--verify", "refs/heads/task-1"]);
}

#[test]
fn force_remove_rescues_notes_first() {
    let repo = repo();
    xtask(
        repo.path(),
        &["worktree", "create", "task-1", "--base", "main"],
    );
    let path = repo.path().join(".worktrees/task-1");
    fs::write(path.join("docs/AI/plan.md"), "progress\n").unwrap();
    fs::write(path.join("docs/AI/handoff.md"), "handoff\n").unwrap();

    let out = xtask(repo.path(), &["worktree", "remove", "task-1", "--force"]);
    assert_eq!(code(&out), 0, "{out:?}");
    assert!(!path.exists());

    let rescued = fs::read_dir(repo.path().join("target/xtask/rescued"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let read = |file: &str| fs::read_to_string(rescued.join("docs/AI").join(file)).unwrap();
    assert_eq!(read("plan.md"), "progress\n");
    assert_eq!(read("handoff.md"), "handoff\n");
}

#[test]
fn remove_from_inside_a_worktree_targets_main_checkout() {
    let repo = repo();
    xtask(repo.path(), &["worktree", "create", "a", "--base", "main"]);
    xtask(repo.path(), &["worktree", "create", "b", "--base", "main"]);
    let inside_a = repo.path().join(".worktrees/a");
    assert_eq!(code(&xtask(&inside_a, &["worktree", "remove", "b"])), 0);
    assert!(!repo.path().join(".worktrees/b").exists());
}

#[test]
fn failed_git_propagates_exit_1() {
    let dir = tempfile::tempdir().unwrap();
    let out = xtask(dir.path(), &["worktree", "list"]);
    assert_eq!(code(&out), 1);
    assert!(String::from_utf8_lossy(&out.stderr).contains("git"));
}
