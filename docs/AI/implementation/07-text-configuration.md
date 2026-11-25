# Text configuration and reload

Status: pending. Planning only; implementation has not started.
Dependencies: 06 completed.

## Context and shared rules

Read [the architecture](../architecture/retro-terminal.md) and [the execution index](../README.md) before work.
Linux first. Reuse alacritty_terminal. Three product crates plus development-only xtask.
CPU, RAM, and GPU must each stay within 1.20 times Alacritty for every shipped preset under matched tests.
Static CRT is default; animation is opt-in but gets no budget exception.
Use a task worktree and the agreed base. Do not commit, push, or open a PR unless asked.
Use project tooling only. Report skipped/unavailable checks; never invent a passing result.
Record progress and handoff here under docs/AI/implementation/. This plan overrides the old plan-directory convention.

## Goal

Provide one typed TOML model for configuration, presets, runtime settings, and the future editor.

## Files

- crates/app/src/config.rs and tests/config.rs.
- Example TOML, assets/presets/, user configuration documentation.
- App reload/error display and layout integration.

## Steps

1. Define defaults -> selected preset -> explicit user values as precedence. Resolve Linux user paths through the platform boundary.
2. Validate unknown keys, types, and supported ranges; document meaningful errors.
3. Add an explicit reload action. Validate the entire candidate before applying it; retain the last valid configuration on failure.
4. Apply font/layout changes and PTY resizing together. Distinguish existing-session changes from new-shell settings.
5. Test precedence, invalid input, atomic reload behavior, and layout changes. Keep supported effect ranges compatible with performance claims.
6. Run the existing checks and performance suite for changed rendering settings.

## Acceptance criteria

- Text configuration is sufficient to use the app and select supported presets.
- Invalid reloads do not break running sessions.
- Future UI can reuse the same model without a second source of settings truth.

## Verification

Commands below are to run after this task and its dependencies supply the tooling.
Do not claim that planned commands already exist.

```sh
cargo xtask check
cargo build --locked --release -p retro-app --bin retro-term
cargo xtask perf --candidate target/release/retro-term --baseline alacritty --suite benchmarks/scenarios.toml --output target/perf/report.json
```

No file watcher or graphical settings editor in this task.

## Handoff

Record base SHA, worktree path, changed files, decisions, exact commands/results, unrun checks, artifact locations, and blockers.
Update this plan and the index status only when supported by evidence. Do not mark performance-dependent work complete on headless checks alone.

