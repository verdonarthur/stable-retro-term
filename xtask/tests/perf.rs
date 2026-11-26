//! Headless regression tests for the performance command.

#![cfg(target_os = "linux")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

#[test]
fn help_exposes_preflight() {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["perf", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("--preflight") && help.contains("--candidate-animated"));
}

#[test]
fn invalid_suite_and_missing_binary_have_distinct_exit_codes() {
    let temp = tempfile::tempdir().unwrap();
    let invalid = temp.path().join("invalid.toml");
    let valid = root().join("benchmarks/scenarios.toml");
    let report = temp.path().join("report.json");
    let missing = temp.path().join("no-such-terminal");
    let run = |suite: &Path| {
        Command::new(env!("CARGO_BIN_EXE_xtask"))
            .current_dir(root())
            .arg("perf")
            .arg("--candidate")
            .arg(&missing)
            .arg("--baseline")
            .arg(&missing)
            .arg("--suite")
            .arg(suite)
            .arg("--output")
            .arg(&report)
            .args(["--preflight", "--candidate-plain"])
            .output()
            .unwrap()
            .status
            .code()
    };
    let incomplete = fs::read_to_string(&valid)
        .unwrap()
        .replace(", \"lifecycle\"", "");
    fs::write(&invalid, incomplete).unwrap();
    assert_eq!(run(&invalid), Some(2));
    assert_eq!(run(&valid), Some(3));
    let report: serde_json::Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
    assert_eq!(report["status"], "unverified");
    assert_eq!(report["candidate_mode"], "plain");
    assert_eq!(report["reasons"].as_array().unwrap().len(), 2);
}

#[test]
fn animated_candidate_mode_is_reported_and_exclusive() {
    let temp = tempfile::tempdir().unwrap();
    let report = temp.path().join("report.json");
    let missing = temp.path().join("no-such-terminal");
    let run = |modes: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_xtask"))
            .current_dir(root())
            .arg("perf")
            .arg("--candidate")
            .arg(&missing)
            .arg("--baseline")
            .arg(&missing)
            .args(["--suite", "benchmarks/scenarios.toml", "--output"])
            .arg(&report)
            .arg("--preflight")
            .args(modes)
            .output()
            .unwrap()
            .status
            .code()
    };
    assert_eq!(run(&["--candidate-plain", "--candidate-animated"]), Some(2));
    assert_eq!(run(&["--candidate-animated"]), Some(3));
    let report: serde_json::Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
    assert_eq!(report["candidate_mode"], "animated-crt");
}

#[test]
fn rejects_a_mismatched_baseline_before_display_access() {
    let temp = tempfile::tempdir().unwrap();
    let baseline = temp.path().join("baseline");
    let report = temp.path().join("report.json");
    fs::write(&baseline, "#!/bin/sh\nprintf 'alacritty 99.0.0\\n'\n").unwrap();
    fs::set_permissions(&baseline, fs::Permissions::from_mode(0o755)).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .current_dir(root())
        .arg("perf")
        .args(["--candidate", "/bin/true", "--baseline"])
        .arg(baseline)
        .args(["--suite", "benchmarks/scenarios.toml", "--output"])
        .arg(&report)
        .arg("--preflight")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let report = fs::read_to_string(report).unwrap();
    assert!(report.contains("static-crt"));
    assert!(report.contains("expected Alacritty"));
    assert!(report.contains("99.0.0"));
}

#[test]
fn driver_headless_regressions() {
    let result = Command::new("python3")
        .args(["-B", "-m", "unittest", "discover", "-s", "xtask/src/perf"])
        .current_dir(root())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
