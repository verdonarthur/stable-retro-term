# retro-term

A terminal emulator with a retro CRT look, written in Rust. Linux is the first target platform.
The CRT effects must stay cheap: CPU, RAM, and GPU use each stay within 1.20 times Alacritty.

Status: early setup. There is no runnable terminal yet.

## First steps

```sh
git clone <repository> && cd retro-term
cargo xtask doctor                               # report missing tools
cargo xtask worktree create my-task --base main  # isolated task checkout
cd .worktrees/my-task && cargo xtask check       # the same checks as CI
```

- [Contributing](CONTRIBUTING.md)
- [Architecture](docs/architecture.md)
- [Development](docs/development.md)
- [Configuration](docs/configuration.md)

## Thanks

Thanks to [cool-retro-term](https://github.com/Swordfish90/cool-retro-term) for the inspiration.

License: [MIT](LICENSE).
