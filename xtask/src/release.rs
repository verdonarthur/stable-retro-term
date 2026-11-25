//! `cargo xtask release`: verify a release tag, measure the packaged binary, and decide the gate.
//! These commands never tag, upload, or publish.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};

use clap::Subcommand;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::package::{Identity, sha256_file};
use crate::perf::{self, Limit};
use crate::process::{artifacts_dir, io_failure, output, workspace_root};
use crate::report::{Report, Status, Step};
use crate::{Failure, Outcome};

const NOTES: &str = "release-gate-notes.md";

#[derive(Subcommand)]
pub enum Command {
    /// Check that the tag, source SHA, versions, and changelog agree.
    Verify(VerifyArgs),
    /// Measure the packaged binary for every gate preset. It never decides publication.
    Measure(MeasureArgs),
    /// Decide from report files whether the packaged release may publish. It never runs the binary.
    Gate(GateArgs),
}

#[derive(clap::Args)]
pub struct VerifyArgs {
    #[arg(long, value_name = "TAG")]
    tag: String,
    #[arg(long, value_name = "SHA")]
    source_sha: String,
    /// Print the JSON summary on stdout.
    #[arg(long)]
    json: bool,
}

#[derive(clap::Args)]
pub struct MeasureArgs {
    #[arg(long)]
    identity: PathBuf,
    #[arg(long)]
    gate: PathBuf,
    #[arg(long)]
    baseline: PathBuf,
    #[arg(long)]
    output_dir: PathBuf,
    /// Print the JSON summary on stdout.
    #[arg(long)]
    json: bool,
}

#[derive(clap::Args)]
pub struct GateArgs {
    #[arg(long)]
    identity: PathBuf,
    #[arg(long)]
    reports: PathBuf,
    #[arg(long)]
    gate: PathBuf,
    #[arg(long, value_name = "SHA")]
    source_sha: String,
    /// Manual override: allow publication when performance presets fail or are unverified.
    /// Integrity checks still block. The summary and the release notes record the override.
    #[arg(long)]
    allow_failed_gate: bool,
    /// Print the JSON summary on stdout.
    #[arg(long)]
    json: bool,
}

/// The release gate data file (`benchmarks/release-gate.toml`).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GateFile {
    version: u8,
    limit: [u32; 2],
    suite: PathBuf,
    presets: Vec<Preset>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Preset {
    name: String,
    candidate_plain: bool,
}

impl GateFile {
    fn read(path: &Path) -> Result<Self, Failure> {
        let text = fs::read_to_string(path).map_err(|e| io_failure(path, &e))?;
        let gate: Self = toml::from_str(&text)
            .map_err(|e| Failure::usage(format!("{}: {e}", path.display())))?;
        let names: BTreeSet<&str> = gate.presets.iter().map(|p| p.name.as_str()).collect();
        if gate.version != 1
            || gate.limit.contains(&0)
            || gate.presets.is_empty()
            || names.len() != gate.presets.len()
            || names.contains("")
        {
            return Err(Failure::usage(format!(
                "{}: needs version 1, a limit without zero, and unique non-empty preset names",
                path.display()
            )));
        }
        Ok(gate)
    }

    fn limit(&self) -> Limit {
        Limit(self.limit[0], self.limit[1])
    }

    fn suite_path(&self, gate: &Path) -> PathBuf {
        gate.parent().unwrap_or(Path::new(".")).join(&self.suite)
    }
}

pub fn run(command: &Command) -> Outcome {
    match command {
        Command::Verify(args) => verify(args),
        Command::Measure(args) => measure(args),
        Command::Gate(args) => gate(args),
    }
}

/// Returns the version of a `vX.Y.Z` tag.
pub fn tag_version(tag: &str) -> Result<&str, Failure> {
    let version = tag.strip_prefix('v').unwrap_or_default();
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    {
        Ok(version)
    } else {
        Err(Failure::usage(format!("tag `{tag}` is not vX.Y.Z")))
    }
}

fn verify(args: &VerifyArgs) -> Outcome {
    tag_version(&args.tag)?;
    let root = workspace_root()?;
    let steps = verify_steps(&root, &args.tag, &args.source_sha)?;
    Report::new("release-verify", steps, false).finish(&artifacts_dir(&root), args.json)
}

/// The tag, source, version, lockfile, manifest, and changelog checks.
pub fn verify_steps(root: &Path, tag: &str, sha: &str) -> Result<Vec<Step>, Failure> {
    let version = tag_version(tag)?;
    let check = |name: &str, result: Result<(), String>| match result {
        Ok(()) => Step::new(name, Status::Passed),
        Err(note) => Step::new(name, Status::Failed).with_note(note),
    };
    let equal = |what: &str, found: &str, expected: &str| {
        if found == expected {
            Ok(())
        } else {
            Err(format!("{what} is {found}, expected {expected}"))
        }
    };
    let git = |args: &[&str]| output("git", args, root).map_err(|f| f.message);
    let tag_ref = format!("refs/tags/{tag}^{{commit}}");
    let members = members(root)?;
    Ok(vec![
        check(
            "head",
            git(&["rev-parse", "HEAD"]).and_then(|h| equal("HEAD", &h, sha)),
        ),
        check(
            "tag",
            git(&["rev-parse", "--verify", &tag_ref]).and_then(|t| equal(tag, &t, sha)),
        ),
        check(
            "version",
            members
                .iter()
                .try_for_each(|(name, found)| equal(name, found, version)),
        ),
        check("lock", lock_versions(root, &members, version)),
        check(
            "manifest",
            manifest_version(root)
                .and_then(|v| equal(".release-please-manifest.json", &v, version)),
        ),
        check("changelog", changelog_has(root, version)),
    ])
}

/// Names and versions of the workspace packages.
fn members(root: &Path) -> Result<Vec<(String, String)>, Failure> {
    let metadata = output(
        "cargo",
        &["metadata", "--locked", "--no-deps", "--format-version", "1"],
        root,
    )?;
    let metadata: Value = serde_json::from_str(&metadata)
        .map_err(|e| Failure::refused(format!("cannot read cargo metadata: {e}")))?;
    Ok(metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| {
            let field = |key: &str| p[key].as_str().unwrap_or_default().to_owned();
            (field("name"), field("version"))
        })
        .collect())
}

pub fn workspace_version(root: &Path) -> Result<String, Failure> {
    let versions: BTreeSet<String> = members(root)?.into_iter().map(|(_, v)| v).collect();
    match versions.into_iter().collect::<Vec<_>>().as_slice() {
        [version] => Ok(version.clone()),
        other => Err(Failure::refused(format!(
            "workspace packages have different versions: {other:?}"
        ))),
    }
}

fn lock_versions(root: &Path, members: &[(String, String)], version: &str) -> Result<(), String> {
    let path = root.join("Cargo.lock");
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let lock: toml::Table = toml::from_str(&text).map_err(|e| e.to_string())?;
    let packages = lock
        .get("package")
        .and_then(|p| p.as_array())
        .ok_or("Cargo.lock has no packages")?;
    for (name, _) in members {
        let entry = packages
            .iter()
            .find(|p| {
                p.get("name").and_then(|n| n.as_str()) == Some(name) && p.get("source").is_none()
            })
            .ok_or_else(|| format!("Cargo.lock has no {name}"))?;
        let found = entry
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if found != version {
            return Err(format!("Cargo.lock {name} is {found}, expected {version}"));
        }
    }
    Ok(())
}

fn manifest_version(root: &Path) -> Result<String, String> {
    let path = root.join(".release-please-manifest.json");
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let manifest: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    manifest["."]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "manifest has no \".\" version".to_owned())
}

fn changelog_has(root: &Path, version: &str) -> Result<(), String> {
    let path = root.join("CHANGELOG.md");
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let heading = text
        .lines()
        .filter(|line| line.starts_with('#'))
        .any(|line| {
            line.split(|c: char| c.is_whitespace() || "[]()".contains(c))
                .any(|word| word == version)
        });
    if heading {
        Ok(())
    } else {
        Err(format!("CHANGELOG.md has no heading for {version}"))
    }
}

fn measure(args: &MeasureArgs) -> Outcome {
    let root = workspace_root()?;
    let gate = GateFile::read(&args.gate)?;
    let identity = Identity::read(&args.identity)?;
    let binary = root.join(&identity.binary.path);
    let mut steps = Vec::new();
    let actual = sha256_file(&binary)?;
    if actual != identity.binary.sha256 {
        steps.push(Step::new("binary", Status::Failed).with_note(format!(
            "{} sha256 is {actual}, identity says {}",
            binary.display(),
            identity.binary.sha256
        )));
        return Report::new("release-measure", steps, false)
            .finish(&artifacts_dir(&root), args.json);
    }
    steps.push(Step::new("binary", Status::Passed));
    for preset in &gate.presets {
        let run = |preflight: bool, file: String| -> Result<String, Failure> {
            let output = args.output_dir.join(file);
            perf::run(&perf::Args {
                candidate: binary.clone(),
                candidate_plain: preset.candidate_plain,
                baseline: args.baseline.clone(),
                suite: gate.suite_path(&args.gate),
                output: output.clone(),
                preflight,
                scenario: None,
                limit: gate.limit(),
            })?;
            Ok(report_status(&output))
        };
        let preflight = run(true, format!("{}-preflight.json", preset.name))?;
        let status = if preflight == "passed" {
            run(false, format!("{}.json", preset.name))?
        } else {
            format!("preflight {preflight}")
        };
        steps.push(if status == "passed" {
            Step::new(&preset.name, Status::Passed)
        } else {
            Step::new(&preset.name, Status::Failed).with_note(status)
        });
    }
    Report::new("release-measure", steps, false).finish(&artifacts_dir(&root), args.json)
}

fn report_status(path: &Path) -> String {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|report| report["status"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "unreadable".to_owned())
}

fn gate(args: &GateArgs) -> Outcome {
    let root = workspace_root()?;
    let gate = GateFile::read(&args.gate)?;
    let identity = Identity::read(&args.identity)?;
    let suite = gate.suite_path(&args.gate);
    let suite_sha = sha256_file(&suite)?;

    let mut steps = vec![
        archive_step(&root, &identity),
        identity_step(&identity, &args.source_sha),
    ];
    let integrity_failed = steps.iter().any(|s| s.status != Status::Passed);
    for preset in &gate.presets {
        let path = args.reports.join(format!("{}.json", preset.name));
        let expected = Expected {
            sha: &args.source_sha,
            binary_sha: &identity.binary.sha256,
            suite_sha: &suite_sha,
            mode: if preset.candidate_plain {
                "plain"
            } else {
                "static-crt"
            },
            limit: gate.limit,
        };
        steps.push(match preset_problems(&path, &expected) {
            Ok(()) => Step::new(&preset.name, Status::Passed),
            Err(note) => Step::new(&preset.name, Status::Failed).with_note(note),
        });
    }

    let dir = artifacts_dir(&root);
    let notes = dir.join(NOTES);
    if notes.exists() {
        fs::remove_file(&notes).map_err(|e| io_failure(&notes, &e))?;
    }
    let mut report = Report::new("release-gate", steps, false);
    let performance_failed = report.steps.iter().any(|s| s.status == Status::Failed);
    if args.allow_failed_gate && !integrity_failed && performance_failed {
        report.exit_code = 0;
        report.overridden = true;
        fs::create_dir_all(&dir).map_err(|e| io_failure(&dir, &e))?;
        fs::write(&notes, override_notes(&report, gate.limit))
            .map_err(|e| io_failure(&notes, &e))?;
        eprintln!("warning: performance gate not met; publication allowed by --allow-failed-gate");
    }
    report.finish(&dir, args.json)
}

fn override_notes(report: &Report, limit: [u32; 2]) -> String {
    let mut text = format!(
        "## Performance gate not met\n\n\
         This release was published with the manual override `allow_failed_gate`.\n\
         The resource gate (ratio limit {}/{}) did not pass or was not verified for these presets:\n\n",
        limit[0], limit[1]
    );
    for step in report.steps.iter().filter(|s| s.status == Status::Failed) {
        text.push_str(&format!(
            "- `{}`: {}\n",
            step.name,
            step.note.as_deref().unwrap_or("failed")
        ));
    }
    text.push_str("\nThe attached performance evidence archive holds the gate summary and every performance report.\n");
    text
}

fn archive_step(root: &Path, identity: &Identity) -> Step {
    match archive_problems(root, identity) {
        Ok(()) => Step::new("archive", Status::Passed),
        Err(note) => Step::new("archive", Status::Failed).with_note(note),
    }
}

/// The archive must match the identity, and the binary inside it must match too.
fn archive_problems(root: &Path, identity: &Identity) -> Result<(), String> {
    let archive = root.join(&identity.archive.path);
    let actual = sha256_file(&archive).map_err(|f| f.message)?;
    if actual != identity.archive.sha256 {
        return Err(format!(
            "archive sha256 is {actual}, identity says {}",
            identity.archive.sha256
        ));
    }
    let extracted = Process::new("tar")
        .arg("-xJOf")
        .arg(&archive)
        .arg(&identity.binary.member)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("cannot start `tar`: {e}"))?;
    if !extracted.status.success() {
        return Err(format!("archive has no {}", identity.binary.member));
    }
    let binary: String = Sha256::digest(&extracted.stdout)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if binary != identity.binary.sha256 {
        return Err(format!(
            "archived binary sha256 is {binary}, identity says {}",
            identity.binary.sha256
        ));
    }
    Ok(())
}

fn identity_step(identity: &Identity, sha: &str) -> Step {
    let problem = if identity.source_sha != sha {
        Some(format!(
            "identity source {} is not {sha}",
            identity.source_sha
        ))
    } else if identity.dirty {
        Some("package was built from a dirty worktree".to_owned())
    } else if identity.tag.is_none() {
        Some("package was built without --tag".to_owned())
    } else {
        None
    };
    match problem {
        None => Step::new("identity", Status::Passed),
        Some(note) => Step::new("identity", Status::Failed).with_note(note),
    }
}

struct Expected<'a> {
    sha: &'a str,
    binary_sha: &'a str,
    suite_sha: &'a str,
    mode: &'a str,
    limit: [u32; 2],
}

/// The rules that bind one performance report to the packaged release.
fn preset_problems(path: &Path, expected: &Expected) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|_| format!("missing report {}", path.display()))?;
    let report: Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("unreadable report {}: {e}", path.display()))?;
    let identity = &report["measurement"]["identity"];
    let rules: [(bool, String); 9] = [
        (
            report["status"] == "passed",
            format!("status {}", report["status"]),
        ),
        (report["preflight"] == false, "preflight report".into()),
        (
            report["scenario"].is_null(),
            format!("partial report for scenario {}", report["scenario"]),
        ),
        (report["dirty"] == false, "measured a dirty worktree".into()),
        (
            report["source"] == expected.sha,
            format!("stale report from source {}", report["source"]),
        ),
        (
            identity["candidate_sha256"] == expected.binary_sha,
            "candidate sha256 is not the packaged binary".into(),
        ),
        (
            report["candidate_mode"] == expected.mode,
            format!(
                "candidate mode {}, expected {}",
                report["candidate_mode"], expected.mode
            ),
        ),
        (
            report["limit"] == serde_json::json!(expected.limit),
            format!("limit {}, expected {:?}", report["limit"], expected.limit),
        ),
        (
            identity["suite_sha256"] == expected.suite_sha,
            "suite sha256 differs from the gate suite".into(),
        ),
    ];
    let problems: Vec<String> = rules
        .into_iter()
        .filter(|(ok, _)| !ok)
        .map(|(_, note)| note)
        .collect();
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("; "))
    }
}
