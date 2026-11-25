# Optional CRT animations

Status: pending. Planning only; implementation has not started.
Dependencies: 08 completed and resource suite passing.

## Context and shared rules

Read [the architecture](../architecture/retro-terminal.md) and [the execution index](../README.md) before work.
Linux first. Reuse alacritty_terminal. Three product crates plus development-only xtask.
CPU, RAM, and GPU must each stay within 1.20 times Alacritty for every shipped preset under matched tests.
Static CRT is default; animation is opt-in but gets no budget exception.
Use a task worktree and the agreed base. Do not commit, push, or open a PR unless asked.
Use project tooling only. Report skipped/unavailable checks; never invent a passing result.
Record progress and handoff here under doc/AI/implementation/. This plan overrides the old plan-directory convention.

## Goal

Evaluate optional temporal effects under exactly the same resource limit as the static preset.

## Files

- crates/renderer/src/effects.rs and relevant shaders.
- crates/app/src/scheduler.rs.
- Animated preset definitions, deterministic render tests, benchmark scenarios and results.

## Steps

1. Add one effect at a time; begin by evaluating it on frames already needed for terminal changes.
2. Only add independent visual timers when measurements justify them. Bound histories/caches and pause unavailable-window rendering.
3. Keep static as default. Document adaptive behavior as normal app behavior, not benchmark-only behavior.
4. Use fixed time/input for render tests and measure every shipped preset, including non-blinking idle.
5. Optimize, simplify, or defer failing effects; never loosen the ceiling or omit failing scenarios.
6. Record a clear feasibility outcome if no animated preset can pass. Do not label the requested animation delivered in that case.

## Acceptance criteria

- Every shipped animation passes CPU/RAM/GPU <= 1.20 times Alacritty.
- Zero baseline GPU work grants zero additional GPU work.
- An infeasible effect remains explicitly deferred; the architecture's acceptance rule stays intact.

## Verification

Commands below are to run after this task and its dependencies supply the tooling.
Do not claim that planned commands already exist.

```sh
rtk cargo xtask check
rtk cargo build --locked --release -p retro-app --bin retro-term
rtk cargo xtask perf --candidate target/release/retro-term --baseline alacritty --suite benchmarks/scenarios.toml --output target/perf/report.json
```

Continuous idle flicker is not guaranteed. No performance exception for opt-in presets.

## Handoff

Record base SHA, worktree path, changed files, decisions, exact commands/results, unrun checks, artifact locations, and blockers.
Update this plan and the index status only when supported by evidence. Do not mark performance-dependent work complete on headless checks alone.

