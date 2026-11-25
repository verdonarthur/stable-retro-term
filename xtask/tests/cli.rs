//! Exit-code and JSON contracts of `doctor` and `check`.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn xtask(dir: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
    command.args(args).current_dir(dir);
    command
}

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

fn code(out: &Output) -> i32 {
    out.status.code().unwrap()
}

#[test]
fn help_and_usage_errors() {
    let root = workspace_root();
    assert_eq!(code(&xtask(root, &["--help"]).output().unwrap()), 0);
    assert_eq!(code(&xtask(root, &["nope"]).output().unwrap()), 2);
}

#[test]
fn run_has_help_and_reports_an_unavailable_cargo() {
    let help = xtask(workspace_root(), &["run", "--help"])
        .output()
        .unwrap();
    assert_eq!(code(&help), 0);
    assert!(String::from_utf8_lossy(&help.stdout).contains("Wayland"));

    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Cargo.toml"), "[workspace]\n").unwrap();
    let out = xtask(dir.path(), &["run"])
        .env("PATH", dir.path())
        .output()
        .unwrap();
    assert_eq!(code(&out), 1);
    assert!(String::from_utf8_lossy(&out.stderr).contains("cannot start `cargo`"));
    assert!(dir.path().join("target/xtask/logs/run.log").is_file());
}

#[test]
fn doctor_reports_missing_tools_with_exit_3() {
    let empty = tempfile::tempdir().unwrap();
    let out = xtask(workspace_root(), &["doctor", "--json"])
        .env("PATH", empty.path())
        .output()
        .unwrap();
    assert_eq!(code(&out), 3, "{out:?}");
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let deny = report["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "cargo-deny")
        .unwrap();
    assert_eq!(deny["status"], "missing");
    assert!(
        deny["note"]
            .as_str()
            .unwrap()
            .contains("cargo install --locked cargo-deny@")
    );
}

#[test]
fn check_rejects_unknown_package() {
    let out = xtask(workspace_root(), &["check", "--package", "nope"])
        .output()
        .unwrap();
    assert_eq!(code(&out), 2);
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown package `nope`"));
}

#[test]
fn check_propagates_failed_step() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[workspace]\n\n[package]\nname = \"bad\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"bad\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("src/lib.rs"),
        "pub fn   badly_formatted( ) {}\n",
    )
    .unwrap();

    let out = xtask(
        dir.path(),
        &["check", "--step", "fmt", "--step", "check", "--json"],
    )
    .output()
    .unwrap();
    assert_eq!(code(&out), 1, "{out:?}");
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let status = |name: &str| {
        report["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == name)
            .unwrap()["status"]
            .clone()
    };
    assert_eq!(status("fmt"), "failed");
    assert_eq!(status("check"), "passed");
    assert_eq!(status("deny"), "skipped");
    assert!(dir.path().join("target/xtask/check.json").is_file());
    assert!(dir.path().join("target/xtask/logs/fmt.log").is_file());
}
