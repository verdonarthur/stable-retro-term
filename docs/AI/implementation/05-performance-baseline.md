# Performance harness and baseline gate

Status: done. The baseline is recorded in `docs/decisions/05-renderer-baseline.md`.
Requires Task 04. Task 06 is unblocked: the user accepted a 1.40 limit for the next phase.

## Goal and acceptance criteria

Compare the plain terminal with a released Alacritty build on matched native Wayland workloads.
For every required scenario, candidate CPU time, steady RSS, peak RSS, and attributable GPU busy time must each be at most 1.40 times the corresponding baseline. (The user raised this from 1.20 after the first full measurement.)
Verify visible output, workload completion, and bounded response time before passing a scenario.
Record a real hardware baseline and a renderer decision, or record the specific blocker. Keep unsupported or inconclusive measurements unverified.

## Context

Read [the performance requirements](../architecture/retro-terminal.md#performance-acceptance-and-measurement), [Task 04's handoff](04-plain-terminal.md#handoff), and [the execution index](../README.md).
Planning used Task 04 commit `24d7eb7bad7a520184c36a7b6df8489c4a14d021`. Start the implementation worktree from the agreed base containing Task 04. Prefer the merged Task 04 revision if it is available when implementation starts.

- `crates/app/src/main.rs` accepts help and development diagnostics. `crates/app/src/app.rs` opens an 800×500 logical-pixel window, uses the system monospace font, and starts a PTY with 10,000 lines of scrollback. Window events drive input and resize. Terminal updates drive redraws via `crates/app/src/scheduler.rs`, which uses a 500 ms blink interval.
- `crates/terminal/src/session.rs` already accepts an explicit PTY program and arguments at the library boundary. Its integration tests cover input, resize, exit, and final output. The app currently uses the default shell. The renderer's GPU readback test checks its drawing path separately from a desktop window.
- `xtask/src/main.rs` dispatches development commands. `xtask/src/process.rs` and `xtask/src/report.rs` contain process and reporting conventions. No performance command, suite, or performance workflow exists.

## Decisions

- **B1.** Use the AMD Radeon 890M KDE Wayland host from Task 04 for local measurements. Prepare a reusable workflow for a controlled GPU runner. Runner provisioning is outside this task. The Task 04 handoff confirms hardware Vulkan rendering. Counter coverage for both emulators is still unknown. The user did not select an Alacritty version. Pin a released version available for the host and record its executable hash, source/build provenance, and configuration before collecting baseline data.
- **D1.** Use one external Wayland driver for both terminals, a shared deterministic PTY fixture, and assertions on displayed output. Keep automation, sampling, and result evaluation in development tooling. Exercise the normal app input, PTY, resize, and rendering paths.
- **D2.** Keep the plan, progress, and handoff in this tracked file.

## Implementation steps

1. **Define the command and suite.** Add `perf` in `xtask/src/main.rs` and `xtask/src/perf.rs`. Put the versioned suite and configurations in `benchmarks/scenarios.toml` and `benchmarks/fixtures/`. Use the same parser for preflight and measurements. Resolve both binaries, validate required cases and bounds, and collect Git and binary identity, configuration and fixture hashes, hardware, kernel, graphics driver/backend, compositor, output scale, refresh rate, and power conditions. Add a `--preflight` mode that writes a report and checks capabilities without running the measured suite. Preserve existing xtask exit conventions and document results, including failed or unverified runs. Return success for a measurement only if all required results pass. Add headless tests in `xtask/tests/perf.rs` for command arguments, bad suites, required scenarios, metadata mismatch, and report output.
2. **Prove the shared Wayland path on the host.** Implement the test driver under `xtask/src/perf/`. First test the KDE session's XDG RemoteDesktop and ScreenCast support for input injection and frame observation. Use the same input actions and observation path for both windows. Verify focus, the expected client area, resize delivery, and window close. Keep capture work equal in each measured leg and account for it separately from emulator work. Record any unsupported action or capture capability as unverified for the affected scenarios. Add fake-driver tests for event order, focus loss, timeouts, and capture failure. Document the actual host setup and supported driver path in `docs/performance.md`.
3. **Implement deterministic workloads.** Put the shared PTY fixture under `xtask/src/perf/` and its data under `benchmarks/fixtures/`. Run the same fixture with both emulators. Use existing launch mechanisms where they can pass the executable and arguments reliably. If the candidate needs explicit command launch, pass it through `crates/app/src/main.rs` and `crates/app/src/app.rs` into the existing `SessionOptions::program`. Preserve the default shell path and test argument parsing and forwarding. Match font family and face, pixel cell dimensions, row/column count, scale, scrollback, cursor settings, and display refresh. Derive the Alacritty configuration from those requirements and verify the effective grid through the PTY and display driver. Reject a mismatch before comparing resource use.

   | Scenario | Completion or correctness evidence |
   | --- | --- |
   | Startup | Time from launch to the first expected visible fixture marker. |
   | Idle, cursor steady and blinking | Verify the configured cursor state and blink behavior over a fixed idle window. |
   | Typing | Inject keys through Wayland. Verify bytes received by the fixture and the visible response. |
   | Sustained output and scrollback | Check ordered fixture checkpoints, the final visible marker, and access to expected scrollback content. |
   | Full-screen redraw | Drive a fixed sequence of distinguishable screen states. Observe required frames before their deadlines and verify final output. |
   | Resize | Apply a fixed window-size sequence. Check the PTY-reported grid and the visible content after each required resize. |
   | Lifecycle churn | Repeat launch and window close with a live PTY. Check completion and cleanup of emulator and fixture processes. |

   Set common timing limits, warm-up windows, and input rates in the suite before measuring the candidate. Add deterministic tests for missing output, wrong order, late frames, wrong grid, and early fixture exit. Resolve or report any unsupported requirement. Defer multi-pane and hidden-tab coverage to Task 08.
4. **Collect counters and evaluate each metric.** Implement Linux sampling and evaluation under `xtask/src/perf/`. Start observation before launch where the case includes startup. Keep the emulator process identity stable across samples and account for child costs separately. Sample emulator user plus system CPU time across its threads and steady/peak RSS with a documented window and sampling resolution. Cross-check peak RSS against the process high-water mark when it covers the same interval. Hold the workload at a completion barrier long enough to take the final live sample where the scenario permits. For close or client churn, record counter coverage through exit and report incomplete intervals as unverified.

   Read DRM `fdinfo` for the same device in both legs. Deduplicate shared handles by device and client ID. Preserve raw per-engine values and document the aggregation rule and units. Follow the driver's supported busy-time semantics, including temporary counter decreases and client lifetimes. Treat incompatible counters or uncertain coverage as unverified. [Linux DRM client usage stats](https://docs.kernel.org/gpu/drm-usage-stats.html)

   Run the emulators sequentially with at least five paired repetitions, alternating order. Retain raw samples, timing, counter resolution, correspondence between pairs, and per-metric results. Define the repeatability rule before measuring. A conservative first rule is to pass when the observed uncertainty range is wholly within the limit, fail when it is wholly above it, and mark the overlap unverified. A proven zero baseline permits zero candidate use. A zero or small reading below measurement resolution requires a longer window, a better counter, or an unverified result. Add table tests for the exact 1.20 boundary, genuine and unresolved zeros, noise, missing/incompatible counters, duplicate handles, temporary decreases and resets, client loss, incomplete workloads, and independent metric/scenario failure.
5. **Add the reusable workflow.** Create `.github/workflows/performance.yml` with `workflow_call`, read-only repository permissions, and serialization per physical GPU. Use pinned actions and a trusted caller on the controlled runner. Build the exact candidate under test, validate the pinned Alacritty/configuration, run the same preflight and suite as local use, and retain reports and logs even on failure. Keep the workflow off the fork PR path. Validate it with the existing workflow linters through `cargo xtask check`. Document runner inputs and required session capabilities in `docs/performance.md`.
6. **Measure and decide.** On the agreed host, run preflight and then the suite with a clean native Wayland session and hardware rendering for both emulators. If a counter is unavailable, collect valid results for the remaining metrics and keep the full gate unverified. Save reports and raw samples under `target/perf/`. Record the measured environment, binary hashes, scenario outcomes, limitations, and reproduction commands in `docs/performance.md`. Add `docs/decisions/05-renderer-baseline.md` with the keep, investigate, or blocked decision and the evidence for it. Investigate failed metrics in the renderer, font path, or graphics backend before Task 06. Record any stack change and rerun the affected checks. Keep Task 06 blocked while any required result fails or remains unverified.

## Verification

Run these in the implementation worktree. Run `cargo xtask doctor` if a pinned tool is missing. The performance commands become available when this task implements them.

```sh
rtk cargo xtask check
rtk cargo test --locked -p xtask --test perf
rtk cargo build --locked --release -p retro-app --bin retro-term
rtk cargo xtask perf --candidate target/release/retro-term --baseline alacritty --suite benchmarks/scenarios.toml --output target/perf/preflight.json --preflight
rtk cargo xtask perf --candidate target/release/retro-term --baseline alacritty --suite benchmarks/scenarios.toml --output target/perf/report.json
```

For the last two commands, use the agreed hardware host with the pinned Alacritty on PATH. If implementation changes the production rendering path, also run the existing hardware correctness test on that host.

```sh
rtk cargo test --locked -p retro-renderer --test gpu_render -- --ignored --nocapture
```

## Out of scope

CRT effects, general user configuration, tabs and splits, X11 qualification, runner provisioning, and release wiring. Software rendering can support correctness checks. The resource gate requires hardware measurements.

## Handoff

- Planning reference: `24d7eb7bad7a520184c36a7b6df8489c4a14d021` on the Task 04 lineage.
- Planning checks: `rtk cargo xtask check` passed formatting, compilation, Clippy, and headless tests. One opt-in GPU test did not run. Check and doctor reports both returned exit 3 for missing `cargo-deny`, `actionlint`, and `zizmor`. The release build, host preflight, and performance suite did not run during planning.
- Implementation base: `24d7eb7bad7a520184c36a7b6df8489c4a14d021` on the Task 04 lineage. Worktree: `/home/arthur/.t3/worktrees/retro-term/t3-2d65ba68-6df0dd6de9959040`.
- Steps 1–4: implemented the perf command, suite, shared KDE portal/PipeWire driver, PTY fixture, Linux process and DRM sampling, and headless tests. The app accepts an explicit PTY command after `--`. The harness pins Alacritty 0.17.0 and checks matching grids and hardware render nodes. Final lifecycle peak RSS and GPU counters remain unverified because procfs and DRM fdinfo cannot supply them after teardown.
- Step 5: added the reusable performance workflow. Runner provisioning and workflow execution remain pending.
- Step 6: the sandbox preflight and measurement commands returned exit 3 because Alacritty was absent from PATH. Host preflight with the signature-checked Fedora `alacritty-0.17.0-2.fc45.x86_64` package, extracted under `/tmp`, confirmed version, font, hardware render-node access, and advertised portal capabilities. It returned exit 1 because output refresh rate and power conditions are still unset. The portal capture/input path and hardware measurements did not run. See `target/perf/preflight.json`, `target/perf/report.json`, and `target/perf/preflight-host.json`.
- Implementation checks: `rtk cargo test --locked -p xtask --test perf` and `rtk cargo build --locked --release -p retro-app --bin retro-term` passed. The full `cargo xtask check` run with the Task 04 pinned tools and writable advisory cache passed formatting, compile, Clippy, headless tests, deny, actionlint, and zizmor. The existing optional GPU correctness test did not run because production rendering code did not change. See `target/xtask/check.json`.
- Host run: re-ran preflight on DP-1 at 59.97 Hz, 100% scale and SDR, with AC power and the Performance platform profile. Preflight passed. The first full run reached all eight scenarios but exposed an 18-pixel Alacritty cell height versus 19 for Retro Terminal. Increased the Alacritty line offset by one pixel, settled the portal overlay before timing, and adjusted the scrollback stimulus. One retry timed out at the KDE chooser. The next full run confirmed matching 100×26 grids and returned exit 1 because results remain unverified. Reports are in `target/perf/report-host.json`, `target/perf/report-host-matched.json`, and `target/perf/report-host-retry.json`. See [the renderer decision](../../decisions/05-renderer-baseline.md) for the metric results.
- Scrollback follow-up: added `--scenario output`. Both terminals missed the marker with the continuous portal scroll gesture. In the focused probe, all five Alacritty runs received no wheel packet at the PTY. This leaves input delivery unverified. Reports: `target/perf/scrollback-host.json` and `target/perf/scrollback-probe.json`.
- Scrollback fix: `Driver.scroll` now sends portal `NotifyPointerAxisDiscrete` steps, then a smooth delta with `finish`. KWin treats positive portal steps as up, so the driver inverts the sign. In all five pairs, Alacritty got wheel-up (button 64), and both terminals showed the scrollback marker. The keyboard fallback is not needed. The scenario is still unverified only because Alacritty CPU use is at or below counter resolution and no GPU time counter is available. Report: `target/perf/scrollback-discrete.json`.
- Counter fix: emulator CPU now reads the process-wide `CPUCLOCK_SCHED` clock in ns, with exited threads included. GPU sampling treats an engine as 0 ns until amdgpu first reports it, and compares only device sets across legs. A headless test covers late engine reporting. Idle run (`target/perf/idle-gpu.json`): both legs now give valid GPU counters, but every pair reads 0 ns busy, so idle gives no variation to derive `RETRO_PERF_DRM_RESOLUTION_NS`. Idle CPU is now measurable (18–75 µs), but candidate/baseline ratios range 0.31–1.99 across pairs, so the result is unverified because of run-to-run noise. Open decision: the source for the GPU resolution bound, and how to gate idle CPU.
- Idle CPU floor: CPU passes as negligible when both legs use less than 1 ms (`CPU_FLOOR_NS`), with a headless test. Redraw run (`target/perf/redraw-gpu.json`): GPU counters are valid in both legs. Baseline GPU busy time ranges from 5.4 to 10.9 ms across the five pairs, so the user-chosen "redraw spread" bound would be about 5.5 ms. That is more than half the baseline, so the GPU gate could never decide. Candidate/baseline GPU ratios are stable at 0.67–0.76. Redraw CPU ratios are 1.09–1.20, and one pair is 1.205, so CPU is unverified because the result sits on the 1.20 limit. Open decisions: the GPU bound source, and how to treat the borderline redraw CPU.
- GPU bound: the user chose the driver-source bound, 100 µs for the engine sum (see `docs/performance.md`). Redraw run with the bound (`target/perf/redraw-gpu-bound.json`): CPU passed (0.82–1.11), RSS passed, GPU unverified with pair ratios 0.75–1.91. The previous redraw run had GPU 0.67–0.76 and CPU 1.09–1.20. Per-pair results are noisy between runs. Because the current rule needs every pair inside the limit, more pairs make a pass less likely, not more. Open decision: an aggregate repeatability rule before raising `repetitions`.
- Median gate and full run: compare now gates on the median pair, with 15 repetitions. Full suite with the 100 µs GPU bound (`target/perf/report-median.json`) returned `failed`. Median ratios: typing CPU 1.35 failed, output CPU 1.29 failed, resize CPU 1.33 failed, redraw GPU 1.28 failed. Passed: blink (all metrics), RSS everywhere except lifecycle peak (0.83–0.86), CPU in startup, idle, blink, redraw and lifecycle. Unverified: startup GPU (counter or resolution unavailable), idle GPU (both near 0 ns), output GPU 1.13 and resize GPU 1.20 (overlap), lifecycle peak RSS and GPU (no counters after exit). The harness now gives decisions. The failures are retro-term costs, not measurement gaps.
- Handoff: the user accepted the `report-median.json` numbers as the base for the next phase and raised the limit to 1.40. With 1.40, every measured median ratio passes. The remaining unverified items (startup GPU, idle GPU, lifecycle peak RSS and GPU) are accepted as known gaps. The full suite was not re-run after the limit change.
- PR review: checked the blink interval boundaries, moved the focused wheel probe outside timing, rejected absent GPU engine counters and candidates without execute permission, and corrected the report guide. The full project check passed with the pinned tools. The opt-in GPU test and hardware measurements did not run during review.
