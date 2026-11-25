# Contributing

## Prerequisites

- [rustup](https://rustup.rs). It installs the pinned toolchain from `rust-toolchain.toml` automatically.

## Checks

```sh
cargo xtask check
```

It runs the same steps as CI. If a pinned tool is not installed, run `cargo xtask doctor`.
See [development](docs/development.md) for the plain Cargo equivalents.

All checks must pass before a change is merged. Keep `Cargo.lock` committed and generate it only with Cargo.

## Workflow

- Keep changes small and inside one crate boundary when possible (see [architecture](docs/architecture.md)).
- Do branch work in a Git worktree under `.worktrees/`.

See [development](docs/development.md) for more detail.
