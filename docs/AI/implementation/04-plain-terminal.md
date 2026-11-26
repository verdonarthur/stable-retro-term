# Plain graphical terminal

Status: implemented and verified on Wayland. Interactive IME coverage was skipped.
Dependency: Task 03 merged into `main` at `5f28328`.

## Context and shared rules

Read [the architecture](../architecture/retro-terminal.md), [the execution index](../README.md),
and [the AI workflow](../../ai-workflow.md).

The app and renderer are stubs at the pinned base. The terminal crate provides a
Linux PTY session, coalesced wakeups, and owned snapshots of the visible grid.
Use [session.rs](../../../crates/terminal/src/session.rs) for wakeups, input, resize,
selection, scrollback, and close. [snapshot.rs](../../../crates/terminal/src/snapshot.rs)
defines the grid, cell, and cursor data. Existing
[terminal tests](../../../crates/terminal/tests/terminal.rs) and
[PTY tests](../../../crates/terminal/tests/session.rs) cover terminal state and session
behavior without a display. The session API needs a way to read application cursor
and keypad modes. [The xtask CLI](../../../xtask/src/main.rs) has `check`, `doctor`,
and `worktree`. This task adds `run`.

Keep PTY and terminal mode handling in `retro-terminal`, GPU work in
`retro-renderer`, and window events and platform integration in `retro-app`.
Keep `xtask` independent of the product crates. Use project tooling for verification.
Do not commit, push, or open a PR unless asked. CPU, RAM, and GPU each have a limit
of 1.20 times Alacritty for every shipped preset under matched tests. Task 05 measures
the plain renderer and decides whether to keep the graphics stack.

## Goal

Connect one shell to a correct, event-driven graphical terminal with CRT effects disabled.

## Settled decisions

- D1. Keep this plan, progress, and handoff in this tracked document.
- D2. Use `5f2832848f6382b0c1b74acb03dc882c394b8c60` on `main` as the worktree base.
- B1. Native Wayland is the only display backend required for Task 04 completion.
  Run the GPU correctness tests and interactive smoke check on a Linux machine with
  a GPU and a Wayland session. If that setup is unavailable, record the blocker in
  the handoff. X11 coverage is deferred.
- D3. Include ordinary keyboard input, application cursor and keypad modes, IME,
  selection, and clipboard shortcuts. Defer PTY mouse reporting.
- D4. Add ISC, Zlib, and CC0-1.0 to the workspace license allowlist.
- D5. Use the safe Wayland data-control clipboard API. Clipboard shortcuts require
  the compositor to expose `ext-data-control` or `wlr-data-control`.

## Start the implementation worktree

From the main checkout, run:

```sh
rtk cargo xtask worktree create 04-plain-terminal --base 5f2832848f6382b0c1b74acb03dc882c394b8c60
```

Copy this refined document into the new worktree if the pinned base still has the
initial plan. Continue in the worktree printed by the command. Record its path below.

## Steps

1. **Resolve the graphics stack.** Update `Cargo.toml`, the app and renderer
   manifests, and `Cargo.lock`. Compile a compatible `winit`, `wgpu`, and `glyphon`
   combination on the pinned Rust toolchain. Use one `wgpu` version for the renderer
   and glyph cache and one device and queue for drawing. Check the resolved licenses
   with the repository's deny step. Record the selected versions and Linux build
   requirements. Keep the stack provisional until Task 05.
2. **Draw a fixed grid.** Implement `crates/renderer/src/{lib,scene,text}.rs`.
   Define renderer-owned scene data and convert the terminal's owned snapshot in the
   app. Place glyphs using cell positions and widths. Keep combining marks with their
   base glyph, preserve a wide glyph's two-cell footprint, skip spacer glyphs, and
   keep fallback glyphs aligned to the grid. Draw colors, decorations, selection,
   cursor, and box drawing. Use an evictable glyph cache. Add headless tests for scene
   conversion and fixed-cell placement, including a wide glyph at a line boundary.
   Add redistributable font fixtures with their licenses for tests that need specific
   glyph coverage. Load those fixtures explicitly so test results do not depend on
   installed fonts.
3. **Connect window and PTY lifecycles.** Implement `crates/app/src/{main,lib,app,platform}.rs`.
   Open one window and start one session with the grid dimensions calculated from
   physical pixels and font metrics. On resize or scale change, use the same grid
   dimensions for the terminal and PTY via `Session::resize`. Handle invalid or
   tiny sizes without sending an invalid resize. Reconfigure the surface when its
   size changes and suspend presentation at zero size. Use the session callback only
   to send a user event to the window loop. Poll session events on the app thread even
   when rendering is suspended, so protocol replies and exit events are served.
   Update the window title from session events. Reuse `Session::close` for window close
   and handle child exit through `Session::poll`. Close outside terminal and UI locks.
   Keep the known Task 03 close behavior for children that ignore hangup. Add headless
   tests for grid size and scale conversion, and reuse the PTY resize and exit tests.
4. **Map desktop input to the session.** Add `crates/app/src/input.rs` for key
   mapping and keep Wayland clipboard calls and window integration in `platform.rs`.
   Add a small platform-neutral mode query in `crates/terminal/src/session.rs` and
   export its type from `lib.rs`. Read the current application cursor and keypad modes
   when encoding keys. Keep Alacritty types private. Route committed IME text once and
   keep preedit local. Map terminal control keys and shortcuts, including Ctrl+C for
   the shell and Ctrl+Shift+C/V for clipboard copy and paste. Use `Input::Paste` to
   preserve bracketed paste. Convert pointer positions to visible grid cells for drag
   selection and map scrolling to scrollback. Test mode changes with terminal escape
   fixtures and key output with expected bytes. Cover shortcut precedence, IME
   commits, and selection coordinates in app tests.
5. **Schedule frames on demand.** Add `crates/app/src/scheduler.rs` and connect it to
   the window loop. Coalesce dirty changes into one pending redraw. Use wait and
   deadline scheduling for content changes and visible cursor blink. Invalidate for
   input changes that affect the view, selection, scroll, resize, expose, and surface
   recovery. Keep dirty state while presentation is suspended and redraw on recovery.
   Poll the session independently of frame scheduling. Take one owned snapshot when
   a frame needs it, then release the terminal lock before shaping or GPU calls.
   Test the scheduler with explicit times and events. Cover non-blinking idle, blink
   deadlines, coalesced redraws, invalidation during redraw, and suspend and resume.
6. **Add the development launch command.** Register `run` in `xtask/src/main.rs`
   and implement it in `xtask/src/run.rs`. Launch the app with locked Cargo resolution,
   explicit plain development settings, and diagnostics under the worktree's
   `target/xtask/`. Pass process arguments directly and propagate failures.
   Keep PTY content and clipboard data out of diagnostic logs. Add CLI help and error
   tests in `xtask/tests/cli.rs`. Document the run command, settings, and logs in
   `docs/development.md`.
7. **Verify on Wayland.** Add `crates/renderer/tests/gpu_render.rs` with selected GPU
   correctness cases driven by the font fixtures. Test the production draw path with
   a render target that supports pixel readback. Check fixed positions for wide,
   combining, fallback, and box-drawing glyphs, plus background and cursor placement.
   Use `#[ignore = "needs display"]` for these opt-in tests. Report adapter or surface
   unavailability explicitly and treat it as incomplete coverage. Document the GPU
   test command and smoke procedure in `docs/development.md`. In `cargo xtask run`,
   exercise an interactive shell and a keyboard-driven full-screen program. Confirm
   native Wayland, then check text, mode-dependent keys, IME commit, selection,
   clipboard, scrollback, resize and scale, non-blinking idle, and clean exit through
   both window close and shell exit. Record the GPU, driver, desktop backend, and
   missing coverage.

## Acceptance criteria

- One interactive shell on native Wayland renders the fixed grid, accepts the agreed
  input, resizes the grid and PTY together, and closes cleanly.
- Unchanged non-blinking idle does not request a continuous frame loop.
- Headless correctness tests pass. Wayland GPU correctness tests and the interactive
  smoke check pass before Task 04 is marked complete. Record any unsupported conditions.
- Resource budget verification remains in Task 05.

## Verification

Run from the implementation worktree after adding the test target and launch command:

```sh
rtk cargo xtask check
rtk cargo build --locked --release -p retro-app --bin retro-term
rtk cargo test --locked -p retro-renderer --test gpu_render -- --ignored --nocapture
rtk cargo xtask run
```

Run the final two commands from a native Wayland session on the GPU test machine.
If `check` reports a missing pinned tool, run `rtk cargo xtask doctor` and record
the unavailable step. Record exit codes, GPU test counts, smoke results, and log paths.

## Out of scope

- X11 verification, PTY mouse reporting, and OSC 52 clipboard access.
- CRT effects, tabs, splits, user configuration files, and a settings editor.
- PTY shutdown policy changes and resource measurements.

## Handoff

- Base: `5f2832848f6382b0c1b74acb03dc882c394b8c60` on `main`.
- Worktree: `.worktrees/04-plain-terminal` on branch `04-plain-terminal`.
- Step 1 changed the workspace, app, and renderer manifests, `Cargo.lock`, and
  `deny.toml`. The lockfile has winit 0.30.13, wgpu 29.0.4, glyphon 0.11.0, and
  wl-clipboard-rs 0.9.5. The app uses the native Wayland backend and the renderer uses
  Vulkan. See `docs/development.md` for runtime requirements.
- Steps 2 and 7 implemented the renderer in `crates/renderer/src/` and added
  `tests/gpu_render.rs` and its fixtures. The app maps snapshots to the renderer
  scene in `crates/app/src/scene.rs`. Common light box junctions use cell geometry.
  Other box drawing uses the font.
- Steps 3–5 implemented the window lifecycle, key and IME handling, clipboard,
  geometry, and scheduler in `crates/app/src/`. The terminal crate exposes keyboard
  modes, default background, and a scroll-to-bottom operation. Headless tests cover
  key sequences, composition, scene conversion, timing, and PTY mode changes.
- Step 6 added `xtask/src/run.rs`, registered the command, and added CLI tests and
  the smoke check instructions in `docs/development.md`.
- Checks used Rust 1.94 and the pinned auxiliary tools in a temporary tool directory.
  `rtk cargo xtask doctor` and `rtk cargo xtask check` exited 0 with these tools on
  PATH. Format, compile, Clippy, 41 headless tests, deny, actionlint, and zizmor
  passed. The standard check reported one opt-in GPU test as `not_run`.
  `rtk cargo build --locked --release -p retro-app --bin retro-term` exited 0.
- `rtk proxy cargo test --locked -p retro-renderer --test gpu_render -- --ignored --nocapture`
  passed on AMD Radeon 890M Graphics (RADV STRIX1), Vulkan, Mesa 26.2.7 in a
  KDE Wayland session. A separate software run with
  `RETRO_TERM_TEST_SOFTWARE_GPU=1` passed on llvmpipe. The sandbox exposed only
  llvmpipe. The host run supplied access to the hardware for the required check.
- `rtk cargo xtask run` opened a window on native Wayland, presented frames on the
  AMD adapter, and exited 0. The user exercised `ls`, `ll`, window movement, and
  resize and reported no failures. The interactive run's close action was not reported.
  `rtk proxy env SHELL=/usr/bin/true cargo xtask run` exited 0 after the PTY child
  exited immediately.
- Follow-up smoke check passed for the full-screen editor, application keypad,
  selection and clipboard exchange, and the scrollback and scale step.
  The idle retest and explicit window close with a live shell passed. IME was
  skipped because no input method was available.
- Artifacts are in the worktree's `target/xtask/`, including `check.json`,
  `doctor.json`, `logs/run-interactive.log`, and `logs/run.log`.
- Resource measurements remain in Task 05.

- PR review fixed a possible blank window when the GPU repeatedly supplied a
  usable but suboptimal frame. The renderer now presents that frame.
  `cargo xtask doctor` and `cargo xtask check` passed with the pinned tools.
  The opt-in GPU test and interactive Wayland smoke check were not rerun.
