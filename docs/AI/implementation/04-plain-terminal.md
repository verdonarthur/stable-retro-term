# Plain graphical terminal

Status: pending. Planning only; implementation has not started.
Dependencies: 03 completed.

## Context and shared rules

Read [the architecture](../architecture/retro-terminal.md) and [the execution index](../README.md) before work.
Linux first. Reuse alacritty_terminal. Three product crates plus development-only xtask.
CPU, RAM, and GPU must each stay within 1.20 times Alacritty for every shipped preset under matched tests.
Static CRT is default; animation is opt-in but gets no budget exception.
Use a task worktree and the agreed base. Do not commit, push, or open a PR unless asked.
Use project tooling only. Report skipped/unavailable checks; never invent a passing result.
Record progress and handoff here under docs/AI/implementation/. This plan overrides the old plan-directory convention.

## Goal

Connect one shell to a correct, event-driven graphical terminal with CRT effects disabled.

## Files

- crates/renderer/src/{lib,scene,text}.rs.
- crates/app/src/{app,platform,scheduler}.rs and entry points.
- Renderer correctness tests, scheduler tests, deterministic font fixtures.
- xtask run command and docs/development.md.

## Steps

1. Prototype compatible winit/wgpu/glyphon versions, sharing one GPU device and glyph cache. Keep the graphics stack provisional until 05.
2. Translate terminal snapshots into renderer-owned scene data. Preserve cell widths, wide glyphs, combining marks, fallback fonts, and box drawing.
3. Implement window input, shortcuts, selection, clipboard, input methods, display scaling, scrollback, and coordinated terminal/PTY resizing.
4. Schedule frames only for actual invalidation or visual timers. Coalesce wakeups, release terminal locks before rendering, and recover correctly from surface/size changes.
5. Add cargo xtask run with explicit development configuration and local logs. Avoid personal shell-output logging by default.
6. Add deterministic scheduler tests and explicitly selected GPU render tests; document and run the project's GPU-test command on a suitable display machine.
7. Exercise interactive and full-screen programs and record backend/hardware and any missing coverage.

## Acceptance criteria

- One interactive shell renders correctly, resizes, and closes cleanly.
- Unchanged non-blinking idle does not request a continuous frame loop.
- Correctness tests and unsupported GPU conditions are reported explicitly.
- The stack is not declared to meet the resource budget before 05.

## Verification

Commands below are to run after this task and its dependencies supply the tooling.
Do not claim that planned commands already exist.

```sh
cargo xtask check
cargo build --locked --release -p retro-app --bin retro-term
cargo xtask run
```

The run command is an interactive smoke check, not a performance result. No CRT effects, tabs, or settings editor.

## Handoff

Record base SHA, worktree path, changed files, decisions, exact commands/results, unrun checks, artifact locations, and blockers.
Update this plan and the index status only when supported by evidence. Do not mark performance-dependent work complete on headless checks alone.

