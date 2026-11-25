# Development

## Setup

Install [rustup](https://rustup.rs). The toolchain pinned in `rust-toolchain.toml` (with `rustfmt` and `clippy`)
installs on the first Cargo command.

Auxiliary tools are pinned in `tools.toml`. `cargo xtask doctor` reports missing or wrong versions and prints the
install command. It never installs anything.

## Commands

| Command | Purpose |
| --- | --- |
| `cargo xtask doctor [--json]` | Report Git, rustup, the toolchain, and pinned tools |
| `cargo xtask check` | Run every check that CI runs |
| `cargo xtask check -p retro-terminal` | Check one package (fmt, deny, and workflow lint are skipped) |
| `cargo xtask check --step clippy` / `--skip deny` | Run only, or leave out, some steps |
| `cargo xtask check --allow-missing` | Do not fail because a pinned tool is not installed |
| `cargo xtask worktree create <name> --base <rev>` | New worktree `.worktrees/<name>` on branch `<name>` |
| `cargo xtask worktree list [--json]` | Show worktrees, branches, and dirty state |
| `cargo xtask worktree remove <name> [--force]` | Remove a worktree; the branch is kept |

`check` steps: `fmt`, `check`, `clippy`, `test`, `deny`, `actionlint`, `zizmor`.
Each step logs to `target/xtask/logs/<step>.log`. Summaries go to `target/xtask/<command>.json` (`--json` also
prints them). Step statuses: `passed`, `failed`, `missing` (tool not installed), `skipped`, `not_run`.

Exit codes: `0` success, `1` a step failed or a request was refused, `2` usage error, `3` a pinned tool is missing.

The raw Cargo commands still work without xtask:

```sh
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
```

## Headless and display tests

All checks are headless. No test needs a display yet. A test that needs a display must be
`#[ignore = "needs display"]`; `check` then reports the ignored tests as `not_run`, never as passed.

## Editor and debugger

rust-analyzer works with the plain workspace; no extra configuration is needed.
To debug xtask, run `cargo run -p xtask -- <command>` or point the debugger at `target/debug/xtask`.

## Worktrees

Each worktree has its own `target/`. Only the Cargo download caches in `~/.cargo` are shared.
`worktree remove` refuses when the worktree has uncommitted or untracked files, or changed or ignored files
under `docs/AI/`. `--force` removes it anyway, after it copies those `docs/AI/` files to
`target/xtask/rescued/<name>-<time>/` in the main checkout. Branches are never deleted.

## CI

`.github/workflows/ci.yml` runs on pull requests and pushes to `main`. Jobs: `check`
(`cargo xtask check --skip deny`) and `deny` (`cargo xtask check --step deny`). On failure, `target/xtask/` is uploaded
as an artifact. Dependabot opens weekly grouped updates for Cargo and GitHub Actions; nothing merges automatically.
