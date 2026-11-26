# retro-term

A terminal emulator with a retro CRT look, written in Rust.

Status: in development. There is no release yet. The terminal runs from source with
CRT profiles, tabs, splits, and Quick Settings.

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

## Using the terminal

Press Ctrl+Shift+P to open the action panel. Use it to open tabs, split panes,
or open Quick Settings. Press Ctrl+Shift+S for direct access to Quick Settings.
Right-click for clipboard and pane actions.

Choose a built-in [CRT profile](docs/configuration.md#profiles) or adjust the
screen, terminal, and effects settings. Use the plain profile to turn the CRT
effects off.

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
