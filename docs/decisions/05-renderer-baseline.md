# Plain-renderer baseline decision

Status: accepted. Task 06 is unblocked.

The resource limit for the next development phase is 1.40 times Alacritty.
The long-term 1.20 target is unchanged in the architecture. The numbers below
are the base that later tasks must not regress.

## Host

AMD Radeon 890M, Mesa 26.2.7, KWin 6.8.3, KDE Wayland. DP-1 at 3440×1440,
59.97 Hz, 100% scale, SDR. AC power with the Performance platform profile.
Fedora Alacritty 0.17.0, Source Code Pro Regular, 100×26 grid with 8×19-pixel
cells in both terminals. GPU bound: 100 µs for the engine sum (see
[performance measurements](../performance.md)).

## Results

Median ratio over 15 pairs, retro-term divided by Alacritty. Report:
`target/perf/report-median.json`.

| Scenario | CPU | GPU | Steady RSS | Peak RSS |
| --- | --- | --- | --- | --- |
| startup | 1.06 | unverified | 0.83 | 0.84 |
| idle | below 1 ms floor | unverified (both ≈ 0 ns) | 0.85 | 0.85 |
| blink | 0.97 | 0.46 | 0.85 | 0.85 |
| typing | 1.35 | 0.94 | 0.85 | 0.85 |
| output | 1.29 | 1.13 | 0.85 | 0.86 |
| redraw | 1.04 | 1.28 | 0.85 | 0.85 |
| resize | 1.33 | 1.20 | 0.86 | 0.86 |
| lifecycle | 1.02 | unverified | 0.85 | unverified |

## Known gaps

- Startup GPU: the GPU client opens after the first sample.
- Idle GPU: both terminals stay below the 100 µs bound.
- Lifecycle peak RSS and GPU: the counters are gone after exit.

## Next phase

CPU in typing, output and resize, and GPU in redraw are the largest costs.
They are candidates for optimization before the limit returns to 1.20.
