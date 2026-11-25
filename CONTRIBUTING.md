# Contributing

## Prerequisites

- [rustup](https://rustup.rs). It installs the pinned toolchain from `rust-toolchain.toml` automatically.

## Checks

```sh
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
```

All checks must pass before a change is merged. Keep `Cargo.lock` committed and generate it only with Cargo.

## Workflow

- Keep changes small and inside one crate boundary when possible (see [architecture](docs/architecture.md)).
- Do branch work in a Git worktree under `.worktrees/`.

See [development](docs/development.md) for more detail.
