# Development

## Setup

Install [rustup](https://rustup.rs). The toolchain pinned in `rust-toolchain.toml` (with `rustfmt` and `clippy`)
installs on the first Cargo command.

## Checks

```sh
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
```

## Development commands

`cargo xtask help` shows the development commands. No commands are implemented yet.
Planned commands (not available yet): `doctor`, `check`, `run`, `worktree`, `perf`, `package`.

## Not available yet

There is no runnable terminal, no test suite, and no CI. They arrive with later milestones.
