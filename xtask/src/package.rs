//! `cargo xtask package --check`: build the release archive with `dist` and check it. Never publishes.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::process::{artifacts_dir, find_in_path, io_failure, output, run_logged, workspace_root};
use crate::release::{tag_version, verify_steps, workspace_version};
use crate::report::{Report, Status, Step};
use crate::{Failure, Outcome};

pub const TARGET: &str = "x86_64-unknown-linux-gnu";
const ARCHIVE: &str = "retro-app-x86_64-unknown-linux-gnu.tar.xz";
/// Files the build must not change.
const MANIFESTS: [&str; 2] = ["Cargo.toml", "Cargo.lock"];
const BINARY: &str = "retro-term";
const DOCUMENTS: [&str; 3] = ["LICENSE", "README.md", "CHANGELOG.md"];

/// Build the release archive with the pinned `dist` and check it. xtask never publishes:
/// it never tags, uploads, or publishes a release.
#[derive(clap::Args)]
pub struct Args {
    /// Required: build and check only.
    #[arg(long)]
    check: bool,
    /// Release tag (vX.Y.Z). Also verifies the tag, versions, and changelog.
    #[arg(long, value_name = "TAG")]
    tag: Option<String>,
    /// Commit the tag must point to. Defaults to HEAD.
    #[arg(long, requires = "tag", value_name = "SHA")]
    source_sha: Option<String>,
    /// Print the JSON summary on stdout.
    #[arg(long)]
    json: bool,
}

/// What was packaged. Paths are relative to the workspace root.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub schema: u8,
    pub version: String,
    pub tag: Option<String>,
    pub source_sha: String,
    pub dirty: bool,
    pub rustc: String,
    pub dist: String,
    pub target: String,
    pub archive: Archive,
    pub binary: Binary,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Archive {
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binary {
    pub path: PathBuf,
    /// Path of the binary inside the archive.
    pub member: String,
    pub sha256: String,
}

impl Identity {
    pub fn read(path: &Path) -> Result<Self, Failure> {
        let text = fs::read_to_string(path).map_err(|e| io_failure(path, &e))?;
        serde_json::from_str(&text).map_err(|e| Failure::usage(format!("{}: {e}", path.display())))
    }
}

pub fn sha256_file(path: &Path) -> Result<String, Failure> {
    let bytes = fs::read(path).map_err(|e| io_failure(path, &e))?;
    Ok(Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

pub fn run(args: &Args) -> Outcome {
    if !args.check {
        return Err(Failure::usage("xtask never publishes; use --check"));
    }
    if let Some(tag) = &args.tag {
        tag_version(tag)?;
    }
    let root = workspace_root()?;
    let steps = package_steps(args, &root)?;
    Report::new("package", steps, false).finish(&artifacts_dir(&root), args.json)
}

/// Runs the steps in order and stops at the first one that does not pass.
fn package_steps(args: &Args, root: &Path) -> Result<Vec<Step>, Failure> {
    let mut steps = Vec::new();
    let Some(_) = find_in_path("dist") else {
        steps.push(
            Step::new("dist", Status::Missing)
                .with_note("`dist` not found; run `cargo xtask doctor`"),
        );
        return Ok(steps);
    };
    let dist = output("dist", &["--version"], root)?;
    steps.push(Step::new("dist", Status::Passed).with_note(dist.clone()));

    if !passed(
        &mut steps,
        command_step("lock", "cargo", &["fetch", "--locked"], root)?,
    ) {
        return Ok(steps);
    }
    let head = output("git", &["rev-parse", "HEAD"], root)?;
    if let Some(tag) = &args.tag {
        let sha = args.source_sha.as_deref().unwrap_or(&head);
        let failed: Vec<String> = verify_steps(root, tag, sha)?
            .into_iter()
            .filter(|step| step.status != Status::Passed)
            .map(|step| format!("{}: {}", step.name, step.note.unwrap_or_default()))
            .collect();
        let step = if failed.is_empty() {
            Step::new("identity", Status::Passed)
        } else {
            Step::new("identity", Status::Failed).with_note(failed.join("; "))
        };
        if !passed(&mut steps, step) {
            return Ok(steps);
        }
    }

    let manifests = MANIFESTS
        .iter()
        .map(|file| fs::read(root.join(file)).map_err(|e| io_failure(&root.join(file), &e)))
        .collect::<Result<Vec<_>, _>>()?;
    let distrib = root.join("target/distrib");
    if distrib.exists() {
        fs::remove_dir_all(&distrib).map_err(|e| io_failure(&distrib, &e))?;
    }
    let mut build = vec!["build", "--artifacts=local", "--target", TARGET];
    if let Some(tag) = &args.tag {
        build.extend(["--tag", tag]);
    }
    if !passed(&mut steps, command_step("build", "dist", &build, root)?) {
        return Ok(steps);
    }

    let changed: Vec<&str> = MANIFESTS
        .iter()
        .zip(&manifests)
        .filter(|(file, before)| fs::read(root.join(file)).ok().as_ref() != Some(*before))
        .map(|(file, _)| *file)
        .collect();
    let unchanged = if changed.is_empty() {
        Step::new("unchanged", Status::Passed)
    } else {
        Step::new("unchanged", Status::Failed).with_note(format!("build changed {changed:?}"))
    };
    if !passed(&mut steps, unchanged) {
        return Ok(steps);
    }

    let extract = artifacts_dir(root).join("package/extract");
    let (layout, binary) = match layout(root, &distrib, &extract) {
        Ok(found) => (Step::new("layout", Status::Passed), Some(found)),
        Err(note) => (Step::new("layout", Status::Failed).with_note(note), None),
    };
    let Some((archive_sha, member)) = binary.filter(|_| passed(&mut steps, layout)) else {
        return Ok(steps);
    };

    let binary_path = extract.join(&member);
    let smoke = match output(&binary_path.to_string_lossy(), &["--help"], root) {
        Ok(text) if text.starts_with("Usage: retro-term") => Step::new("smoke", Status::Passed),
        Ok(text) => Step::new("smoke", Status::Failed)
            .with_note(format!("unexpected --help output: {text}")),
        Err(failure) => Step::new("smoke", Status::Failed).with_note(failure.message),
    };
    if !passed(&mut steps, smoke) {
        return Ok(steps);
    }

    let identity = Identity {
        schema: 1,
        version: workspace_version(root)?,
        tag: args.tag.clone(),
        source_sha: head,
        dirty: !output("git", &["status", "--porcelain"], root)?.is_empty(),
        rustc: output("rustc", &["-V"], root)?,
        dist,
        target: TARGET.to_owned(),
        archive: Archive {
            path: relative(root, &distrib.join(ARCHIVE)),
            sha256: archive_sha,
        },
        binary: Binary {
            path: relative(root, &binary_path),
            sha256: sha256_file(&binary_path)?,
            member,
        },
    };
    let file = artifacts_dir(root).join("package/identity.json");
    let text = serde_json::to_string_pretty(&identity).expect("identity serializes");
    fs::write(&file, text).map_err(|e| io_failure(&file, &e))?;
    steps.push(Step::new("identity-file", Status::Passed).with_note(file.display().to_string()));
    Ok(steps)
}

fn passed(steps: &mut Vec<Step>, step: Step) -> bool {
    let ok = step.status == Status::Passed;
    steps.push(step);
    ok
}

fn command_step(name: &str, program: &str, args: &[&str], root: &Path) -> Result<Step, Failure> {
    let log = artifacts_dir(root)
        .join("logs")
        .join(format!("package-{name}.log"));
    let code = run_logged(program, args, root, &log)?;
    let status = if code == Some(0) {
        Status::Passed
    } else {
        Status::Failed
    };
    let mut step = Step::new(name, status);
    step.exit_code = code;
    step.log = Some(log);
    Ok(step)
}

/// Checks the archive, its checksum, and its entries, then extracts it.
/// Returns the archive sha256 and the binary's path inside the archive.
fn layout(root: &Path, distrib: &Path, extract: &Path) -> Result<(String, String), String> {
    let archives: Vec<String> = fs::read_dir(distrib)
        .map_err(|e| format!("{}: {e}", distrib.display()))?
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.ends_with(".tar.xz"))
        .collect();
    if archives != [ARCHIVE] {
        return Err(format!("expected only {ARCHIVE}, found {archives:?}"));
    }
    let archive = distrib.join(ARCHIVE);
    let checksum = distrib.join(format!("{ARCHIVE}.sha256"));
    let expected =
        fs::read_to_string(&checksum).map_err(|e| format!("{}: {e}", checksum.display()))?;
    let actual = sha256_file(&archive).map_err(|f| f.message)?;
    if expected.split_whitespace().next() != Some(actual.as_str()) {
        return Err(format!(
            "{ARCHIVE}.sha256 does not match the archive ({actual})"
        ));
    }

    let archive_arg = archive.to_string_lossy();
    let verbose = output("tar", &["-tvJf", &archive_arg], root).map_err(|f| f.message)?;
    if let Some(line) = verbose
        .lines()
        .find(|line| !line.starts_with('-') && !line.starts_with('d'))
    {
        return Err(format!("archive entry is not a file or directory: {line}"));
    }
    let names = output("tar", &["-tJf", &archive_arg], root).map_err(|f| f.message)?;
    let names: Vec<&str> = names.lines().collect();
    if let Some(name) = names
        .iter()
        .find(|name| name.starts_with('/') || name.split('/').any(|part| part == ".."))
    {
        return Err(format!("unsafe archive path: {name}"));
    }
    let binaries: Vec<&&str> = names
        .iter()
        .filter(|name| Path::new(name).file_name() == Some(BINARY.as_ref()))
        .collect();
    let [member] = binaries[..] else {
        return Err(format!(
            "expected one {BINARY} in the archive, found {binaries:?}"
        ));
    };
    let parent = Path::new(member).parent().unwrap_or(Path::new(""));
    for document in DOCUMENTS {
        let wanted = parent.join(document);
        if !names.iter().any(|name| Path::new(name) == wanted) {
            return Err(format!("archive has no {}", wanted.display()));
        }
    }

    if extract.exists() {
        fs::remove_dir_all(extract).map_err(|e| format!("{}: {e}", extract.display()))?;
    }
    fs::create_dir_all(extract).map_err(|e| format!("{}: {e}", extract.display()))?;
    let extract_arg = extract.to_string_lossy();
    output("tar", &["-xJf", &archive_arg, "-C", &extract_arg], root).map_err(|f| f.message)?;
    let mode = fs::metadata(extract.join(member))
        .map_err(|e| format!("{member}: {e}"))?
        .permissions()
        .mode();
    if mode & 0o111 == 0 {
        return Err(format!("{member} is not executable"));
    }
    Ok((actual, (*member).to_owned()))
}

fn relative(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}
