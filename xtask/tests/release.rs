//! Headless contracts of `package` and `release`, the release configuration, and the workflows.

#![cfg(target_os = "linux")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

fn xtask(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn code(out: &Output) -> i32 {
    out.status.code().unwrap()
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn step<'a>(report: &'a Value, name: &str) -> &'a Value {
    report["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == name)
        .unwrap_or_else(|| panic!("no step {name} in {report}"))
}

fn run(program: &str, args: &[&str], dir: &Path) -> String {
    let out = Command::new(program)
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "{program} {args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

#[test]
fn package_refuses_without_check() {
    let out = xtask(root(), &["package"]);
    assert_eq!(code(&out), 2);
    assert!(String::from_utf8_lossy(&out.stderr).contains("never publishes"));
}

#[test]
fn package_help_says_never_publishes() {
    let out = xtask(root(), &["package", "--help"]);
    assert_eq!(code(&out), 0);
    assert!(String::from_utf8_lossy(&out.stdout).contains("never publishes"));
}

#[test]
fn package_reports_missing_dist() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Cargo.toml"), "[workspace]\n").unwrap();
    let empty = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["package", "--check", "--json"])
        .current_dir(dir.path())
        .env("PATH", empty.path())
        .output()
        .unwrap();
    assert_eq!(code(&out), 3, "{out:?}");
    let report = read_json(&dir.path().join("target/xtask/package.json"));
    assert_eq!(step(&report, "dist")["status"], "missing");
}

fn workspace_members() -> Vec<String> {
    let metadata: Value = serde_json::from_str(&run(
        "cargo",
        &["metadata", "--locked", "--no-deps", "--format-version", "1"],
        root(),
    ))
    .unwrap();
    let mut names: Vec<String> = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap().to_owned())
        .collect();
    names.sort();
    names
}

fn toml_file(path: &str) -> toml::Table {
    toml::from_str(&fs::read_to_string(root().join(path)).unwrap()).unwrap()
}

#[test]
fn release_config_is_consistent() {
    let config = read_json(&root().join("release-please-config.json"));
    assert_eq!(config["draft"], true);
    assert_eq!(config["force-tag-creation"], true);
    assert_eq!(config["include-component-in-tag"], false);
    let package = &config["packages"]["."];
    let extra = package["extra-files"].as_array().unwrap();
    for file in extra {
        assert!(
            root().join(file["path"].as_str().unwrap()).is_file(),
            "{file}"
        );
    }

    let cargo = toml_file("Cargo.toml");
    let version = cargo["workspace"]["package"]["version"].as_str().unwrap();
    let manifest = read_json(&root().join(".release-please-manifest.json"));
    assert_eq!(manifest["."], version);

    let lock_path = extra.iter().find(|f| f["path"] == "Cargo.lock").unwrap()["jsonpath"]
        .as_str()
        .unwrap();
    let mut named: Vec<String> = lock_path
        .split("@.name==='")
        .skip(1)
        .map(|part| part.split('\'').next().unwrap().to_owned())
        .collect();
    named.sort();
    assert_eq!(named, workspace_members());
}

#[test]
fn dist_config_matches_pins() {
    let tools = toml_file("tools.toml");
    let dist = toml_file("dist-workspace.toml");
    let dist = dist["dist"].as_table().unwrap();
    assert_eq!(dist["cargo-dist-version"], tools["tools"]["dist"]);
    assert!(dist["ci"].as_array().unwrap().is_empty());
    assert!(dist["installers"].as_array().unwrap().is_empty());
    assert_eq!(dist["create-release"].as_bool(), Some(false));

    for member in workspace_members() {
        let dir = match member.as_str() {
            "xtask" => "xtask".to_owned(),
            name => format!("crates/{}", name.trim_start_matches("retro-")),
        };
        let manifest = toml_file(&format!("{dir}/Cargo.toml"));
        let flag = manifest["package"]
            .get("metadata")
            .and_then(|m| m.get("dist"))
            .and_then(|d| d.get("dist"))
            .and_then(toml::Value::as_bool);
        let expected = match member.as_str() {
            "retro-app" => Some(true),
            "xtask" => Some(false),
            _ => None,
        };
        assert_eq!(flag, expected, "{member}");
    }
}

const SHA: &str = "1111111111111111111111111111111111111111";

/// A workspace with a packaged archive, its identity, a gate file, and passing reports.
struct GateFixture {
    dir: tempfile::TempDir,
}

impl GateFixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path();
        fs::write(path.join("Cargo.toml"), "[workspace]\n").unwrap();
        let pkg = path.join("stage/retro-app");
        fs::create_dir_all(&pkg).unwrap();
        fs::write(pkg.join("retro-term"), "packaged binary").unwrap();
        fs::write(pkg.join("LICENSE"), "MIT").unwrap();
        fs::create_dir_all(path.join("distrib")).unwrap();
        run(
            "tar",
            &[
                "-cJf",
                "distrib/retro-app.tar.xz",
                "-C",
                "stage",
                "retro-app",
            ],
            path,
        );
        fs::write(path.join("scenarios.toml"), "suite\n").unwrap();
        fs::write(
            path.join("gate.toml"),
            "version = 1\nlimit = [6, 5]\nsuite = \"scenarios.toml\"\n\n\
             [[presets]]\nname = \"static-crt\"\ncandidate_plain = false\n\n\
             [[presets]]\nname = \"plain\"\ncandidate_plain = true\n",
        )
        .unwrap();
        let fixture = Self { dir };
        let identity = json!({
            "schema": 1, "version": "0.1.0", "tag": "v0.1.0", "source_sha": SHA, "dirty": false,
            "rustc": "rustc", "dist": "dist 0.34.0", "target": "x86_64-unknown-linux-gnu",
            "archive": {"path": "distrib/retro-app.tar.xz", "sha256": fixture.sha("distrib/retro-app.tar.xz")},
            "binary": {"path": "stage/retro-app/retro-term", "member": "retro-app/retro-term",
                       "sha256": fixture.sha("stage/retro-app/retro-term")},
        });
        fixture.write("identity.json", &identity);
        for (name, mode) in [("static-crt", "static-crt"), ("plain", "plain")] {
            fixture.write(&format!("reports/{name}.json"), &fixture.report(mode));
        }
        fixture
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.dir.path().join(relative)
    }

    fn sha(&self, relative: &str) -> String {
        let out = run("sha256sum", &[relative], self.dir.path());
        out.split_whitespace().next().unwrap().to_owned()
    }

    fn write(&self, relative: &str, value: &Value) {
        let path = self.path(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
    }

    fn report(&self, mode: &str) -> Value {
        json!({
            "schema": 1, "status": "passed", "limit": [6, 5], "preflight": false, "scenario": null,
            "source": SHA, "dirty": false, "candidate_mode": mode,
            "measurement": {"identity": {
                "candidate_sha256": self.sha("stage/retro-app/retro-term"),
                "suite_sha256": self.sha("scenarios.toml"),
            }},
        })
    }

    fn edit_report(&self, name: &str, edit: impl FnOnce(&mut Value)) {
        let path = format!("reports/{name}.json");
        let mut report = read_json(&self.path(&path));
        edit(&mut report);
        self.write(&path, &report);
    }

    fn gate(&self, extra: &[&str]) -> (i32, Value) {
        let args = [
            &[
                "release",
                "gate",
                "--identity",
                "identity.json",
                "--reports",
                "reports",
                "--gate",
                "gate.toml",
                "--source-sha",
                SHA,
            ],
            extra,
        ]
        .concat();
        let out = xtask(self.dir.path(), &args);
        let summary = self.path("target/xtask/release-gate.json");
        let report = if summary.is_file() {
            read_json(&summary)
        } else {
            Value::Null
        };
        (code(&out), report)
    }

    fn notes(&self) -> PathBuf {
        self.path("target/xtask/release-gate-notes.md")
    }
}

#[test]
fn gate_passes_only_with_bound_evidence() {
    let (code, report) = GateFixture::new().gate(&[]);
    assert_eq!(code, 0, "{report}");
    assert!(report.get("overridden").is_none());

    type Edit = Box<dyn Fn(&GateFixture)>;
    let report_edit = |key: &'static str, value: Value| -> Edit {
        Box::new(move |f: &GateFixture| f.edit_report("plain", |r| r[key] = value.clone()))
    };
    let cases: Vec<(&str, &str, Edit)> = vec![
        (
            "missing",
            "plain",
            Box::new(|f: &GateFixture| fs::remove_file(f.path("reports/plain.json")).unwrap()),
        ),
        (
            "unverified",
            "plain",
            report_edit("status", json!("unverified")),
        ),
        ("failed", "plain", report_edit("status", json!("failed"))),
        ("preflight", "plain", report_edit("preflight", json!(true))),
        ("scenario", "plain", report_edit("scenario", json!("idle"))),
        ("dirty", "plain", report_edit("dirty", json!(true))),
        (
            "stale",
            "plain",
            report_edit("source", json!("2".repeat(40))),
        ),
        (
            "checksum",
            "plain",
            Box::new(|f: &GateFixture| {
                f.edit_report("plain", |r| {
                    r["measurement"]["identity"]["candidate_sha256"] = json!("0".repeat(64))
                })
            }),
        ),
        (
            "mode",
            "plain",
            report_edit("candidate_mode", json!("static-crt")),
        ),
        ("limit", "plain", report_edit("limit", json!([7, 5]))),
        (
            "archive",
            "archive",
            Box::new(|f: &GateFixture| {
                fs::write(f.path("distrib/retro-app.tar.xz"), "changed").unwrap()
            }),
        ),
    ];
    for (case, failing, edit) in cases {
        let fixture = GateFixture::new();
        edit(&fixture);
        let (code, report) = fixture.gate(&[]);
        assert_eq!(code, 1, "{case}: {report}");
        assert_eq!(
            step(&report, failing)["status"],
            "failed",
            "{case}: {report}"
        );
        assert!(!fixture.notes().exists(), "{case}");
    }
}

#[test]
fn gate_override_allows_failed_performance_and_records_it() {
    let fixture = GateFixture::new();
    fixture.edit_report("plain", |r| r["status"] = json!("unverified"));
    let (code, report) = fixture.gate(&[]);
    assert_eq!(code, 1, "blocked by default: {report}");
    assert!(!fixture.notes().exists());

    let (code, report) = fixture.gate(&["--allow-failed-gate"]);
    assert_eq!(code, 0, "{report}");
    assert_eq!(report["overridden"], true);
    assert_eq!(step(&report, "plain")["status"], "failed");
    let notes = fs::read_to_string(fixture.notes()).unwrap();
    assert!(notes.contains("Performance gate not met"), "{notes}");
    assert!(notes.contains("`plain`"), "{notes}");
    assert!(!notes.contains("`static-crt`"), "{notes}");
}

#[test]
fn gate_override_never_skips_integrity_checks() {
    let fixture = GateFixture::new();
    fs::write(fixture.path("distrib/retro-app.tar.xz"), "changed").unwrap();
    let (code, report) = fixture.gate(&["--allow-failed-gate"]);
    assert_eq!(code, 1, "{report}");
    assert!(report.get("overridden").is_none());
    assert!(!fixture.notes().exists());
}

#[test]
fn gate_rejects_invalid_gate_file() {
    let original = fs::read_to_string(GateFixture::new().path("gate.toml")).unwrap();
    let cases = [
        format!("{original}unknown = 1\n"),
        format!("{original}\n[[presets]]\nname = \"plain\"\ncandidate_plain = true\n"),
        original.replace("[6, 5]", "[6, 0]"),
    ];
    for gate in cases {
        let fixture = GateFixture::new();
        fs::write(fixture.path("gate.toml"), &gate).unwrap();
        assert_eq!(fixture.gate(&[]).0, 2, "{gate}");
    }
}

#[test]
fn verify_rejects_mismatches() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    fs::write(
        path.join("Cargo.toml"),
        "[workspace]\n\n[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    fs::write(
        path.join("Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"a\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::create_dir(path.join("src")).unwrap();
    fs::write(path.join("src/lib.rs"), "").unwrap();
    fs::write(
        path.join(".release-please-manifest.json"),
        "{\".\": \"0.1.0\"}\n",
    )
    .unwrap();
    fs::write(
        path.join("CHANGELOG.md"),
        "# Changelog\n\n## 0.1.0 (2026-11-25)\n",
    )
    .unwrap();
    let git = |args: &[&str]| run("git", args, path);
    git(&["init", "-q"]);
    git(&["-c", "user.name=t", "-c", "user.email=t@t", "add", "."]);
    git(&[
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@t",
        "commit",
        "-qm",
        "one",
    ]);
    let first = git(&["rev-parse", "HEAD"]);
    git(&["tag", "v0.1.0"]);
    git(&["tag", "v0.2.0"]);
    let verify = |tag: &str, sha: &str| {
        code(&xtask(
            path,
            &["release", "verify", "--tag", tag, "--source-sha", sha],
        ))
    };

    assert_eq!(verify("0.1.0", &first), 2);
    assert_eq!(verify("v0.1", &first), 2);
    assert_eq!(verify("v0.1.0", &first), 0);
    assert_eq!(verify("v0.2.0", &first), 1, "tag and version differ");

    fs::write(
        path.join("Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"a\"\nversion = \"0.0.9\"\n",
    )
    .unwrap();
    assert_eq!(verify("v0.1.0", &first), 1, "lock differs");
    git(&["checkout", "-q", "Cargo.lock"]);

    git(&[
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@t",
        "commit",
        "-q",
        "--allow-empty",
        "-m",
        "two",
    ]);
    let second = git(&["rev-parse", "HEAD"]);
    assert_eq!(verify("v0.1.0", &first), 1, "HEAD is not the source");
    assert_eq!(verify("v0.1.0", &second), 1, "tag is on another commit");
}

fn workflow(name: &str) -> String {
    fs::read_to_string(root().join(".github/workflows").join(name)).unwrap()
}

/// Text of the job `name` in a workflow: from its key to the next job key.
fn job(text: &str, name: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let is_key =
        |line: &str| line.starts_with("  ") && !line.starts_with("   ") && line.ends_with(':');
    let start = lines
        .iter()
        .position(|line| *line == format!("  {name}:"))
        .unwrap_or_else(|| panic!("no job {name}"));
    let end = (start + 1..lines.len())
        .find(|&i| is_key(lines[i]))
        .unwrap_or(lines.len());
    lines[start..end].join("\n")
}

#[test]
fn workflows_keep_the_publication_boundary() {
    let dir = root().join(".github/workflows");
    for entry in fs::read_dir(&dir).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        let text = workflow(&name);
        assert!(!text.contains("tags:"), "{name} has a tag trigger");
        let writes = text
            .lines()
            .filter(|line| line.trim() == "contents: write")
            .count();
        let expected = match name.as_str() {
            "release.yml" | "release-please.yml" => 1,
            _ => 0,
        };
        assert_eq!(writes, expected, "{name}");
    }
    let release = workflow("release.yml");
    let publish = job(&release, "publish");
    assert!(publish.contains("contents: write"));
    assert!(!publish.contains("actions/checkout"));
    let please = workflow("release-please.yml");
    assert!(please.contains("RETRO_RELEASE_ENABLED"));
    assert!(job(&please, "release").contains("contents: write"));
}

#[test]
fn only_a_manual_input_can_override_the_gate() {
    let please = workflow("release-please.yml");
    assert!(please.contains("allow_failed_gate: false"));
    assert!(!please.contains("allow_failed_gate: true"));
    let release = workflow("release.yml");
    assert!(release.contains("--allow-failed-gate"));
    let dispatch = &release
        [release.find("workflow_dispatch:").unwrap()..release.find("\npermissions").unwrap()];
    assert!(dispatch.contains("allow_failed_gate:"));
    assert!(dispatch.contains("default: false"));
}
