# Repository setup

Status: done (2026-11-24), uncommitted on `main`. See Handoff.
Dependencies: None. Base: `main` at `bc5f6ab` ("doc: Planify the implementation").

## Context and shared rules

Read [the architecture](../architecture/retro-terminal.md) and [the execution index](../README.md) before work.
Linux first. Reuse alacritty_terminal. Three product crates plus development-only xtask.
CPU, RAM, and GPU must each stay within 1.20 times Alacritty for every shipped preset under matched tests.
Static CRT is default; animation is opt-in but gets no budget exception.
Do not commit, push, or open a PR unless asked.
Use project tooling only. Report skipped/unavailable checks; never invent a passing result.
Record progress and handoff here under docs/AI/implementation/.

Current state: the repository only contains `docs/AI/**` (tracked in `bc5f6ab`).
There is no `.gitignore`, no Cargo files, and `.git/info/exclude` is empty.
The toolchain installed locally is stable cargo/rustc 1.94.0.

## Decisions (settled)

- D-base: work directly on `main` in the main checkout. No worktree and no task branch for this task.
- D-doc-ai: `docs/AI/` stays tracked in Git. Do not ignore it. Do not copy plans anywhere.
- D-plan: this file is the plan for the task. Do not create `.claude/plans/`.
- D-toolchain: `rust-toolchain.toml` pins channel `1.94.0` with components `rustfmt` and `clippy`.
  Workspace: `edition = "2024"`, `rust-version = "1.94"`.
- D-lints: `[workspace.lints]` with `rust.unsafe_code = "deny"` and `clippy.all = "warn"` (priority -1).
  No pedantic. A crate can override `unsafe_code` later when it has a concrete need.
- D-license: `license = "MIT"` inherited by every package; add a root `LICENSE` file (MIT, holder "Arthur Verdon").
  All packages have `publish = false`.
- D-human-docs: human documentation lives in `docs/` (same root as `docs/AI/`). The former `doc/` folder was renamed to `docs/`.
  `docs/architecture.md` is a general architectural overview for humans (crates, responsibilities,
  data flow, resource-budget principle). No precise implementation details, no copy of `docs/AI` content,
  no link dependency on `docs/AI`. `docs/development.md` covers setup and the real checks.

## Files

- Root: `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.gitignore`, `.editorconfig`, `LICENSE`.
- `.cargo/config.toml`: `[alias] xtask = "run --package xtask --"`. No global target directory.
- `crates/terminal/` (package `retro-terminal`): `Cargo.toml`, `src/lib.rs`.
- `crates/renderer/` (package `retro-renderer`): `Cargo.toml`, `src/lib.rs`.
- `crates/app/` (package `retro-app`, binary `retro-term`): `Cargo.toml`, `src/lib.rs`, `src/main.rs`.
- `xtask/` (package `xtask`): `Cargo.toml`, `src/main.rs`.
- `AGENTS.md`, `README.md`, `CONTRIBUTING.md`, `docs/architecture.md`, `docs/development.md`.

## Steps

1. Recheck `git status`, `git log`, and existing files on `main`. Preserve any user changes.
2. Root workspace `Cargo.toml`:
   - `[workspace] resolver = "3"`, `members = ["crates/*", "xtask"]`.
   - `[workspace.package]`: `version = "0.1.0"`, `edition = "2024"`, `rust-version = "1.94"`,
     `license = "MIT"`, `publish = false`.
   - `[workspace.lints]` as in D-lints. Empty `[workspace.dependencies]` is not needed; add when first dependency arrives.
3. `rust-toolchain.toml` as in D-toolchain. `LICENSE` with the MIT text.
4. Product crates: each `Cargo.toml` uses `*.workspace = true` for version/edition/rust-version/license/publish
   and `[lints] workspace = true`. No dependencies.
   - `retro-terminal` and `retro-renderer`: `lib.rs` with a one-line crate doc comment only.
   - `retro-app`: `lib.rs` crate doc; `[[bin]] name = "retro-term", path = "src/main.rs"`; `main.rs` is an empty `fn main() {}`
     (no GUI, no output claiming functionality).
   - `retro-app` does not depend on the other crates yet (dependency arrives with its milestone).
5. `xtask`: same inheritance, no dependencies. `main.rs` parses `std::env::args`:
   no args, `help`, `-h`, `--help` print usage listing no implemented commands and saying the commands arrive in task 02;
   any other argument prints an "unknown command" error plus usage and exits with code 2.
   Do not advertise doctor/check/worktree as implemented.
6. `.cargo/config.toml` alias as in Files.
7. `.gitignore`: `/target/`, `/.worktrees/`, private runtime data (e.g. `*.log`, `/.env`, `/perf-results/` only if justified — keep minimal).
   Must not ignore `docs/` or `docs/AI/`.
8. `.editorconfig`: utf-8, lf, final newline, trim trailing whitespace, 4 spaces for `*.rs`/`*.toml`, 2 for `*.yml`/`*.md` trailing-space exception off.
9. Generate `Cargo.lock` with `cargo generate-lockfile` (or the first `cargo check`), never by hand.
10. `AGENTS.md` (portable, no personal tool requirement): crate boundaries (terminal / renderer / app / xtask dev-only),
    the project check commands (Verify section), plans in `docs/AI/<category>/<plan>.md`, human docs in `docs/`,
    no commit/push/PR unless asked, worktrees under `.worktrees/` for task branches (helper arrives in task 02),
    no ad-hoc verification scripts.
11. `README.md`: one-paragraph project intent, status "early setup, no runnable terminal yet", link to
    `CONTRIBUTING.md`, `docs/architecture.md`, MIT license.
12. `CONTRIBUTING.md`: prerequisites (rustup; toolchain auto-installed from `rust-toolchain.toml`),
    the check commands, workflow rules. Link to `docs/development.md`.
13. `docs/architecture.md` per D-human-docs (target ~40–80 lines). `docs/development.md`: setup, checks, `cargo xtask help`,
    planned milestones listed as future (no claim the app or CI exists).
14. Run the Verify commands. Record results under Handoff. Update `docs/AI/README.md`: remove the "no commits" text,
    set 01 status to Done only with evidence, note base `bc5f6ab`.

No tests are added (empty scaffolding tests are forbidden).

## Acceptance criteria

- All four packages load and compile with the pinned toolchain and lockfile.
- Product crate boundaries and binary name `retro-term` match the architecture.
- `cargo xtask help` prints usage; it lists no unimplemented command as available.
- Root instructions and contribution docs let a fresh session run the real checks.
- No GUI, terminal parser, CI, release workflow, or placeholder tests.
- `docs/AI/` is not ignored. No commit or remote operation without approval.

## Verification

```sh
cargo metadata --locked --no-deps --format-version 1
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo xtask help
git check-ignore -v target/ .worktrees/            # must match .gitignore rules
git check-ignore docs/AI/README.md; echo $?          # must print 1 (not ignored)
git status --short                                  # review new files; nothing committed
```

## Out of scope

- CI, GitHub workflows, release-please, dist, deny.toml, CHANGELOG (task 02 / 10).
- xtask doctor/check/worktree/perf commands (task 02 and later).
- Any dependency, including alacritty_terminal, wgpu, winit (task 03+).
- Commits, branches, worktrees, pushes.

## Handoff

Done on `main` (base `bc5f6ab`), no worktree, nothing committed.

Changed files: `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.cargo/config.toml`, `.gitignore`, `.editorconfig`,
`LICENSE`, `crates/{terminal,renderer,app}/**`, `xtask/**`, `AGENTS.md`, `README.md`, `CONTRIBUTING.md`,
`docs/architecture.md`, `docs/development.md`, `docs/AI/README.md`, `docs/AI/architecture/retro-terminal.md`. Earlier: `doc/` renamed to `docs/` (staged `git mv`).

Results (2026-11-24, toolchain 1.94.0):
- `cargo metadata --locked --no-deps`: exit 0.
- `cargo fmt --all -- --check`: exit 0.
- `cargo check --locked --workspace --all-targets`: exit 0, 4 crates.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: no issues.
- `cargo xtask help`: usage, exit 0. Unknown command: exit 2.
- `git check-ignore`: `target/` and `.worktrees/` ignored; `docs/AI/README.md` not ignored (exit 1).

Deviation: `.gitignore` has `!/AGENTS.md`, because the user's global gitignore ignores `AGENTS.md`.

Not run: no tests exist (by design). No CI (task 02).
Next: task 02.
