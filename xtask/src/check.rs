//! `cargo xtask check`: the checks that CI runs.

use std::path::Path;

use clap::ValueEnum;

use crate::process::{artifacts_dir, find_in_path, output, run_logged, workspace_root};
use crate::report::{Report, Status, Step};
use crate::{Failure, Outcome};

#[derive(clap::Args)]
pub struct Args {
    /// Check only this package (repeatable). Workspace-wide steps are skipped.
    #[arg(long = "package", short = 'p', value_name = "NAME")]
    packages: Vec<String>,
    /// Run only this step (repeatable).
    #[arg(long = "step", value_enum, conflicts_with = "skip")]
    steps: Vec<StepName>,
    /// Do not run this step (repeatable).
    #[arg(long, value_enum)]
    skip: Vec<StepName>,
    /// Report missing tools as skipped and do not fail because of them.
    #[arg(long)]
    allow_missing: bool,
    /// Print the JSON summary on stdout.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum StepName {
    Fmt,
    Check,
    Clippy,
    Test,
    Deny,
    Actionlint,
    Zizmor,
}

impl StepName {
    fn as_str(self) -> &'static str {
        match self {
            Self::Fmt => "fmt",
            Self::Check => "check",
            Self::Clippy => "clippy",
            Self::Test => "test",
            Self::Deny => "deny",
            Self::Actionlint => "actionlint",
            Self::Zizmor => "zizmor",
        }
    }

    /// Steps that check the whole repository, not single packages.
    fn is_workspace_only(self) -> bool {
        matches!(
            self,
            Self::Fmt | Self::Deny | Self::Actionlint | Self::Zizmor
        )
    }
}

pub fn run(args: &Args) -> Outcome {
    let root = workspace_root()?;
    validate_packages(&root, &args.packages)?;
    let selection: Vec<String> = if args.packages.is_empty() {
        vec!["--workspace".to_owned()]
    } else {
        args.packages
            .iter()
            .map(|p| format!("--package={p}"))
            .collect()
    };
    let selection: Vec<&str> = selection.iter().map(String::as_str).collect();

    let mut steps = Vec::new();
    for name in StepName::value_variants().iter().copied() {
        let step = if !is_selected(args, name) {
            Step::new(name.as_str(), Status::Skipped).with_note("not selected")
        } else if name.is_workspace_only() && !args.packages.is_empty() {
            Step::new(name.as_str(), Status::Skipped).with_note("workspace-wide step")
        } else {
            run_step(name, &root, &selection)?
        };
        let test_log = (name == StepName::Test && step.status != Status::Skipped)
            .then(|| step.log.clone())
            .flatten();
        steps.push(step);
        if let Some(log) = test_log {
            steps.extend(ignored_tests_step(&log));
        }
    }
    Report::new("check", steps, args.allow_missing).finish(&artifacts_dir(&root), args.json)
}

fn is_selected(args: &Args, name: StepName) -> bool {
    (args.steps.is_empty() || args.steps.contains(&name)) && !args.skip.contains(&name)
}

fn run_step(name: StepName, root: &Path, selection: &[&str]) -> Result<Step, Failure> {
    let (program, args): (&str, Vec<&str>) = match name {
        StepName::Fmt => ("cargo", vec!["fmt", "--all", "--", "--check"]),
        StepName::Check => (
            "cargo",
            [&["check", "--locked", "--all-targets"], selection].concat(),
        ),
        StepName::Clippy => (
            "cargo",
            [
                &["clippy", "--locked", "--all-targets"],
                selection,
                &["--", "-D", "warnings"],
            ]
            .concat(),
        ),
        StepName::Test => ("cargo", [&["test", "--locked"], selection].concat()),
        StepName::Deny => ("cargo-deny", vec!["--locked", "check"]),
        StepName::Actionlint => ("actionlint", vec![]),
        StepName::Zizmor => ("zizmor", vec!["--offline", ".github/workflows"]),
    };
    let label = name.as_str();
    if matches!(name, StepName::Actionlint | StepName::Zizmor)
        && !root.join(".github/workflows").is_dir()
    {
        return Ok(Step::new(label, Status::Skipped).with_note("no .github/workflows"));
    }
    if find_in_path(program).is_none() {
        return Ok(Step::new(label, Status::Missing)
            .with_note(format!("`{program}` not found; run `cargo xtask doctor`")));
    }
    let log = artifacts_dir(root)
        .join("logs")
        .join(format!("{label}.log"));
    let code = run_logged(program, &args, root, &log)?;
    let mut step = Step::new(
        label,
        if code == Some(0) {
            Status::Passed
        } else {
            Status::Failed
        },
    );
    step.exit_code = code;
    step.log = Some(log);
    Ok(step)
}

/// Ignored tests (for example tests that need a display) are reported as not run, never as passed.
fn ignored_tests_step(log: &Path) -> Option<Step> {
    let text = std::fs::read_to_string(log).ok()?;
    let ignored: u32 = text
        .lines()
        .filter_map(|line| line.strip_prefix("test result: "))
        .filter_map(|line| {
            line.split(';')
                .find_map(|part| part.trim().strip_suffix(" ignored")?.parse::<u32>().ok())
        })
        .sum();
    (ignored > 0).then(|| {
        Step::new("ignored-tests", Status::NotRun)
            .with_note(format!("{ignored} ignored tests did not run"))
    })
}

fn validate_packages(root: &Path, packages: &[String]) -> Result<(), Failure> {
    if packages.is_empty() {
        return Ok(());
    }
    let metadata = output(
        "cargo",
        &["metadata", "--locked", "--no-deps", "--format-version", "1"],
        root,
    )?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata)
        .map_err(|e| Failure::refused(format!("cannot read cargo metadata: {e}")))?;
    let known: Vec<&str> = metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|package| package["name"].as_str())
        .collect();
    match packages.iter().find(|p| !known.contains(&p.as_str())) {
        Some(unknown) => Err(Failure::usage(format!(
            "unknown package `{unknown}`; workspace packages: {}",
            known.join(", ")
        ))),
        None => Ok(()),
    }
}
