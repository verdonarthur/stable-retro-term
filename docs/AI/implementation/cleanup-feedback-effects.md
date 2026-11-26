# Cleanup, first feedback, and CRT profiles

The implementation replaces obsolete planning material with current project
guidance. Ctrl+Shift+P opens the action panel. Ctrl+Shift+S and both menus
open Quick Settings. Right-click opens the context menu at the pointer.

Quick Settings keeps TOML precedence and the existing save behavior. It groups
controls under General, Terminal, Effects, and Advanced. The CRT profiles use
original colors and effect settings inspired by the reference screenshots.
The default and benchmark modes keep their existing geometry and color gains.

## Handoff

- Base: `bc6af45` on `main`.
- Worktree: `.worktrees/cleanup-feedback-effects`.
- Docs: project README, architecture, usage, configuration, development,
  performance, release guidance, and the AI plan index.
- App: shortcuts, menus, Quick Settings, configuration, and redraw scheduling.
- Renderer: CRT settings, shader, phosphor history, and GPU readback.
- Profiles: eleven presets under `assets/presets/`.
- Checks passed: `cargo xtask check`, release build, and GPU readback with
  both the hardware and software adapters.
- Benchmark preflight passed with Alacritty 0.17.0 and native KDE Wayland.
- Interactive GUI and IME smoke checks remain open.
- The first capture attempt stopped at the KDE portal. After approval on
  DP-1, the cleanup worktree candidate completed all eight static CRT
  scenarios against Alacritty 0.17.0. The 1.40× development gate failed on GPU
  time for blink and output, and on CPU time for resize. Typing and redraw
  passed. Startup, idle, and lifecycle remained unverified.
  See `target/perf/static.json` for raw data.
- Plain mode passed blink, typing, output, redraw, and resize. Startup and idle
  GPU time and lifecycle GPU time and peak RSS remained unverified.
  See `target/perf/plain.json`.
- Animated CRT passed typing. Blink failed the idle content check. Output and
  resize failed to show expected markers before the deadline. Startup, idle,
  redraw, and lifecycle remained unverified. See `target/perf/animated.json`.

## Follow-up

Quick Settings now opens a browsable list from the Profile row. Space opens
the list, Enter chooses the highlighted profile, and Escape returns to
settings. The normal save step applies the choice. An automated test covers
cancel and save. The action and context menus only open when they fit.

The project checks passed after these UI changes. Benchmark results above
describe the earlier build. No benchmark ran during this follow-up.

## Second feedback round

The CRT pass reserves room for margin and bezel before mapping the terminal.
The new frame shades and antialiases its edge. GPU readback covers all four
terminal corners with the largest frame, margin, and radius settings.

Quick Settings previews edits, saves without closing, and restores the last
saved appearance on cancel. Profiles replace font and CRT overrides. A
modified appearance displays Custom. The upstream profile mapping covers
the supported controls and uses system fonts.

`cargo xtask check`, the hardware GPU readback test, and `git diff --check`
passed. The benchmark results above still describe the earlier build.
