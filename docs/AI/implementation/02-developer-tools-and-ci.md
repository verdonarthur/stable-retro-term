# Developer tools, worktrees, and CI

Status: pending. Planning done; implementation has not started.
Dependencies: 01 completed.

## Context and shared rules

Read [the architecture](../architecture/retro-terminal.md) and [the execution index](../README.md) before work.
Linux first. Reuse alacritty_terminal. Three product crates plus development-only xtask.
CPU, RAM, and GPU must each stay within 1.20 times Alacritty for every shipped preset under matched tests.
Static CRT is default; animation is opt-in but gets no budget exception.
Use a task worktree and the agreed base. Do not commit, push, or open a PR unless asked.
Use project tooling only. Report skipped/unavailable checks; never invent a passing result.
Record progress and handoff here under docs/AI/implementation/.

## Goal

Make local and GitHub checks identical and provide reliable worktree tools.

## Current state (base `89a4126`)

- `xtask/src/main.rs`: only `help`; unknown command exits 2. `xtask/Cargo.toml`: no dependencies.
- `.cargo/config.toml`: alias `xtask = "run --package xtask --"`.
- No `.github/`, no `deny.toml`, no Git remote.
- Not installed locally: `cargo-deny`, `actionlint`, `zizmor`.
- `docs/AI/` is tracked by Git (not ignored). Worktrees get plans through Git.

## Decisions (settled with the user)

- D1 Tools: pin auxiliary tool versions in one file, `tools.toml` at the repo root
  (`cargo-deny`, `actionlint`, `zizmor`). `doctor` reads it, reports missing or wrong versions,
  and prints the install command. It never installs anything. CI installs the same versions from this file.
- D2 Missing tools in `check`: the step is marked `SKIPPED` in human and JSON output.
  Exit is non-zero unless `--allow-missing` is given. CI never passes `--allow-missing`.
- D3 Dependencies: xtask uses `clap` (derive) for commands/help/exit code 2, `serde` + `serde_json` for
  JSON summaries, `tempfile` as a dev-dependency for tests. Nothing from the product crates or GPU stack.
- D4 No plan transfer: `docs/AI/` is tracked, so `worktree create` does not copy notes.
- D5 Each worktree keeps its own `target/`. No `CARGO_TARGET_DIR` override. Only `~/.cargo` registry/git caches are shared.
- D6 CI: `ubuntu-latest` only. Jobs with stable names `check` and `deny`. No GPU/display job;
  docs say that no display tests exist yet. Actions pinned by commit SHA, `Swatinem/rust-cache`,
  `timeout-minutes: 30`, `concurrency` with `cancel-in-progress` for PRs, `permissions: contents: read`.
- D7 Dependabot: weekly, grouped, ecosystems `cargo` and `github-actions`. No auto-merge.
- D8 Worktrees: path `.worktrees/<name>`, branch `<name>`, name matches `^[a-z0-9][a-z0-9-]*$`.
  `--base` is required. `remove` refuses when the worktree has uncommitted, untracked, or unpushed/unmerged
  work, unless `--force`. Even with `--force`, untracked/ignored files under `docs/AI/` are copied to
  `target/xtask/rescued/<name>-<timestamp>/` in the main checkout before removal.
- Remote: the repo will be on GitHub later. Until then, workflows are validated locally only;
  GitHub settings are recorded as pending (see step 9).

## Command contracts

```
cargo xtask doctor [--json]
cargo xtask check [--package <name>]... [--allow-missing] [--json]
cargo xtask worktree create <name> --base <rev>
cargo xtask worktree list [--json]
cargo xtask worktree remove <name> [--force]
```

Exit codes (all commands): `0` success, `1` a check/step failed or a refusal happened,
`2` usage error (clap), `3` missing tool without `--allow-missing`.
JSON summaries go to stdout with `--json` and are also written to `<worktree>/target/xtask/<command>.json`.
Step logs go to `<worktree>/target/xtask/logs/<step>.log`.

`check` steps, in order (each a process with an argument array, never a shell string):
1. `cargo fmt --all -- --check`
2. `cargo check --locked --workspace --all-targets` (or `-p <name>` per package)
3. `cargo clippy --locked --workspace --all-targets -- -D warnings` (same filter)
4. `cargo test --locked --workspace` (same filter). Tests that need a display must be `#[ignore]`d
   with reason `"needs display"`; summary lists them as `not run`, never as passed.
5. `cargo deny --locked check` (workspace only; skipped with `--package`)
6. `actionlint` and `zizmor` on `.github/workflows/` (skipped with `--package`)

## Steps

1. **Dependencies and skeleton.** `xtask/Cargo.toml`: add `clap` (derive), `serde`, `serde_json`;
   dev-dep `tempfile`. `xtask/src/main.rs`: clap `Cli` with subcommands; map results to the exit codes above.
   Add `xtask/src/{doctor,check,worktree,process,report}.rs`. `process.rs`: run a command with args,
   capture output to a log file, return status. `report.rs`: `Step { name, status, exit_code, log }`
   with `status ∈ passed|failed|skipped|not_run`, human print + JSON write.
   Tests: unit test for report JSON shape and exit-code mapping.
2. **tools.toml + doctor.** Create `tools.toml` with pinned versions. `doctor` checks: `rustup`, the
   toolchain from `rust-toolchain.toml`, `git`, each pinned tool (`<tool> --version` vs pin).
   Report-only for system libraries needed later (none required yet; list is empty but the mechanism exists).
   Tests: integration test with a fake `PATH` containing stub-free empty dir → tools reported missing, exit 3.
3. **check.** Implement steps above with `--package` filter (validate names via `cargo metadata --locked --no-deps`).
   Tests: unknown package → exit 2 with clear message; failing subprocess propagates exit 1
   (run against a temp crate with a format error).
4. **worktree create/list.** Use `git worktree add -b <name> .worktrees/<name> <base>`. Validate name regex,
   refuse if path or branch exists, refuse if `--base` does not resolve. `list` parses
   `git worktree list --porcelain` and shows dirty state.
   Tests (temp repo via `tempfile`): create ok; collision on path; collision on branch; bad name; bad base.
5. **worktree remove.** Refuse on dirty/untracked/unmerged unless `--force`; with `--force`, rescue
   `docs/AI/` untracked and ignored files first, then `git worktree remove --force` and keep the branch
   (never `git branch -D`). Tests: dirty refusal; untracked note refusal; `--force` rescues note;
   branch still exists after remove.
6. **deny.toml.** Licenses allow-list compatible with MIT, bans on duplicate majors as warnings,
   advisories on. Run via `check`.
7. **GitHub files.** `.github/workflows/ci.yml` (decisions D6; installs tools from `tools.toml`;
   runs `cargo xtask check --json`; uploads `target/xtask/` as artifact on failure; triggers
   `pull_request` and `push` to `main`; no secrets). `.github/dependabot.yml` (D7).
   `.github/pull_request_template.md` (summary, checks run, checks not run, linked plan).
8. **Docs.** `docs/development.md`: commands, exit codes, rust-analyzer (works with plain Cargo; no special
   config), debugging `cargo xtask` with `cargo run -p xtask -- …`, which checks need a display (none yet).
   `docs/ai-workflow.md`: worktree flow, plan location `docs/AI/`, handoff template.
   `CONTRIBUTING.md` and `AGENTS.md` checks section: replace the three commands with `cargo xtask check`
   (keep the raw Cargo commands as an equivalent fallback).
9. **Pending GitHub settings** (record in handoff, do not do): add remote; branch protection on `main`
   requiring `check` and `deny`; allow Dependabot PRs to run workflows; keep auto-merge disabled.

## Acceptance criteria

- Local and CI checks share an implementation (`cargo xtask check`); ordinary Cargo/editor tooling still works.
- Worktree helpers cannot silently discard a task's work or notes.
- Bot dependency PRs get ordinary checks; no automatic merge is enabled.
- Headless checks are explicit; display/GPU tests are reported `not run`, never passed.

## Verification

```sh
cargo test --locked -p xtask              # while developing
cargo xtask doctor
cargo xtask check                         # includes xtask tests, fmt, clippy, deny, actionlint, zizmor
cargo xtask check --allow-missing --json  # if tools are not installed; report the skipped steps
```

If the pinned tools are not installed, say so and report the skipped steps. Do not install them without asking.

## Out of scope

- Terminal, renderer, or app features. Performance harness (05). Release workflows and credentials (10).
- Creating the GitHub remote or changing GitHub settings.
- Plan copying between worktrees (`docs/AI/` is tracked).

## Handoff

Record base SHA, worktree path, changed files, decisions, exact commands/results, unrun checks, artifact locations, and blockers.
Update this plan and the index status only when supported by evidence.
