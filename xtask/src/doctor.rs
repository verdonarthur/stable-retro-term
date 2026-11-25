//! `cargo xtask doctor`: report missing or wrongly pinned tools. Installs nothing.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::process::{artifacts_dir, find_in_path, io_failure, output, workspace_root};
use crate::report::{Report, Status, Step};
use crate::{Failure, Outcome};

#[derive(clap::Args)]
pub struct Args {
    /// Print the JSON summary on stdout.
    #[arg(long)]
    json: bool,
}

#[derive(Deserialize)]
struct ToolsFile {
    tools: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct ToolchainFile {
    toolchain: Toolchain,
}

#[derive(Deserialize)]
struct Toolchain {
    channel: String,
}

/// Pinned tool versions from `tools.toml`.
pub fn pinned_tools(root: &Path) -> Result<BTreeMap<String, String>, Failure> {
    let file: ToolsFile = read_toml(&root.join("tools.toml"))?;
    Ok(file.tools)
}

pub fn run(args: &Args) -> Outcome {
    let root = workspace_root()?;
    let mut steps = vec![required("git"), required("rustup")];
    steps.push(toolchain_step(&root)?);
    for (tool, version) in pinned_tools(&root)? {
        steps.push(tool_step(&tool, &version, &root));
    }
    Report::new("doctor", steps, false).finish(&artifacts_dir(&root), args.json)
}

fn required(program: &str) -> Step {
    match find_in_path(program) {
        Some(path) => Step::new(program, Status::Passed).with_note(path.display().to_string()),
        None => Step::new(program, Status::Missing).with_note(format!("install `{program}`")),
    }
}

fn toolchain_step(root: &Path) -> Result<Step, Failure> {
    let file: ToolchainFile = read_toml(&root.join("rust-toolchain.toml"))?;
    let channel = file.toolchain.channel;
    if find_in_path("rustc").is_none() {
        return Ok(Step::new("toolchain", Status::Missing)
            .with_note(format!("install rustup; it installs Rust {channel}")));
    }
    Ok(match output("rustc", &["--version"], root) {
        Ok(version) if has_version(&version, &channel) => {
            Step::new("toolchain", Status::Passed).with_note(version)
        }
        Ok(version) => Step::new("toolchain", Status::Failed)
            .with_note(format!("found `{version}`, expected {channel}")),
        Err(failure) => Step::new("toolchain", Status::Failed).with_note(failure.message),
    })
}

fn tool_step(tool: &str, pinned: &str, root: &Path) -> Step {
    if find_in_path(tool).is_none() {
        return Step::new(tool, Status::Missing).with_note(install_hint(tool, pinned));
    }
    match output(tool, &["--version"], root) {
        Ok(version) if has_version(&version, pinned) => Step::new(tool, Status::Passed),
        Ok(version) => Step::new(tool, Status::Failed).with_note(format!(
            "found `{}`, pinned {pinned}: {}",
            version.lines().next().unwrap_or_default(),
            install_hint(tool, pinned)
        )),
        Err(failure) => Step::new(tool, Status::Failed).with_note(failure.message),
    }
}

/// True when `text` contains `version` as a whole word (an optional `v` prefix is allowed).
fn has_version(text: &str, version: &str) -> bool {
    text.split(|c: char| c.is_whitespace() || c == ',')
        .any(|word| word.trim_start_matches('v') == version)
}

fn install_hint(tool: &str, version: &str) -> String {
    match tool {
        "actionlint" => {
            format!("go install github.com/rhysd/actionlint/cmd/actionlint@v{version}")
        }
        "dist" => format!("cargo install --locked cargo-dist@{version}"),
        _ => format!("cargo install --locked {tool}@{version}"),
    }
}

fn read_toml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, Failure> {
    let text = fs::read_to_string(path).map_err(|e| io_failure(path, &e))?;
    toml::from_str(&text).map_err(|e| Failure::refused(format!("{}: {e}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_whole_words_only() {
        assert!(has_version("cargo-deny 0.22.1", "0.22.1"));
        assert!(has_version("v1.7.12\ninstalled by go", "1.7.12"));
        assert!(!has_version("zizmor 1.33.10", "1.33.1"));
    }

    #[test]
    fn dist_hint_installs_the_cargo_dist_crate() {
        assert_eq!(
            install_hint("dist", "0.34.0"),
            "cargo install --locked cargo-dist@0.34.0"
        );
    }
}
