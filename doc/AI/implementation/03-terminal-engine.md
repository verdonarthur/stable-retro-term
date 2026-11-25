# Terminal engine and PTY sessions

Status: pending. Planning only; implementation has not started.
Dependencies: 02 completed.

## Context and shared rules

Read [the architecture](../architecture/retro-terminal.md) and [the execution index](../README.md) before work.
Linux first. Reuse alacritty_terminal. Three product crates plus development-only xtask.
CPU, RAM, and GPU must each stay within 1.20 times Alacritty for every shipped preset under matched tests.
Static CRT is default; animation is opt-in but gets no budget exception.
Use a task worktree and the agreed base. Do not commit, push, or open a PR unless asked.
Use project tooling only. Report skipped/unavailable checks; never invent a passing result.
Record progress and handoff here under doc/AI/implementation/. This plan overrides the old plan-directory convention.

## Goal

Wrap alacritty_terminal behind a window-independent session API.

## Files

- crates/terminal/src/{lib,session,engine,input,snapshot}.rs.
- crates/terminal/tests/{terminal,session}.rs and deterministic fixtures.
- Workspace dependency declarations and Cargo.lock.

## Steps

1. Select a compatible published alacritty_terminal version and reuse its PTY/event-loop machinery.
2. Define stable session IDs, visible snapshots, events, and platform-neutral input. Keep winit/wgpu types outside the crate.
3. Handle shell lifecycle, input, terminal protocol replies, resize, selection, scrolling, child exit, and cleanup.
4. Use short state locks and coalesced notifications. Do not copy scrollback per output event or add unbounded application queues.
5. Test fixed escape-sequence outcomes, colors/cursor movement, alternate screen, Unicode widths, scrollback, resize, bracketed paste, and real PTY input/output/exit.
6. Document API ownership and errors for the app integration.

## Acceptance criteria

- Shell output produces correct terminal state and protocol replies.
- Input/resize reach the PTY; closing the session cleans up its worker and child.
- Tests run without a GPU/window; expected outcomes are independent of the implementation.

## Verification

Commands below are to run after this task and its dependencies supply the tooling.
Do not claim that planned commands already exist.

```sh
rtk cargo test --locked -p retro-terminal
rtk cargo xtask check
```

No new terminal parser, graphics code, tabs, or performance claims.

## Handoff

Record base SHA, worktree path, changed files, decisions, exact commands/results, unrun checks, artifact locations, and blockers.
Update this plan and the index status only when supported by evidence. Do not mark performance-dependent work complete on headless checks alone.

