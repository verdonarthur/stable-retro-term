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
| `cargo xtask run` | Launch the Wayland terminal with development diagnostics and your [configuration](configuration.md) |
| `cargo xtask package --check [--tag vX.Y.Z --source-sha <sha>]` | Build the release archive with the pinned `dist` and check it. Never publishes |
| `cargo xtask release verify --tag vX.Y.Z --source-sha <sha>` | Check that tag, commit, versions, lockfile, manifest, and changelog agree |
| `cargo xtask release measure --identity <file> --gate <file> --baseline alacritty --output-dir <dir>` | Measure the packaged binary for every gate preset (GPU host only) |
| `cargo xtask release gate --identity <file> --reports <dir> --gate <file> --source-sha <sha> [--allow-failed-gate]` | Decide from report files whether the release may publish |
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

The standard checks run without a display. Display tests use
`#[ignore = "needs display"]`. The check report lists them as `not_run`.

## Run the plain terminal

Use a native Wayland session on Linux. Install the Wayland client library and
`libxkbcommon` at runtime, a Vulkan loader and driver, and a system monospace font
with fallback fonts for the scripts you use. Clipboard copy and paste require the
compositor to expose `ext-data-control` or `wlr-data-control`. Use Ctrl+Shift+C to
copy the selection and Ctrl+Shift+V to paste. The paste action accepts up to 8 MiB
of UTF-8 text.

```sh
cargo build --locked --release -p retro-app --bin retro-term
cargo xtask run
cargo test --locked -p retro-renderer --test gpu_render -- --ignored --nocapture
```

`run` launches a debug build with development diagnostics enabled. It reads your
[configuration file](configuration.md); press Ctrl+Shift+R to reload it. It records the Wayland adapter and successful presentations in
`target/xtask/logs/run.log`. The diagnostics contain no PTY or clipboard contents.
The log is replaced on each launch.

For the smoke check, use a shell and a keyboard-driven full-screen program such
as `vi`. Check ordinary input, application cursor and keypad keys, IME composition
and commit, selection, clipboard integration with another application, scrolling
through shell history, window resize, and a display scale change. Check that the
frame log stops growing while the screen is idle with a non-blinking cursor.
Exit the shell. Launch once more and close the window while the shell is running.
Record the compositor, GPU, driver, and any missing coverage in the Task 04 plan.

The GPU test reads pixels from the production draw path with fixed font fixtures.
If only a software adapter is available, set `RETRO_TERM_TEST_SOFTWARE_GPU=1` for
an optional correctness run. Hardware verification remains pending in that case.

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
(`cargo xtask check --skip deny`), `deny` (`cargo xtask check --step deny`), and `package`
(`cargo xtask package --check`). On failure, `target/xtask/` is uploaded
as an artifact. The release workflows are described in [releasing](releasing.md). Dependabot opens weekly grouped updates for Cargo and GitHub Actions; nothing merges automatically.
