# Static CRT rendering

Status: implemented locally. Native Wayland and hardware performance verification are pending.
Dependencies: 05 passed for the plain renderer.

## Context and shared rules

Read [the architecture](../architecture/retro-terminal.md) and [the execution index](../README.md) before work.
Linux first. Reuse alacritty_terminal. Three product crates plus development-only xtask.
Use matched tests for CPU, RAM, and GPU. See Decisions for the Task 06 ceiling.
Static CRT is default; animation is opt-in but gets no budget exception.
Use a task worktree and the agreed base. Do not commit, push, or open a PR unless asked.
Use project tooling only. Report skipped/unavailable checks; never invent a passing result.
Record progress and handoff here under docs/AI/implementation/. This plan overrides the old plan-directory convention.

## Goal

Add the default static CRT appearance while preserving the resource ceiling.

## Decisions

- Task 06 uses **1.40× Alacritty** for CPU, GPU, steady RSS, and peak RSS
  under matched required scenarios. This is a Task 06 exception to the
  shared 1.20× rule. Preserve the accepted Task 05 plain-renderer baseline.
- Include geometric distortion in the default static preset. Use the
  matching coordinate transform for pointer input and selection.
- Keep a full plain-rendering bypass for users and the performance suite.

## Implementation context and feasibility

Use the Task 05 head, `143d7013348459034e308b79893357bc638baebc`, as the
reviewed base. The existing `feat/static-crt` worktree starts at that commit
and has no code diff against the Task 05 branch. Check the head before work.
If a replacement worktree is needed, use
`rtk cargo xtask worktree create 06-static-crt --base 143d7013348459034e308b79893357bc638baebc`.

The app takes a terminal snapshot, converts it to a scene, and calls the
renderer on scheduler-driven redraws. The grid renderer draws fills and text
directly to the surface. Pointer input currently goes directly from physical
window coordinates to grid cells. The app also positions the IME candidate
area from the cursor cell. The performance harness launches one candidate mode.

Feasibility: **80%**. The draw, pointer, and benchmark paths have clear entry
points. Task 05 redraw GPU time is already 1.28× Alacritty. A full-window
texture and an extra pass must fit the remaining GPU and memory budget.

### Verification blocker

**B2.** Task 05 leaves startup and idle GPU and lifecycle GPU and peak RSS
unverified. See [the accepted baseline](../../decisions/05-renderer-baseline.md)
and [the measurement rules](../../performance.md). Collect valid measurements
on the matched GPU host where possible. If the harness still cannot measure a
required metric, record the exact gap and keep the corresponding acceptance
check open. A partial report cannot verify the full resource ceiling.

## Steps

1. Add an explicit render mode in `crates/app/src/main.rs` and carry it through
   `crates/app/src/app.rs` into renderer creation and device recovery. Use
   static CRT by default and add `--plain` for direct rendering. Preserve
   existing library entry points and arguments after `--`. Extend the CLI
   tests for defaults, bypass, and child argument forwarding.
2. Add `crates/renderer/src/effects.rs` and
   `crates/renderer/src/shaders/crt.wgsl`. Use a mild, monotonic curvature with
   a defined source mapping and sufficient inset to keep the outer cells
   readable. Sample a single scene texture and apply fixed tint and scanlines
   in the same full-screen pass. Keep transform parameters and coordinate
   conventions in one renderer-owned definition and pass those parameters to
   WGSL. Use a clear background outside the sampled scene. Include no time
   uniform. Add transform tests for center, edges, aspect ratios, and round
   trips between display and source coordinates.
3. Integrate the pass in `crates/renderer/src/lib.rs`. In CRT mode, draw the
   complete scene into one reusable renderable and sampleable texture per
   active surface size. Encode both passes in one submission. Recreate the
   texture on size, format, or device changes and avoid allocation at zero
   size. Keep the direct draw path in plain mode so it allocates no CRT
   texture or pipeline. Reuse existing text resources. Start with a complete
   scene update on each scheduled redraw. Add scene caching only if profiling
   justifies it, with complete invalidation before reusing cached content.
   Extend the fixed-input GPU readback in
   `crates/renderer/tests/gpu_render.rs` for direct output, tint and scanline
   variation, displaced landmarks, selection and cursor changes, and resize.
4. Map physical pointer coordinates to source pixels in
   `crates/app/src/app.rs` before calling `scene::cell_at`. Preserve unclamped
   initial clicks and clamped drag endpoints, including at the distorted
   bounds. Derive the IME candidate area from the projected cursor cell and
   refresh it after viewport changes. Keep terminal snapshots in grid
   coordinates. Extend the app scene tests for edges, padding, selection, and
   bypass. Exercise surface and device recovery on Wayland. Use the existing
   scheduler invalidation paths for terminal updates, selection, cursor,
   resize and mode changes. Extend scheduler coverage for idle and recovery
   if the integration changes frame requests.
5. Add a candidate-plain switch to `xtask/src/perf.rs` and thread it through
   `xtask/src/perf/run.py` to the candidate launcher. Record the chosen mode in
   each report. Keep paired scenarios, sample interval and 15 repetitions in
   `benchmarks/scenarios.toml`. Cover forwarding and report metadata in the
   existing harness tests. Update `docs/performance.md` with both commands
   and comparison rules. Measure both modes on the matched host. Compare
   plain-mode results with Task 05 and preserve comparable baselines for CRT
   as well. Use fresh matched measurements to investigate apparent
   regressions and keep unresolved results open. Tune the effect or renderer
   if results exceed the gates.

## Acceptance criteria

- Static CRT is the default and adds no independent animation timer.
- Tint, scanlines, and geometric distortion retain readability. Pointer and
  selection mapping follow the displayed geometry. Resize, cursor, and
  selection updates render correctly.
- The plain bypass retains direct rendering and the Task 05 baseline.
  Verify the static 1.40× CPU, RAM, and GPU ceiling and compare against
  Task 05 under the matched scenarios. Record uncertainty and missing data.

## Verification

Run the following during implementation. If the standard check reports a
missing pinned tool, run `rtk cargo xtask doctor`. Run the GPU readback and
interactive resize, selection, cursor, and IME checks on native Wayland.
Use separate report files for the two candidate modes.

```sh
rtk cargo xtask check
rtk cargo build --locked --release -p retro-app --bin retro-term
rtk cargo test --locked -p retro-renderer --test gpu_render -- --ignored --nocapture
rtk cargo xtask run
rtk cargo xtask perf --candidate target/release/retro-term --baseline alacritty --suite benchmarks/scenarios.toml --output target/perf/preflight.json --preflight
rtk cargo xtask perf --candidate target/release/retro-term --baseline alacritty --suite benchmarks/scenarios.toml --output target/perf/static.json
rtk cargo xtask perf --candidate target/release/retro-term --baseline alacritty --suite benchmarks/scenarios.toml --output target/perf/plain.json --candidate-plain
```

`--candidate-plain` is planned in step 5. Compare the reports against
[Task 05](../../decisions/05-renderer-baseline.md). Record the host, command
results, measurement gaps, and report paths in the handoff.

## Out of scope

Animation, history textures, blur, bloom, configuration files and reload,
settings UI, tabs, and splits.

## Handoff

- Base is `143d7013348459034e308b79893357bc638baebc`. Worktree is
  `/home/arthur/.t3/worktrees/retro-term/t3-25ed9db5-96b074539c94bae4`
  on `feat/static-crt`.
- The app CLI, window and scene modules select the mode and map selection and
  IME coordinates. The renderer library, effects module, and shader reuse one
  scene texture in CRT mode. Renderer readback covers tint, scanlines, landmark
  displacement, changes to fills, and texture resize. The scheduler is unchanged.
- The performance CLI, launcher, capture driver, and their tests distinguish
  CRT and plain runs. The approved addition is a CRT-aware capture probe for
  curved grid geometry, changed colors, cursor, and typed cells. Capture
  evidence was tested with synthetic frames. Live capture is pending.
- `rtk cargo xtask check` passed formatting, compilation, Clippy, and automated
  tests. It exited 3 because `cargo-deny`, `actionlint`, and `zizmor` are missing.
  `rtk cargo xtask doctor` confirmed the missing tools. See
  `target/xtask/check.json` and `target/xtask/doctor.json`.
- `rtk cargo build --locked --release -p retro-app --bin retro-term` passed.
  The native GPU readback command found only llvmpipe. The same test passed with
  `RETRO_TERM_TEST_SOFTWARE_GPU=1`.
- `rtk proxy env SHELL=/usr/bin/true cargo xtask run` could not connect to a
  Wayland compositor. See `target/xtask/logs/run.log`. Manual readability,
  selection, resize, IME, and native recovery checks are pending.
- The preflight, static, and plain commands above exited 3 because Alacritty
  was unavailable. See `target/perf/preflight.json`, `target/perf/static.json`,
  and `target/perf/plain.json`. The resource ceiling and Task 05 comparison
  remain unverified. Run the native checks and both suites on the matched host.

- PR review fixed CRT benchmark detection for isolated marker-colored desktop
  pixels and small grids where the cursor overlaps a background probe.
  The synthetic capture test covers both grid sizes. The review worktree's
  standard checks passed formatting, compilation, Clippy and tests. Doctor
  confirmed the same three missing tools listed above.
