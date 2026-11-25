# Settings UI and documentation polish

Status: pending. Planning only; implementation has not started.
Dependencies: 10 completed; higher-priority functionality and resource gates pass.

## Context and shared rules

Read [the architecture](../architecture/retro-terminal.md) and [the execution index](../README.md) before work.
Linux first. Reuse alacritty_terminal. Three product crates plus development-only xtask.
CPU, RAM, and GPU must each stay within 1.20 times Alacritty for every shipped preset under matched tests.
Static CRT is default; animation is opt-in but gets no budget exception.
Use a task worktree and the agreed base. Do not commit, push, or open a PR unless asked.
Use project tooling only. Report skipped/unavailable checks; never invent a passing result.
Record progress and handoff here under docs/AI/implementation/. This plan overrides the old plan-directory convention.

## Goal

Optional low-priority follow-up: add convenient settings controls and complete user-facing documentation.

## Files

- crates/app/src/ui.rs and config integration.
- Configuration persistence tests and performance scenarios.
- README.md and user/development/release documentation.

## Steps

1. Select a small UI approach that shares the existing graphics device; measure any added toolkit cost.
2. Reuse the typed configuration model. Preserve comments and unrelated user values when writing TOML, using atomic replacement.
3. Test invalid values, persistence failure, reload interactions, and no-op UI idle.
4. Document supported platforms/presets, exact performance evidence, shortcuts, configuration, and known limits.
5. Rerun every affected correctness check and the release resource suite. Keep macOS/Windows ports separate future tasks.

## Acceptance criteria

- Settings controls do not create a second configuration model or GPU context.
- File updates preserve user content and fail cleanly.
- All shipped configurations still meet the resource gate; unsupported platforms are not advertised.

## Verification

Commands below are to run after this task and its dependencies supply the tooling.
Do not claim that planned commands already exist.

```sh
cargo xtask check
cargo xtask package --check
```

This task is lower priority and can remain pending without blocking the initial Linux release. Recheck packaged-binary performance before any release.

## Handoff

Record base SHA, worktree path, changed files, decisions, exact commands/results, unrun checks, artifact locations, and blockers.
Update this plan and the index status only when supported by evidence. Do not mark performance-dependent work complete on headless checks alone.

