# Static CRT rendering

Status: pending. Planning only; implementation has not started.
Dependencies: 05 passed for the plain renderer.

## Context and shared rules

Read [the architecture](../architecture/retro-terminal.md) and [the execution index](../README.md) before work.
Linux first. Reuse alacritty_terminal. Three product crates plus development-only xtask.
CPU, RAM, and GPU must each stay within 1.20 times Alacritty for every shipped preset under matched tests.
Static CRT is default; animation is opt-in but gets no budget exception.
Use a task worktree and the agreed base. Do not commit, push, or open a PR unless asked.
Use project tooling only. Report skipped/unavailable checks; never invent a passing result.
Record progress and handoff here under docs/AI/implementation/. This plan overrides the old plan-directory convention.

## Goal

Add the default static CRT appearance while preserving the resource ceiling.

## Files

- crates/renderer/src/effects.rs and shaders/crt.wgsl.
- crates/app/src/scheduler.rs.
- Static preset assets and rendering/scheduler tests.
- benchmarks/scenarios.toml and docs/performance.md.

## Steps

1. Begin with inexpensive tint and scanline operations. Retain a full effects bypass.
2. Cache reusable text/effect surfaces, bound allocations, and share resources across the scene.
3. Handle cursor/selection/configuration/resize invalidation as well as terminal damage; do not apply row-only damage blindly to effects with neighboring-pixel dependencies.
4. If distortion is added, keep pointer/selection hit testing consistent with its transform.
5. Test static idle, bypass, resize/recovery, and selection/cursor updates with fixed render inputs.
6. Measure both bypass and static modes. Optimize or simplify any effect that fails before proceeding.

## Acceptance criteria

- Static is the default; it introduces no independent animation timer.
- The static preset passes CPU/RAM/GPU <= 1.20 times Alacritty in required scenarios.
- Readability, input mapping, and correctness remain intact.

## Verification

Commands below are to run after this task and its dependencies supply the tooling.
Do not claim that planned commands already exist.

```sh
cargo xtask check
cargo build --locked --release -p retro-app --bin retro-term
cargo xtask perf --candidate target/release/retro-term --baseline alacritty --suite benchmarks/scenarios.toml --output target/perf/report.json
```

No animated history textures or settings editor. Run the GPU correctness command defined in 04 as well.

## Handoff

Record base SHA, worktree path, changed files, decisions, exact commands/results, unrun checks, artifact locations, and blockers.
Update this plan and the index status only when supported by evidence. Do not mark performance-dependent work complete on headless checks alone.

