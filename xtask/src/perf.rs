use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::process::{find_in_path, io_failure, output, workspace_root};
use crate::{Failure, Outcome};

const SCENARIOS: [&str; 8] = [
    "startup",
    "idle",
    "blink",
    "typing",
    "output",
    "redraw",
    "resize",
    "lifecycle",
];

#[derive(Parser)]
pub(crate) struct Args {
    #[arg(long)]
    candidate: PathBuf,
    /// Measure the candidate with its CRT effects bypassed.
    #[arg(long)]
    candidate_plain: bool,
    /// Measure the experimental animated CRT preset with the suite's animated_config.
    #[arg(long, conflicts_with = "candidate_plain")]
    candidate_animated: bool,
    #[arg(long)]
    baseline: PathBuf,
    #[arg(long)]
    suite: PathBuf,
    #[arg(long)]
    output: PathBuf,
    /// Check the host, tools and configurations before running timed workloads.
    #[arg(long)]
    preflight: bool,
    /// Run one scenario for diagnosis. A partial run cannot pass the gate.
    #[arg(long, value_parser = clap::builder::PossibleValuesParser::new(SCENARIOS), conflicts_with = "preflight")]
    scenario: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Suite {
    version: u8,
    baseline_version: String,
    alacritty_config: PathBuf,
    candidate_config: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    animated_config: Option<PathBuf>,
    repetitions: u8,
    sample_ms: u64,
    idle_ms: u64,
    timeout_ms: u64,
    scenarios: Vec<String>,
}

impl Suite {
    fn parse(text: &str) -> Result<Self, Failure> {
        let suite: Self = toml::from_str(text).map_err(|e| Failure::usage(e.to_string()))?;
        if suite.version != 1
            || suite.baseline_version.is_empty()
            || !(5..=30).contains(&suite.repetitions)
            || !(5..=100).contains(&suite.sample_ms)
            || !(2_000..=120_000).contains(&suite.idle_ms)
            || !(1_000..=30_000).contains(&suite.timeout_ms)
            || suite.scenarios.len() != SCENARIOS.len()
            || !SCENARIOS
                .iter()
                .all(|name| suite.scenarios.iter().any(|s| s == name))
        {
            return Err(Failure::usage(
                "invalid performance suite version, timing bounds, or scenario set",
            ));
        }
        Ok(suite)
    }
}

fn binary(path: &Path) -> Option<PathBuf> {
    let path = if path.components().count() == 1 && path.is_relative() {
        find_in_path(path.to_str()?)?
    } else {
        path.to_owned()
    };
    let path = path.canonicalize().ok()?;
    path.is_file().then_some(path)
}

fn finish(path: &Path, report: &Value, code: u8) -> Outcome {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|e| io_failure(parent, &e))?;
    }
    let body = serde_json::to_vec_pretty(report).map_err(|e| Failure::refused(e.to_string()))?;
    fs::write(path, body).map_err(|e| io_failure(path, &e))?;
    println!(
        "{}: {}",
        path.display(),
        report["status"].as_str().unwrap_or("unverified")
    );
    Ok(ExitCode::from(code))
}

pub(crate) fn run(args: &Args) -> Outcome {
    let root = workspace_root()?;
    let text = fs::read_to_string(&args.suite).map_err(|e| io_failure(&args.suite, &e))?;
    let suite = Suite::parse(&text)?;
    let config = args
        .suite
        .parent()
        .unwrap_or(Path::new("."))
        .join(&suite.alacritty_config);
    let config = config.canonicalize().map_err(|e| io_failure(&config, &e))?;
    let candidate_config = args
        .suite
        .parent()
        .unwrap_or(Path::new("."))
        .join(&suite.candidate_config);
    let candidate_config = candidate_config
        .canonicalize()
        .map_err(|e| io_failure(&candidate_config, &e))?;
    let animated_config = if args.candidate_animated {
        let path = suite.animated_config.as_ref().ok_or_else(|| {
            Failure::usage("--candidate-animated needs animated_config in the suite")
        })?;
        let path = args.suite.parent().unwrap_or(Path::new(".")).join(path);
        Some(path.canonicalize().map_err(|e| io_failure(&path, &e))?)
    } else {
        None
    };
    let candidate_mode = if args.candidate_plain {
        "plain"
    } else if args.candidate_animated {
        "animated-crt"
    } else {
        "static-crt"
    };
    let source = output("git", &["rev-parse", "HEAD"], &root)?;
    let dirty = !output("git", &["status", "--porcelain"], &root)?.is_empty();
    let candidate = binary(&args.candidate);
    let baseline = binary(&args.baseline);
    let mut report = json!({
        "schema": 1, "status": "unverified", "preflight": args.preflight, "scenario": args.scenario,
        "source": source, "dirty": dirty, "suite": suite,
        "candidate_mode": candidate_mode,
        "candidate": candidate, "baseline": baseline,
    });
    let python = find_in_path("python3");
    let mut missing = Vec::new();
    if candidate.is_none() {
        missing.push(format!(
            "candidate executable unavailable: {}",
            args.candidate.display()
        ));
    }
    if baseline.is_none() {
        missing.push(format!(
            "baseline executable unavailable: {}",
            args.baseline.display()
        ));
    }
    if python.is_none() {
        missing.push("python3 unavailable".into());
    }
    let (Some(candidate), Some(baseline), Some(python)) = (candidate, baseline, python) else {
        report["reasons"] = json!(missing);
        return finish(&args.output, &report, Failure::MISSING_TOOL);
    };
    let suite_path = args
        .suite
        .canonicalize()
        .map_err(|e| io_failure(&args.suite, &e))?;
    let request = json!({
        "suite": suite, "suite_path": suite_path, "config": config,
        "candidate_plain": args.candidate_plain, "candidate_config": candidate_config,
        "candidate_animated": args.candidate_animated, "animated_config": animated_config,
        "candidate": candidate, "baseline": baseline, "preflight": args.preflight, "scenario": args.scenario,
    });
    let driver = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/perf/run.py");
    let response = invoke(&python, &driver, &root, &request);
    match response {
        Ok(value) => {
            let status = value["status"].as_str().unwrap_or("unverified");
            let code = if status == "passed" {
                0
            } else {
                Failure::FAILED
            };
            report["status"] = json!(status);
            report["measurement"] = value;
            finish(&args.output, &report, code)
        }
        Err(error) => {
            report["reasons"] = json!([error.message]);
            finish(&args.output, &report, Failure::FAILED)
        }
    }
}

fn invoke(python: &Path, driver: &Path, root: &Path, request: &Value) -> Result<Value, Failure> {
    let mut child = Command::new(python)
        .arg(driver)
        .current_dir(root)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| io_failure(python, &e))?;
    let payload = serde_json::to_vec(request).map_err(|e| Failure::refused(e.to_string()))?;
    let written = child.stdin.take().expect("piped stdin").write_all(&payload);
    let result = child
        .wait_with_output()
        .map_err(|e| io_failure(driver, &e))?;
    written.map_err(|e| io_failure(driver, &e))?;
    if !result.status.success() {
        return Err(Failure::refused(format!(
            "performance driver exited {}",
            result.status
        )));
    }
    serde_json::from_slice(&result.stdout)
        .map_err(|e| Failure::refused(format!("invalid performance response: {e}")))
}
