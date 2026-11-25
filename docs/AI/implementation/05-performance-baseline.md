# Performance harness and baseline gate

Status: pending. Planning only; implementation has not started.
Dependencies: 04 completed.

## Context and shared rules

Read [the architecture](../architecture/retro-terminal.md) and [the execution index](../README.md) before work.
Linux first. Reuse alacritty_terminal. Three product crates plus development-only xtask.
CPU, RAM, and GPU must each stay within 1.20 times Alacritty for every shipped preset under matched tests.
Static CRT is default; animation is opt-in but gets no budget exception.
Use a task worktree and the agreed base. Do not commit, push, or open a PR unless asked.
Use project tooling only. Report skipped/unavailable checks; never invent a passing result.
Record progress and handoff here under docs/AI/implementation/. This plan overrides the old plan-directory convention.

## Goal

Measure the plain prototype against Alacritty and decide whether the proposed renderer fits the budget.

## Files

- xtask/src/perf.rs and harness unit/integration tests.
- benchmarks/scenarios.toml and fixtures/configurations.
- .github/workflows/performance.yml.
- docs/performance.md and renderer decision record.

## Steps

1. Implement the maintained perf CLI specified below. Match workload, font, cell area, scale, cursor settings, scrollback, display backend, run length, and release configuration.
2. Record baseline version, source/build identity, hardware/driver metadata, raw samples, and pass/fail/unverified results.
3. Measure emulator CPU time, steady/peak RSS, and attributable GPU busy time separately. Keep child workload costs separate and avoid duplicated GPU client accounting.
4. Test ratio evaluation, zero and sub-resolution baselines, missing counters, counter semantics, workload completion, and metadata/artifact identity.
5. Cover startup, idle with/without blinking cursor, typing, output, full-screen redraw, resize, and lifecycle churn. Add multi-pane cases when 08 exists.
6. Run sequential measurements on a controlled Linux GPU machine. Missing hardware/counters leave the gate unverified. Prepare a serialized reusable workflow; remote runner setup requires the necessary infrastructure.
7. If the plain prototype exceeds 1.20 times Alacritty on any required metric/scenario, investigate rendering/font/backend overhead before feature expansion. Record any stack change and rerun affected checks.

## Acceptance criteria

- CPU, RAM, and GPU each have an independent result; no failing scenario is averaged away.
- Zero/noisy baselines receive no invented allowance.
- A real plain-renderer baseline is recorded, or progress is explicitly blocked by unavailable measurement infrastructure.
- Reduced output correctness or unbounded latency cannot make a test pass.

## Verification

Commands below are to run after this task and its dependencies supply the tooling.
Do not claim that planned commands already exist.

```sh
cargo xtask check
cargo build --locked --release -p retro-app --bin retro-term
cargo xtask perf --candidate target/release/retro-term --baseline alacritty --suite benchmarks/scenarios.toml --output target/perf/report.json
```

No made-up numbers or GPU claims based on software rendering. Complete the baseline decision before 06.

## Handoff

Record base SHA, worktree path, changed files, decisions, exact commands/results, unrun checks, artifact locations, and blockers.
Update this plan and the index status only when supported by evidence. Do not mark performance-dependent work complete on headless checks alone.

