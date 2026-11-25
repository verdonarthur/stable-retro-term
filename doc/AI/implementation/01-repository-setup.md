# Repository setup

Status: pending. Planning only; implementation has not started.
Dependencies: None. Git is initialized on main, but currently has no commits.

## Context and shared rules

Read [the architecture](../architecture/retro-terminal.md) and [the execution index](../README.md) before work.
Linux first. Reuse alacritty_terminal. Three product crates plus development-only xtask.
CPU, RAM, and GPU must each stay within 1.20 times Alacritty for every shipped preset under matched tests.
Static CRT is default; animation is opt-in but gets no budget exception.
Use a task worktree and the agreed base. Do not commit, push, or open a PR unless asked.
Use project tooling only. Report skipped/unavailable checks; never invent a passing result.
Record progress and handoff here under doc/AI/implementation/. This plan overrides the old plan-directory convention.

## Goal

Create a small, buildable Rust workspace and the shared repository instructions. Do not implement the terminal or workflow automation in this task.

## Files

- Root Cargo.toml, Cargo.lock, rust-toolchain.toml, .gitignore, .editorconfig.
- crates/terminal/Cargo.toml and src/lib.rs (retro-terminal).
- crates/renderer/Cargo.toml and src/lib.rs (retro-renderer).
- crates/app/Cargo.toml, src/main.rs and src/lib.rs (retro-app; binary retro-term).
- xtask/Cargo.toml and src/main.rs (development-only command skeleton).
- .cargo/config.toml, AGENTS.md, README.md, CONTRIBUTING.md.
- docs/architecture.md and docs/development.md, initially concise.

## Steps

1. Recheck Git status, branch, log, applicable instructions, and existing files. Preserve any user changes.
2. Resolve the initial-base blocker before branch work. An unborn repository cannot supply the worktree base. Ask for an existing base or explicit initial-commit authorization when this task is executed. Do not assume that permission to implement repository setup allows a commit.
3. Once a base exists, create the task branch in a worktree using Git directly; the xtask worktree helper does not exist yet. Record the base SHA and worktree path. Copy this plan, the architecture reference, and index into matching doc/AI/ paths without overwriting newer notes.
4. Pin an available supported Rust toolchain and set the workspace edition, minimum Rust version, shared lints, and version inheritance. Create the three product crates plus a small xtask skeleton. Keep product dependencies empty until needed. Mark packages unpublished on crates.io.
5. Set up the cargo xtask alias and a useful help response. Do not advertise doctor/check/worktree commands as implemented yet.
6. Ignore target outputs, .worktrees/, and private runtime data. Preserve the local doc/AI ignore rule and do not globally ignore all documentation. Generate Cargo.lock through Cargo.
7. Write portable AGENTS.md instructions: crate boundaries, project checks, doc/AI/<category> plan convention, no unasked commit/push/PR, and worktree use. Personal RTK wrapping stays an environment convention, not a CI dependency.
8. Document current setup and future milestones without claiming the app or CI exists. Record the first successful build and hand off to task 02.

## Acceptance criteria

- All four packages load and compile with the pinned toolchain and lockfile.
- Product crate boundaries and application binary name match the architecture.
- Root instructions and contribution docs let a fresh session run the real checks.
- No GUI, terminal parser, release workflow, or placeholder tests are added.
- Worktree/base details are recorded; no unapproved commit or remote operation occurs.

## Verification

Commands below are to run after this task and its dependencies supply the tooling.
Do not claim that planned commands already exist.

```sh
rtk cargo metadata --locked --no-deps --format-version 1
rtk cargo fmt --all -- --check
rtk cargo check --locked --workspace --all-targets
rtk cargo clippy --locked --workspace --all-targets -- -D warnings
rtk git check-ignore -v doc/AI/implementation/01-repository-setup.md
```

Do not write tests that merely check empty scaffolding. CI and reusable development tasks belong to 02. An unavailable base blocks implementation, not the preparation of this plan.

## Handoff

Record base SHA, worktree path, changed files, decisions, exact commands/results, unrun checks, artifact locations, and blockers.
Update this plan and the index status only when supported by evidence. Do not mark performance-dependent work complete on headless checks alone.

