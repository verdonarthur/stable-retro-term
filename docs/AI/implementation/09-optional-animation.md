# Optional CRT animations

Status: implemented, perf unverified. Q1–Q9 decided (recommended options). See Progress and Handoff.
Dependencies: 08 implemented locally (PR #10, `08-tabs-and-splits`). 04–07 are in the same stack.

## Context and shared rules

Read [the architecture](../architecture/retro-terminal.md) (sections "Feasibility and unresolved
engineering risks" B2, "Rendering and resource policy", "Performance acceptance and measurement"),
[the execution index](../README.md), [the AI workflow](../../ai-workflow.md),
[task 06](06-static-crt.md) (the CRT pass), [task 07](07-text-configuration.md) (the config model),
[performance](../../performance.md) and [the renderer baseline](../../decisions/05-renderer-baseline.md).

Linux first. Three product crates plus development-only `xtask`. Keep the crate boundaries in
`CLAUDE.md`: effect math and shaders live in `retro-renderer`; config, presets, and the frame
counter live in `retro-app`; the capture probe lives in `xtask`. `xtask` does not depend on product crates.
Make surgical changes. Do not commit, push, or open a PR unless the user asks.
Use project tooling only. No ad-hoc scripts, no extra servers. Report checks that did not run.

## Goal

Add one opt-in temporal CRT effect under the same resource rule as the static preset, with
deterministic tests, a perf mode that can measure it, and an honest feasibility record.
Static CRT stays the default and stays byte-identical.

## Settled decisions (user)

- D1. Base: `a03bd1bdb79bd741aec8a0e119d364fd5292dba6` (`origin/08-tabs-and-splits`, PR #10,
  stacked on #8 -> #7 -> #6 -> #5). The PR for this task targets `08-tabs-and-splits`.
- D2. Worktree: `.worktrees/09-optional-animation` on branch `09-optional-animation`. It exists.
- D3. Performance limit for this phase: **1.40× Alacritty** (`docs/decisions/05-renderer-baseline.md`).
  The release gate (task 10) uses 1.20×.
- D4. This host is the measurement machine (AMD Radeon 890M, RADV), but Alacritty 0.17.0 is not
  installed. `cargo xtask perf` exits 3 here. Implement anyway and record performance as
  **unverified**. Never claim a pass.
- D5. No animated preset is "delivered", "supported", or "passing" without perf evidence. Docs label
  it "experimental; performance unverified" until a valid report exists.
- D6. Static stays the default. Animation is opt-in and gets **no** budget exception.
- D7. Task 10's manual `allow_failed_gate` override does not change this task's rules. An override
  is a release decision, not perf evidence.
- D8. Tools installed. `cargo xtask check` runs every step; a missing-tool skip is a real problem.
- D9. No new crate dependencies.

## Code facts at the base (read before work)

- `crates/renderer/src/effects.rs`:
  - `enum RenderMode { Plain, #[default] StaticCrt }` (Copy, Eq). `source_point`/`display_point`
    return the point unchanged only for `Plain`; geometry constants `SCALE = 1.025`,
    `CURVATURE = 0.018` are duplicated in `xtask/src/perf/wayland.py` (`crt_point`).
  - `Crt { pipeline, sampler, params, target }`. `params` is a 16-byte `UNIFORM` buffer
    (`[SCALE, CURVATURE, 0.0, 0.0]`) created with `create_buffer_init`, **no `COPY_DST`**, never
    written again. `prepare()` reallocates the scene texture only on size change. `draw()` is one
    full-screen triangle into the surface view.
- `crates/renderer/src/shaders/crt.wgsl`: `params: vec4<f32>`; uses `.x`, `.y`. Fixed tint
  `(0.95, 1, 1)` and scanline gain `0.88` on odd rows. Comment: "no time uniform" (task 06 step 2).
- `crates/renderer/src/lib.rs`: `GridRenderer::with_mode` builds `Crt` only when
  `mode == RenderMode::StaticCrt`. `GridRenderer::draw(device, queue, view, size, scene)` encodes
  scene pass + CRT pass in one submission. `SurfaceRenderer::{with_family, reconfigure,
  recreate_surface}` rebuild `GridRenderer` from `mode`; `SurfaceRenderer::draw(scene, before_present)`.
- `crates/renderer/tests/gpu_render.rs`: one `#[ignore = "needs display"]` test
  `fixed_grid_readback`; helpers `readback(..)`, `at(..)`; asserts exact static CRT values
  (`bright.1 == 96`, `(0,0)` black). `cargo xtask check` reports ignored tests as `not-run`
  (`xtask/src/check.rs` `ignored_tests_step`), never as passed.
- `crates/app/src/scheduler.rs`: `dirty`/`pending` coalescing, blink deadline (500 ms), retry
  deadline (50 ms). `deadline()` drives `ControlFlow::WaitUntil` in `App::about_to_wait`
  (`app.rs` ~line 1007). No frames when idle and non-blinking (test
  `coalesces_redraws_and_does_not_poll_nonblinking_idle`). Suspend/occlusion clear timers.
- `crates/app/src/app.rs`: `State::draw` (~line 615) gets `now`, builds one `Scene`, calls
  `renderer.draw(..)`, handles `Frame::*`. `render_mode(config)` (~line 881) maps
  `effects.crt` to `StaticCrt`/`Plain`. `reload()` calls `renderer.reconfigure(render_mode(..), ..)`
  when `Changes::between(..).renderer`.
- `crates/app/src/config.rs`: `Effects { crt: bool }`, `EffectsLayer { crt: Option<bool> }`,
  `Layer::apply`, `PRESETS` = `static-crt`, `plain` (`include_str!` of `assets/presets/*.toml`).
  `Changes::between` treats any `effects` difference as a renderer rebuild. `--plain` sets
  `effects.crt = false` after the user layer.
- `crates/app/tests/config.rs`: `every_builtin_preset_resolves` iterates `PRESETS`;
  `example_file_equals_defaults` parses `assets/config.example.toml`.
- `xtask/src/perf.rs`: `--candidate-plain` only; report `candidate_mode` is `"plain"` or
  `"static-crt"`. `Suite` has `deny_unknown_fields`, `candidate_config` key, exactly 8 scenarios.
- `xtask/src/perf/run.py`: `candidate_flags(request)` = `--config <candidate_config>` [+ `--plain`].
  `leg()` sets `driver.crt`. `cursor_run()` fails with "terminal content changed during idle" if a
  captured frame stops matching the expected colors. Marker colors differ by **1 blue level**
  (`[51, 91, 129 + index]`, `[51, 91, 130 + index]`).
- `xtask/src/perf/wayland.py`: `crt_colors(color)` returns the exact sRGB bytes for the fixed tint
  and the two scanline phases; `visible_crt`, `crt_evidence`, `crt_glyph` require exact matches.
  Any per-frame brightness change breaks these checks.
- `xtask/src/perf/test_perf.py`: headless unit tests run by `xtask/tests/perf.rs`
  (`driver_headless_regressions`) inside `cargo xtask check`.
- Task 10 (PR #9, `origin/10-automatic-release`, based on `feat/static-crt`, not on this stack):
  `benchmarks/release-gate.toml` lists `[[presets]] name, candidate_plain: bool` for `static-crt`
  and `plain`; `xtask/src/release.rs` checks each report's `candidate_mode` against
  `candidate_plain`. Its follow-up F1 says: replace the list with the presets that 07 ships and that
  09 marks feasible, and add per-preset config. PR #9 also adds `perf --limit N/D` and removes
  `candidate_config` from `Suite` (it predates task 07).

## Effect candidates (analysis)

| Effect | Needs | Idle cost (B2) | Probe impact | Pointer mapping | Verdict |
| --- | --- | --- | --- | --- | --- |
| Flicker (global brightness gain per frame) | 1 uniform float, CPU-side gain | 0 on needed frames | Gain-aware exact colors | Unchanged | **First** |
| Noise (per-pixel hash, seeded per frame) | Shader hash, 1 seed | 0 on needed frames | Per-pixel variance; exact checks fail | Unchanged | Later, after flicker |
| Rolling scanline band | Continuous time | Needs independent timer: fails idle when Alacritty GPU ≈ 0 | Band moves through probe points | Unchanged | Defer |
| Phosphor persistence / ghosting | History texture (+1 full-window texture), decay tail frames | Bounded tail timer after each change | Ghost colors near changes | Unchanged | Defer; needs measured proof |
| Jitter (frame offset) | Time-varying transform | Like flicker if on needed frames | Grid area moves | Must follow time; IME/selection drift | Reject for now |

"Needed frames": frames the scheduler already requests for terminal output, input, cursor blink,
selection, resize, reload, or recovery. On those frames the effect only changes a uniform, so it
adds no frame, no texture, and no wakeup. Idle stays frozen at the last state. This is normal
product behavior, not benchmark behavior (architecture: "Do not silently disable animation only
during benchmarks").

## Design (follows the recommended answers; change it if the user answers otherwise)

- Renderer:
  - `RenderMode::AnimatedCrt` added. Geometry identical to `StaticCrt` (`map` treats every non-`Plain`
    mode the same; keep the `self == Self::Plain` test).
  - `effects.rs`: `pub const FLICKER_GAINS: [f32; 3] = [1.0, 0.975, 0.955];` (changed from the
    planned 4 gains by T11, see Deviations) and a pure
    `pub fn flicker_gain(frame: u32) -> f32` (integer hash of `frame`, index into `FLICKER_GAINS`).
    No clock, no `Instant`, no randomness source.
  - `Crt::new(device, format, animated: bool)`: `params` gets `COPY_DST`; initial value
    `[SCALE, CURVATURE, 1.0, 0.0]` (`.z` = gain). `Crt::set_frame(queue, frame)` writes the 4-byte
    gain only when `animated`. Static mode never writes the buffer after creation.
  - `crt.wgsl`: multiply the final color by `params.z`. With `params.z == 1.0` the static output is
    unchanged (exact float multiply by 1.0). Update the "no time uniform" comment.
  - `GridRenderer::set_animation_frame(&mut self, frame: u32)` and
    `SurfaceRenderer::set_animation_frame(..)` store the frame; `draw` uploads the gain before encoding.
    `draw` signatures do not change.
- App:
  - `State.animation_frame: u32`, `wrapping_add(1)` after each `Frame::Presented`. Set on the
    renderer before `renderer.draw(..)`. Only `AnimatedCrt` reads it.
  - `render_mode(config)`: `crt && animation` -> `AnimatedCrt`; `crt` -> `StaticCrt`; else `Plain`.
  - Scheduler: **no change**. No new deadline, no frame-rate cap needed (frames stay coalesced by
    `pending` and presented through the surface's present mode).
- Config (07 model):
  - `Effects { crt: bool, animation: bool }`, default `animation = false`; `EffectsLayer.animation`.
  - `assets/presets/animated-crt.toml`: `[effects] crt = true, animation = true`. Add to `PRESETS`.
  - `animation = true` with `crt = false` is valid and renders `Plain` (so `--plain` works with the
    animated preset).
- Perf:
  - `xtask perf --candidate-animated` (conflicts with `--candidate-plain`). Report
    `candidate_mode: "animated-crt"`. run.py passes `--config <animated_config>`; new optional suite
    key `animated_config = "fixtures/retro-term-animated.toml"` (serde default; suite version stays 1).
  - `wayland.py`: `crt_colors(color, gains=(1.0,))`; in animated mode the driver uses
    `FLICKER_GAINS` (copied, with a comment naming `effects.rs`, the same way as `SCALE`/`CURVATURE`).
    A frame matches when **all** probe points match the same gain. Idle (`cursor_run`) accepts gain
    changes only on frames whose content otherwise matches.
  - Same 8 scenarios, 15 pairs, same Alacritty leg. Idle with a non-blinking cursor already covers
    "non-blinking idle for every animated preset" (architecture line ~438).

## Steps

Run `cargo xtask check` after each step. Keep each step a small, reviewable diff.

1. **Answer the open questions.** Record the answers under "Decisions on Q1–Q9" below. If an
   answer changes the design, update the Design section first.
2. **Pure effect math (renderer, headless).** In `effects.rs` add `FLICKER_GAINS`,
   `flicker_gain(frame)`, and `RenderMode::AnimatedCrt`. Unit tests (see Tests T1–T3).
3. **GPU path.** `Crt::new(.., animated)`, `COPY_DST`, `params.z`, `set_frame`, shader multiply,
   `GridRenderer::with_mode` builds `Crt` for both CRT modes, `set_animation_frame` on both
   renderers, keep `mode` through `reconfigure`/`recreate_surface`. Extend `gpu_render.rs` (T4–T5).
   Do not change the existing static assertions.
4. **Config.** `Effects.animation`, `EffectsLayer.animation`, `Layer::apply`, default `false`,
   `assets/presets/animated-crt.toml`, `PRESETS`, `assets/config.example.toml` (preset list comment
   and `animation = false` with its comment), `docs/configuration.md` (key row, preset list with
   "experimental; performance unverified", `--plain` note). Tests T6–T8.
5. **App wiring.** `render_mode`, `animation_frame` counter in `State::draw`. Scheduler untouched;
   confirm the scheduler unit tests still pass unchanged. Test T9.
6. **Perf mode.** `perf.rs` flag + report mode + optional suite key; `run.py` `candidate_flags`;
   `benchmarks/fixtures/retro-term-animated.toml` (copy of `retro-term.toml` with
   `preset = "animated-crt"`, `animation = true`); `wayland.py` gain-aware matching. Tests T10–T12.
   Keep the static and plain paths byte-identical. Update `docs/performance.md` with the third command.
7. **Run what exists** (Verification). Record exit codes and report paths.
8. **Record the feasibility outcome** with the rules below. Update the index row in
   `docs/AI/README.md` and the Progress/Handoff sections here.

## Tests to add (exact)

Headless (run by `cargo xtask check`):

- T1 `effects::tests::flicker_gain_is_deterministic_and_bounded`: for `frame in 0..10_000`,
  `flicker_gain(frame) == flicker_gain(frame)`, value is in `FLICKER_GAINS`; every entry of
  `FLICKER_GAINS` appears; `FLICKER_GAINS[0] == 1.0`; min ≥ 0.95 (tested range).
- T2 `effects::tests::flicker_gain_varies_between_frames`: in `0..64` at least 2 distinct values,
  and no run of more than 8 identical consecutive values (visible variation on needed frames).
- T3 extend `curvature_retains_edges_and_round_trips`: `AnimatedCrt` gives the same
  `display_point`/`source_point` as `StaticCrt` for every tested point and size.
- T6 `crates/app/tests/config.rs::animated_preset_enables_animation`: `preset = "animated-crt"`
  resolves to `crt = true, animation = true`; user `animation = false` overrides it;
  `Overrides { plain: true, .. }` gives `crt = false` (animation value irrelevant).
- T7 `defaults_match_the_base_behavior` / `example_file_equals_defaults`: `animation == false`.
- T8 `changes_classify_live_and_new_session_settings`: toggling `effects.animation` sets
  `renderer` and `layout` (current `Changes` rule) and not `new_sessions`.
- T9 app unit test next to `render_mode` (Linux-gated module): the 4 combinations of
  `crt`/`animation` map to `Plain`, `Plain`, `StaticCrt`, `AnimatedCrt`.
- T10 `test_perf.py::test_candidate_mode_flag`: animated request gives
  `["--config", animated_config]` with no `--plain`.
- T11 `test_perf.py::test_flicker_gains_keep_markers_distinct`: for every pair of suite marker
  colors (`READY`, `START`, `HISTORY`, `[51,91,129..153]`, `[51,91,130..149]`, typed colors
  `[81,97,150..156]`) and every pair of gains × scanline phase, `crt_colors` triples never collide
  across different colors. If this fails, change the gains (not the markers) and update T1.
- T12 `test_perf.py`: extend `check_crt_frame` with a synthetic frame rendered at gain 0.97: it
  is visible in animated mode, not visible in static mode; a frame mixing two gains is rejected.
- Rust side of perf: extend `xtask/tests/perf.rs` help test to show `--candidate-animated`, and
  that `--candidate-plain --candidate-animated` exits 2.

GPU readback (`#[ignore = "needs display"]`, run manually on native Wayland or with
`RETRO_TERM_TEST_SOFTWARE_GPU=1`):

- T4 in `fixed_grid_readback`: `AnimatedCrt` at frame `f0` where `flicker_gain(f0) == 1.0` equals
  the `StaticCrt` readback byte for byte. Find `f0` by search over `0..64` in the test.
- T5: frame `f1` with gain `0.955`: every pixel ≤ the static pixel per channel, the sampled grey
  pixel `(64, 32)` is darker by at least 1 level, black border stays `(0,0,0)`; rendering `f1`
  twice gives identical bytes (determinism); landmark from `display_point` still bright.

## Feasibility outcome rules

Use exactly one outcome in the index and in this plan:

1. **Implemented; performance unverified** (expected on this host). Preset exists, docs say
   "experimental; performance unverified". Do not write "delivered", "supported", "passing".
   Task 10 must keep `animated-crt` gated (see Q6) or excluded.
2. **Feasible at the phase limit (1.40×)**: a full measured run with `--candidate-animated` on the
   matched host returns `passed` for all 8 scenarios. Record report path and the per-scenario
   ratios. Release still needs 1.20× through task 10.
3. **Infeasible / deferred**: a valid run fails any metric. Optimize or simplify (smaller gain set,
   gain only on non-blink frames is **not** allowed — that is benchmark-specific). If it still fails,
   remove `animated-crt` from `PRESETS` and the docs, keep the effect code only if the user agrees,
   and record the failing scenario and metric.
4. Unverified metrics (startup/idle GPU, lifecycle, see 05 decision) stay unverified; they never
   count as a pass. Compare the animated report with a static report from the **same** run session.
5. Any independent timer (rolling scanline, persistence tail) is out of this task. When Alacritty's
   idle GPU is zero, a continuous timer is infeasible by construction (B2); record that sentence
   if the user asks about it.

## Acceptance criteria

- Static CRT and plain output are unchanged: existing GPU assertions, perf probe, and fixtures pass
  unchanged; static mode never writes the params buffer after creation.
- Animated mode adds no frame, no wakeup, and no texture: scheduler code and tests unchanged; idle
  with a non-blinking cursor submits no frames.
- Effect parameters are fixed in code (tested range). No user-tunable amplitude.
- Deterministic tests T1–T12 exist; headless ones pass in `cargo xtask check`.
- Perf can measure the animated preset (`--candidate-animated`), report mode `animated-crt`.
- Docs and index state the outcome with the rules above. No claim of a perf pass without a report.

## Verification

Exists now:

```sh
rtk cargo xtask check
rtk cargo build --locked --release -p retro-app --bin retro-term
rtk cargo test --locked -p retro-renderer --test gpu_render -- --ignored --nocapture
# software adapter only, correctness only:
RETRO_TERM_TEST_SOFTWARE_GPU=1 rtk cargo test --locked -p retro-renderer --test gpu_render -- --ignored --nocapture
rtk cargo xtask perf --candidate target/release/retro-term --baseline alacritty --suite benchmarks/scenarios.toml --output target/perf/static.json
```

Added by step 6 (does not exist before it):

```sh
rtk cargo xtask perf --candidate target/release/retro-term --baseline alacritty --suite benchmarks/scenarios.toml --output target/perf/animated-preflight.json --preflight --candidate-animated
rtk cargo xtask perf --candidate target/release/retro-term --baseline alacritty --suite benchmarks/scenarios.toml --output target/perf/animated.json --candidate-animated
```

Expected on this host: every `perf` command exits 3 (Alacritty missing). Record that; it is not a
failure of the effect and not a pass. A visual smoke check (`cargo xtask run` with
`preset = "animated-crt"` in a config file) needs a person on native Wayland; record it as not run
if nobody does it.

## Out of scope

Independent animation timers, rolling scanline, phosphor persistence/history textures, noise
(unless Q1 adds it), jitter, user-tunable amplitudes, settings UI controls (task 11), release-gate
wiring on PR #9 (task 10 F1), file watcher.

## Questions Q1–Q9 (decided; kept for context)

Q1. Which effect first? (a) flicker only; (b) flicker + noise; (c) phosphor persistence;
(d) rolling scanline. **Recommend (a)**: zero extra texture, zero extra frames, probe can match it exactly.

Q2. Config exposure? (a) `effects.animation: bool` + preset `animated-crt`; (b) per-effect keys
(`effects.flicker = 0.0..=0.05`); (c) preset only, no key. **Recommend (a)**: one tested range, the
settings UI (task 11) gets one toggle, per-effect keys can come later.

Q3. When does the effect run? (a) only on frames already needed; (b) bounded tail timer after each
change (e.g. ≤ 300 ms at ≤ 30 Hz); (c) continuous timer with a cap. **Recommend (a)**. (c) fails
idle when the baseline GPU is zero (B2). (b) only after (a) is measured.

Q4. Idle behavior? (a) frozen at the last frame's gain, no wakeups; (b) settle to gain 1.0 with one
extra frame. **Recommend (a)**: (b) adds a frame per change and a deadline.

Q5. Time source for tests? (a) frame counter (`u32`) set on the renderer, gain from a pure function;
(b) injected elapsed time (`Duration`) from the app clock; (c) a clock trait. **Recommend (a)**:
no clock needed for needed-frame effects; (b) only if Q3 becomes (b)/(c).

Q6. Ship `animated-crt` in `assets/presets` now, and gate it? (a) ship it, label "experimental;
performance unverified", and require task 10 F1 to add it to `benchmarks/release-gate.toml` (the
release then blocks unless `allow_failed_gate` is used and noted); (b) ship it but keep it out of the
gate list; (c) do not ship a preset; key only; (d) code behind a non-default cargo feature.
**Recommend (a)**: honest gating, no silent ungated preset. (b) contradicts "every shipped preset passes".

Q7. Perf probe for animated frames? (a) probe knows the discrete gain set and requires one gain per
frame; (b) colour tolerance band; (c) estimate gain from the background. **Recommend (a)**: exact,
unit-testable (T11). (b) is ambiguous: markers differ by 1 blue level.

Q8. Perf CLI shape? (a) `--candidate-animated` + optional suite key `animated_config`; (b) generic
`--candidate-config <path>`; (c) `--candidate-mode <name>` replacing `--candidate-plain`.
**Recommend (a)**: keeps `--candidate-plain` that PR #9's `release.rs` uses; smallest merge conflict.

Q9. `animation = true` with `crt = false`? (a) valid, renders plain; (b) validation error.
**Recommend (a)**: `--plain` must work with the animated preset; (b) would make that an error.

## Decisions on Q1–Q9 (user)

The user accepted every recommended option.

- Q1 (a): flicker only.
- Q2 (a): `effects.animation: bool` and preset `animated-crt`, labelled "experimental; performance unverified".
- Q3 (a): the effect runs only on frames the app already draws. Scheduler unchanged.
- Q4 (a): idle stays frozen at the last frame's gain. No wakeups.
- Q5 (a): `u32` frame counter in `retro-app`; gain from the pure `flicker_gain(frame)`.
- Q6 (a): ship the preset as experimental; task 10 F1 must add `animated-crt` to the release gate list.
- Q7 (a): the probe knows the discrete gain set; every probe point of a frame matches one gain.
- Q8 (a): `xtask perf --candidate-animated` plus optional suite key `animated_config`.
- Q9 (a): `animation = true` with `crt = false` is valid and renders plain.

## Contradictions found while planning

- C1. `docs/AI/README.md` lists 09's prerequisite as "08 completed and resource suite passing". The
  resource suite is unverified for 06–08 (Alacritty missing). The user chose to proceed with "perf
  unverified" (D4). The index row must not imply the prerequisite was met.
- C2. Old plan text says "Every shipped animation passes CPU/RAM/GPU <= 1.20" while this phase uses
  1.40 (D3). This plan measures at 1.40; the 1.20 release rule stays with task 10.
- C3. The old plan names `crates/app/src/scheduler.rs` as a changed file. With Q3 (a) it does not change.
- C4. Task 06 required "no time uniform" for the static pass. This task adds a gain uniform that is
  constant `1.0` in static mode; static output stays identical (tested by T4).
- C5. PR #9 (task 10) is based on `feat/static-crt`, not on this stack. Its `perf.rs` removes
  `candidate_config` and adds `--limit`; its gate file has `candidate_plain: bool` per preset and
  checks `candidate_mode` as `plain`/`static-crt` only. Merging #9 with this task needs: keep
  `candidate_config`/`animated_config`, and a gate preset shape that can say `animated-crt` (F1).
- C6. The architecture tree lists `crates/renderer/tests/rendering.rs` and
  `crates/app/tests/scheduler.rs`; the code uses `gpu_render.rs` and scheduler unit tests. Not a blocker.
- C7. Task 11 (settings UI) is planned in parallel from the same base. It must use the key chosen in
  Q2 (`effects.animation` if (a)); tell the task 11 planner the answer.

## Progress

- Planning done on base `a03bd1b`.
- Step 1: Q1–Q9 recorded above.
- Step 2: `FLICKER_GAINS`, `flicker_gain`, `RenderMode::AnimatedCrt`; T1–T3 pass.
- Step 3: `Crt::new(.., animated)`, `COPY_DST`, `params.z` gain, `Crt::set_frame` (writes only when
  animated), `set_animation_frame` on `GridRenderer` and `SurfaceRenderer`; T4–T5 pass on the
  Radeon 890M (RADV).
- Step 4: `effects.animation`, `animated-crt` preset, example file, `docs/configuration.md`; T6–T8 pass.
- Step 5: `render_mode` match, `State.animation_frame` (incremented after `Frame::Presented`); T9 passes.
  `scheduler.rs` unchanged.
- Step 6: `--candidate-animated`, `animated_config`, `retro-term-animated.toml`, gain-aware probe,
  `docs/performance.md`; T10–T12 and the Rust perf tests pass.
- Steps 7–8: see Handoff. Feasibility outcome: **1. Implemented; performance unverified.**

## Deviations

- V1. `FLICKER_GAINS` is `[1.0, 0.975, 0.955]`, not `[1.0, 0.985, 0.97, 0.955]`. With the planned
  gains, T11 failed: `[51, 91, 130]` at gain 0.97 gives the same bright-phase bytes as
  `[51, 91, 129]` at gain 1.0. The plan says to change the gains, not the markers.
- V2. T11 checks the bright scanline phase only. The dark phase (0.88) already collides between
  markers one blue level apart in static mode (`[51, 91, 131]` and `[51, 91, 130]`), so a check over
  both phases fails even with gain 1.0 only. `visible_crt` locates the marker with the bright phase.
- V3. `crt_colors(color, gain=1.0)` takes one gain, not a tuple; callers loop over the gains so every
  probe point of a frame uses the same gain. Animated evidence records `"gain"`; static evidence is
  unchanged.
- V4. T12 uses `FLICKER_GAINS[-1]` (0.955) for the synthetic dark frame and mixes gain 1.0 into
  the lower half for the rejection case (0.97 is no longer a gain).
- V5. T5's landmark threshold is `> 150` for the animated frame (static uses `> 175`), because the
  gain darkens the red landmark.
- V6. The report's `identity` gets `animated_config_sha256` only in animated mode; the static and
  plain reports keep their keys. Suite key `animated_config` is skipped when absent.

## Handoff

- Base: `a03bd1bdb79bd741aec8a0e119d364fd5292dba6` on `08-tabs-and-splits`
- Worktree: `.worktrees/09-optional-animation`
- Changed files:
  - Renderer: `crates/renderer/src/{effects.rs,lib.rs,shaders/crt.wgsl}`, `crates/renderer/tests/gpu_render.rs`.
  - Config: `crates/app/src/config.rs`, `crates/app/tests/config.rs`, `assets/presets/animated-crt.toml` (new),
    `assets/config.example.toml`, `docs/configuration.md`.
  - App: `crates/app/src/app.rs` (`render_mode`, `animation_frame`, T9 test module).
  - Perf: `xtask/src/perf.rs`, `xtask/src/perf/{run.py,wayland.py,test_perf.py}`, `xtask/tests/perf.rs`,
    `benchmarks/scenarios.toml`, `benchmarks/fixtures/retro-term-animated.toml` (new), `docs/performance.md`.
  - Docs: this plan, `docs/AI/README.md` (09 row).
- Decisions and deviations: D1–D9; Q1–Q9 all (a); deviations V1–V6.
- Commands run and results:
  - `cargo xtask check`: fmt, check, clippy, test, deny, actionlint, zizmor passed; ignored-tests not_run (1).
  - `cargo build --locked --release -p retro-app --bin retro-term`: exit 0.
  - `cargo test --locked -p retro-renderer --test gpu_render -- --ignored --nocapture`: 1 passed on
    AMD Radeon 890M (RADV STRIX1, Mesa 26.2.7), native Wayland.
  - `cargo xtask perf ... --output target/perf/animated-preflight.json --preflight --candidate-animated`:
    exit 3, status `unverified`, `candidate_mode` `animated-crt`, reason "baseline executable unavailable: alacritty".
  - `timeout 15 cargo xtask run`: ran until the timeout (exit 124), no error output. It uses the
    user configuration (static by default), so it does not show the animated preset.
- Not run, and why:
  - Full `perf --candidate-animated` and static comparison: Alacritty 0.17.0 not installed (exit 3 expected).
  - Visual smoke check of `preset = "animated-crt"`: needs a person on native Wayland with a config file.
  - `RETRO_TERM_TEST_SOFTWARE_GPU=1` GPU run: hardware run done instead.
- Artifacts: `target/perf/animated-preflight.json`, `target/xtask/check.json`.
- Feasibility outcome: **Implemented; performance unverified.** Not delivered, not supported, not passing.
- Blockers and next step: install Alacritty 0.17.0, run static and `--candidate-animated` perf in the
  same session at 1.40×. Task 10 F1 must gate `animated-crt` (Q6). Task 11 uses `effects.animation`.
