# Agent instructions

## Crate boundaries

- `crates/terminal` (`retro-terminal`): PTY sessions and terminal state. Wraps `alacritty_terminal`. No GPU or window code.
- `crates/renderer` (`retro-renderer`): GPU drawing of terminal content and CRT effects. No PTY or window-event code.
- `crates/app` (`retro-app`, binary `retro-term`): window, input, tabs, splits, configuration. Connects the two crates above.
- `xtask`: development-only commands. It never ships and does not depend on the product crates.

Do not add a shared "core" crate, plugin system, or backend traits without a concrete need.

## Checks

Run these before you hand off work:

```sh
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
```

Use only project tooling to verify work. Do not write ad-hoc verification scripts or start extra servers.
Report the checks that you ran and the checks that you did not run.

## Plans and documentation

- AI plans live in `docs/AI/<category>/<plan>.md`. Record progress and handoff in the plan.
- Human documentation lives in `docs/` (for example `docs/architecture.md`).

## Workflow

- Do not commit, push, or open a pull request unless the user asks.
- Do not add `Co-Authored-By` trailers for AI agents to commit messages.
- Do new branch work in a Git worktree under `.worktrees/` (ignored by Git).
