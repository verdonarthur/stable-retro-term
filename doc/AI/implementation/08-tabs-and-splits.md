# Native tabs and splits

Status: pending. Planning only; implementation has not started.
Dependencies: 07 completed.

## Context and shared rules

Read [the architecture](../architecture/retro-terminal.md) and [the execution index](../README.md) before work.
Linux first. Reuse alacritty_terminal. Three product crates plus development-only xtask.
CPU, RAM, and GPU must each stay within 1.20 times Alacritty for every shipped preset under matched tests.
Static CRT is default; animation is opt-in but gets no budget exception.
Use a task worktree and the agreed base. Do not commit, push, or open a PR unless asked.
Use project tooling only. Report skipped/unavailable checks; never invent a passing result.
Record progress and handoff here under doc/AI/implementation/. This plan overrides the old plan-directory convention.

## Goal

Support nested splits and tabs with independent shells and shared rendering resources.

## Files

- crates/app/src/{tabs,layout,app,ui}.rs and tests/layout.rs.
- Session lifecycle integration and scene assembly.
- Multi-session performance fixtures and docs.

## Steps

1. Add stable TabId/PaneId/SessionId ownership. Each tab owns a split tree; each leaf identifies a pane/session.
2. Implement nested horizontal/vertical splits, focus, tab switching, minimum dimensions, and split resizing.
3. Route input to the focused pane and send matching dimensions to each resized terminal/PTY.
4. Keep hidden sessions parsing output but skip their text/GPU preparation; invalidate fully when revealed.
5. Collapse layouts and free session/render resources when panes close.
6. Test focus/geometry, nested close/resize, hidden output, and repeated open/close.
7. Extend the benchmark to matched multiple Alacritty sessions with equivalent visible cell area and visibility. Record comparison topology and recheck single-pane overhead.

## Acceptance criteria

- Tabs/splits work without an external multiplexer.
- Hidden tabs do no rendering; no extra graphics device exists per pane.
- Lifecycle and resource limits hold for the matched multi-session scenarios.

## Verification

Commands below are to run after this task and its dependencies supply the tooling.
Do not claim that planned commands already exist.

```sh
rtk cargo xtask check
rtk cargo build --locked --release -p retro-app --bin retro-term
rtk cargo xtask perf --candidate target/release/retro-term --baseline alacritty --suite benchmarks/scenarios.toml --output target/perf/report.json
```

No daemon or persistent session restore.

## Handoff

Record base SHA, worktree path, changed files, decisions, exact commands/results, unrun checks, artifact locations, and blockers.
Update this plan and the index status only when supported by evidence. Do not mark performance-dependent work complete on headless checks alone.

