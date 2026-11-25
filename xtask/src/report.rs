//! Step results with human and JSON output.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;

use crate::Failure;
use crate::process::io_failure;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Passed,
    Failed,
    /// A required tool is not installed.
    Missing,
    /// The step does not apply (filtered out or nothing to check).
    Skipped,
    /// Tests that exist but did not run here, for example tests that need a display.
    NotRun,
}

#[derive(Debug, Serialize)]
pub struct Step {
    pub name: String,
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl Step {
    pub fn new(name: &str, status: Status) -> Self {
        Self {
            name: name.to_owned(),
            status,
            exit_code: None,
            log: None,
            note: None,
        }
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub command: String,
    pub exit_code: u8,
    pub steps: Vec<Step>,
}

impl Report {
    /// Builds a report. Missing tools are an error unless `allow_missing` is set.
    pub fn new(command: &str, steps: Vec<Step>, allow_missing: bool) -> Self {
        let exit_code = exit_code(&steps, allow_missing);
        Self {
            command: command.to_owned(),
            exit_code,
            steps,
        }
    }

    /// Prints the report, writes `<dir>/<command>.json`, and returns the exit code.
    pub fn finish(&self, dir: &Path, json: bool) -> Result<ExitCode, Failure> {
        let text = serde_json::to_string_pretty(self).expect("report serializes");
        fs::create_dir_all(dir).map_err(|e| io_failure(dir, &e))?;
        let file = dir.join(format!("{}.json", self.command));
        fs::write(&file, &text).map_err(|e| io_failure(&file, &e))?;
        if json {
            println!("{text}");
        } else {
            self.print_human(&file);
        }
        Ok(ExitCode::from(self.exit_code))
    }

    fn print_human(&self, summary: &Path) {
        for step in &self.steps {
            let status = serde_json::to_value(step.status).expect("status serializes");
            let mut line = format!("{:<12} {}", step.name, status.as_str().unwrap_or("?"));
            if let Some(note) = &step.note {
                line.push_str(&format!(" ({note})"));
            }
            if step.status == Status::Failed
                && let Some(log) = &step.log
            {
                line.push_str(&format!("\n             log: {}", log.display()));
                if let Ok(content) = fs::read_to_string(log) {
                    eprintln!("--- {} ---\n{content}", step.name);
                }
            }
            println!("{line}");
        }
        println!("summary: {}", summary.display());
    }
}

fn exit_code(steps: &[Step], allow_missing: bool) -> u8 {
    let has = |status| steps.iter().any(|step| step.status == status);
    if has(Status::Failed) {
        Failure::FAILED
    } else if has(Status::Missing) && !allow_missing {
        Failure::MISSING_TOOL
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps(statuses: &[Status]) -> Vec<Step> {
        statuses.iter().map(|s| Step::new("x", *s)).collect()
    }

    #[test]
    fn failure_wins_over_missing_tool() {
        let report = Report::new("check", steps(&[Status::Missing, Status::Failed]), false);
        assert_eq!(report.exit_code, 1);
    }

    #[test]
    fn missing_tool_exits_3_unless_allowed() {
        assert_eq!(
            Report::new("c", steps(&[Status::Missing]), false).exit_code,
            3
        );
        assert_eq!(
            Report::new("c", steps(&[Status::Missing]), true).exit_code,
            0
        );
    }

    #[test]
    fn skipped_and_not_run_do_not_fail() {
        let report = Report::new(
            "c",
            steps(&[Status::Passed, Status::Skipped, Status::NotRun]),
            false,
        );
        assert_eq!(report.exit_code, 0);
    }

    #[test]
    fn json_shape_is_stable() {
        let mut failed = Step::new("fmt", Status::Failed);
        failed.exit_code = Some(1);
        let report = Report::new(
            "check",
            vec![failed, Step::new("deny", Status::NotRun)],
            false,
        );
        let value = serde_json::to_value(&report).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "command": "check",
                "exit_code": 1,
                "steps": [
                    {"name": "fmt", "status": "failed", "exit_code": 1},
                    {"name": "deny", "status": "not_run"}
                ]
            })
        );
    }
}
