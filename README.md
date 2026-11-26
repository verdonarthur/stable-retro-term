# retro-term

A terminal emulator with a retro CRT look, written in Rust.

Status: in development. There is no release yet. The terminal runs from source with
static CRT effects, a text configuration file, tabs, splits, and a settings panel.

## Platform

Linux on x86_64, in a native Wayland session. X11, macOS, and Windows are not supported.
See [usage](docs/usage.md) for the runtime requirements.

## Build and run

```sh
cargo build --locked --release -p retro-app --bin retro-term
target/release/retro-term                  # static CRT preset
target/release/retro-term --plain          # no CRT effects
target/release/retro-term -- htop          # run a program in the first pane
```

Options: `retro-term [--plain] [--config PATH] [-- COMMAND [ARG...]]`.

## Presets

- `static-crt` (default): static CRT effects.
- `plain`: no CRT effects.

Select a preset in the [configuration file](docs/configuration.md) or in the settings panel (Ctrl+Shift+P).

## Performance

The target is CPU, RAM, and GPU use within 1.20 times Alacritty. This development phase uses a limit of
1.40 times. The plain renderer has accepted evidence ([renderer baseline](docs/decisions/05-renderer-baseline.md)).
Static CRT, configuration, tabs and splits, and the settings panel are **unverified** against the limit.
See [performance](docs/performance.md).

## First steps

```sh
git clone <repository> && cd retro-term
cargo xtask doctor                               # report missing tools
cargo xtask worktree create my-task --base main  # isolated task checkout
cd .worktrees/my-task && cargo xtask check       # the same checks as CI
```

- [Usage](docs/usage.md)
- [Contributing](CONTRIBUTING.md)
- [Architecture](docs/architecture.md)
- [Development](docs/development.md)
- [Configuration](docs/configuration.md)

## Thanks

Thanks to [cool-retro-term](https://github.com/Swordfish90/cool-retro-term) for the inspiration.

License: [MIT](LICENSE).
