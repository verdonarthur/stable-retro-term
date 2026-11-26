# Architecture

retro-term is a terminal emulator with a retro CRT appearance.
It reuses an existing terminal engine and focuses its own work on rendering and user experience.

## Principles

- **Resource budget.** CPU, RAM, and GPU use each stay within 1.20 times Alacritty on the same machine and workload.
  This applies to every shipped preset, including animated ones. It is the release target.
  The current development phase uses a limit of 1.40 times ([renderer baseline](decisions/05-renderer-baseline.md)).
- **Static by default.** The default CRT look is static. Animation is opt-in and must fit the same budget.
- **Reuse before build.** Terminal emulation comes from `alacritty_terminal`, not a custom parser.
- **Linux first.** Keep a practical path to macOS and Windows.

## Components

```text
          keyboard, mouse, window events
                       │
                 ┌─────▼─────┐
                 │    app    │  windows, tabs, splits, configuration
                 └──┬─────┬──┘
     input, resize  │     │  screen snapshot
              ┌─────▼──┐ ┌▼─────────┐
              │terminal│ │ renderer │  GPU text and CRT effects
              └───┬────┘ └──────────┘
                  │
            shell process (PTY)
```

| Crate | Package | Responsibility |
| --- | --- | --- |
| `crates/terminal` | `retro-terminal` | Starts shells in a PTY and keeps terminal state. |
| `crates/renderer` | `retro-renderer` | Draws terminal content and CRT effects on the GPU. |
| `crates/app` | `retro-app` | The `retro-term` binary: window, input, tabs, splits, configuration. |
| `xtask` | `xtask` | Development commands only. Never shipped. |

The terminal and renderer crates do not know about each other. The app crate connects them.

## Data flow

1. The app sends keyboard input and window size to the terminal crate.
2. The terminal crate reads shell output and updates the terminal state.
3. The app takes a snapshot of the visible screen and gives it to the renderer.
4. The renderer redraws only when something changed, or when an enabled animation needs a frame.

## Milestones

Done in development builds: terminal engine, plain graphical terminal, performance harness, static CRT,
text configuration, tabs and splits, settings panel. Planned: optional animation, automatic release.
The settings panel is drawn into the same scene as the terminal, with no extra GPU resources.
