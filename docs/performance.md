# Performance baseline

Task 06 applies the 1.40× Alacritty ceiling to emulator CPU time, steady and
peak RSS, and attributable GPU engine time in each scenario. Measure default
static CRT and the plain bypass separately. The suite also checks that
expected output becomes visible before its deadline.

## Measurement host

Use the Task 04 AMD Radeon 890M machine and its native KDE Wayland session. Use
a dedicated, unscaled SDR output with a fixed refresh rate, US keyboard layout,
default KDE window gestures, and both windows fully visible. Close unrelated
windows and avoid interacting with the session during measurements.

Install the released [Alacritty 0.17.0](https://github.com/alacritty/alacritty/releases/tag/v0.17.0),
Source Code Pro Regular, Python 3.11 or later, PyGObject, GStreamer with the
PipeWire source and GstVideo introspection, and Fontconfig's `fc-match`. Start
the run from the desktop session with access to its D-Bus socket, PipeWire, and
hardware DRM devices. Provide these settings in the environment.

| Setting | Value to record |
| --- | --- |
| `RETRO_PERF_ALACRITTY_BUILD` | Package/build identity or source revision and build flags for the pinned binary |
| `RETRO_PERF_REFRESH_HZ` | Fixed output refresh rate |
| `RETRO_PERF_POWER` | Power source, power/profile setting and other frequency conditions |
| `RETRO_PERF_DRM_RESOLUTION_NS` | Supported busy-time error bound in nanoseconds across the selected engine sum |
| `RETRO_PERF_DRM_RESOLUTION_SOURCE` | Driver documentation or measured evidence supporting that bound |

If DRM error bounds are unavailable, omit both DRM variables. The harness retains
raw DRM counters and leaves GPU comparisons unverified.

On amdgpu, use `RETRO_PERF_DRM_RESOLUTION_NS=100000`. amdgpu sums, per engine,
the scheduler fence "finished" minus "scheduled" timestamps in nanoseconds
(`amdgpu_ctx_fence_time` in `drivers/gpu/drm/amd/amdgpu/amdgpu_ctx.c`). The
100 µs bound allows about 1 µs of signalling latency for up to 100 jobs in a
scenario interval.

```sh
cargo build --locked --release -p retro-app --bin retro-term
cargo xtask perf --candidate target/release/retro-term --baseline alacritty --suite benchmarks/scenarios.toml --output target/perf/preflight.json --preflight
cargo xtask perf --candidate target/release/retro-term --baseline alacritty --suite benchmarks/scenarios.toml --output target/perf/static.json
cargo xtask perf --candidate target/release/retro-term --baseline alacritty --suite benchmarks/scenarios.toml --output target/perf/plain.json --candidate-plain
cargo xtask perf --candidate target/release/retro-term --baseline alacritty --suite benchmarks/scenarios.toml --output target/perf/animated.json --candidate-animated
```

The candidate runs with `--config benchmarks/fixtures/retro-term.toml` (the suite key
`candidate_config`), so a user configuration file does not change the result.
`--candidate-animated` uses `benchmarks/fixtures/retro-term-animated.toml` (the suite key
`animated_config`) and reports `candidate_mode = "animated-crt"`. The capture probe accepts a
frame only when every probe point matches the same flicker gain. The animated preset is
experimental; its performance is unverified until a valid report exists.

Preflight checks the baseline version and records binary, source, suite, and font
identity. It checks that the desktop portal advertises the keyboard, pointer,
and output capture capabilities. The measured run requests those permissions
and checks the actual PipeWire stream. Select the dedicated output in the portal
dialog. The driver uses that stream for both terminals. The suite requires
matching effective terminal rows, columns, and pixel cell area. Calibrate
Alacritty's geometry and font offsets on the host if reported grids differ.
Make calibration changes before collecting comparison results.

## Reports

Use `--scenario output` with the same command to diagnose scrollback.
The focused run records whether Alacritty forwards the injected wheel event
to its PTY with mouse reporting enabled. A focused run cannot pass the gate.

The checked-in suite runs 15 alternating-order pairs per scenario. It
checks ordered display markers, idle cursor state, typed bytes, resize delivery,
and cleanup after window close. Running preflight alone produces no measured
results. If a capability is missing, consult the report before interpreting
any individual result.

The report records the candidate mode. Static CRT capture checks account for
the projected grid, tint, and scanline phases. Plain and Alacritty capture
checks use the rectangular grid and original colors. Compare each report to
the Alacritty measurements from its own run and to the comparable results
in the accepted [Task 05 baseline](decisions/05-renderer-baseline.md). Keep
any metric with missing measurements or unresolved uncertainty open.

The JSON report retains raw samples for each terminal as well as the fixture's
separate CPU and RSS samples. Emulator CPU uses the kernel's process-wide
scheduler clock in nanoseconds, which includes exited threads. Steady
RSS is the median of interval samples. Peak RSS is the largest sampled value.
GPU totals sum busy-time deltas across reported engines after deduplicating
DRM clients by PCI device and client ID. A driver may omit an engine until the
client first uses it, so a missing engine counts as 0 ns. Workload latency includes the shared
capture path. Missing or inconsistent evidence prevents a pass.
Each metric is gated on the median pair. The median pair passes when its
uncertainty range is wholly within 1.40, fails when wholly above it, and is
unverified otherwise. With an even pair count, both middle pairs must support the same decision.
When both terminals use less than 1 ms of CPU in an interval, the CPU
comparison passes as negligible, because scheduler noise exceeds the 1.40 ratio.

Measured runs return 0 only when every required scenario passes. Exit 1 covers
failed or unverified comparisons, exit 2 covers invalid command or suite input,
and exit 3 covers missing required executables. The lifecycle case currently
lacks a final accounting source for DRM clients destroyed at close. The result
remains unverified until a supported source can account for the close interval.

## Reusable workflow

A trusted protected-branch caller can use `.github/workflows/performance.yml`.
Assign a unique runner label to the physical GPU and pass it as `gpu_label`.
Run the runner with access to the controlled desktop session and the prerequisites
above. The reusable workflow serializes measurements by label and uploads
available JSON reports. Portal access must be available for the job's session.

See the [renderer baseline decision](decisions/05-renderer-baseline.md) for
the current result.

## Multi-session scenarios (planned; harness not implemented)

Task 08 defines these scenarios. The harness accepts only the 8 single-pane scenarios
today. One tab with one pane renders the same scene as before tabs, so the
single-pane suite stays valid.

Matched rule: equal total visible cells and equal per-session grids in both legs;
same fixture, font, scrollback, and cursor settings as the single-pane suite.
Alacritty runs as one process with one window per session
(`alacritty msg create-window -e <fixture>`); a hidden tab is a minimized window.

| Scenario | Candidate | Baseline | Measures |
| --- | --- | --- | --- |
| `split-idle` | 1 window, 2 panes side by side, non-blinking, 4 s | 2 windows, same grids | No frames, CPU/RSS/GPU at idle |
| `split-output` | 2 panes, 300-line bursts ×4 alternating | 2 windows | Output path with two visible sessions |
| `hidden-output` | 2 tabs, hidden tab outputs 300 lines ×4, then reveal | 2 windows, one minimized, then restored | Hidden work is parse-only; reveal shows the final frame |
| `pane-lifecycle` | 20× split + close | 20× create-window + close | Reaped children, RSS returns, no growth |

The harness needs a separate suite or scenario list, a fixture server that accepts
N connections with per-session identity, one marker color per session, key
injection for the shortcuts, and a CRT probe that maps each pane rect through the
window-level warp.
