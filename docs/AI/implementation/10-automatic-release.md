# Automatic packaging and release

Status: pending. Planning only; implementation has not started.
Dependencies: 08 completed; 09's feasibility outcome recorded. The full suite must pass for every included preset. 02/05 supply CI and measurement tooling.

## Context and shared rules

Read [the architecture](../architecture/retro-terminal.md) and [the execution index](../README.md) before work.
Linux first. Reuse alacritty_terminal. Three product crates plus development-only xtask.
CPU, RAM, and GPU must each stay within 1.20 times Alacritty for every shipped preset under matched tests.
Static CRT is default; animation is opt-in but gets no budget exception.
Use a task worktree and the agreed base. Do not commit, push, or open a PR unless asked.
Use project tooling only. Report skipped/unavailable checks; never invent a passing result.
Record progress and handoff here under docs/AI/implementation/. This plan overrides the old plan-directory convention.

## Goal

Implement the GitHub release-PR flow, with publication gated on the exact packaged binary.

## Files

- .github/workflows/{release-please,release,performance}.yml.
- release-please-config.json, .release-please-manifest.json, dist-workspace.toml.
- xtask/src/release.rs, package/release contract tests, CHANGELOG.md.
- docs/releasing.md and release-support/performance documentation.

## Steps

1. Configure one inherited application version and changelog; distribute the app, not xtask/internal libraries. Validate Release Please workspace/lockfile updates.
2. Use pinned dist for artifact building and Release Please for release PRs, tags, and draft creation. Enable draft and forced tag creation with a version supporting those options.
3. On release-PR merge, call the reusable release workflow with the returned tag/SHA directly. Verify version/tag/SHA agreement; do not depend on implicit bot-triggered event chains.
4. Build/package the tagged revision with locked dependencies. Include licenses/resources/example configuration/checksums and document supported Linux targets/runtime libraries.
5. Test the actual packaged executable and bind its performance reports to its checksum and source/build metadata.
6. Publish the existing draft only after all checks pass. Isolate credentials from artifact execution/GPU jobs.
7. Configure a scoped bot identity, repository rules, and GPU runner when external setup is authorized. No unavailable infrastructure may be treated as a passing gate.
8. Rehearse in dry-run mode or an authorized test repository. Cover missing/stale/failing reports, SHA/hash mismatch, unavailable runners, duplicate events, and retries.
9. Document same-tag retry and corrective-release recovery. Never move published tags or replace different released bytes.

## Acceptance criteria

- Release PRs receive normal CI and contain reviewable version/changelog changes.
- Merge initiates the pipeline; successful checks publish without another manual step.
- Failures leave an unpublished draft. CPU/RAM/GPU gates apply to all included presets.
- Artifact identity is preserved from measurement through upload.
- Prepared local configuration is distinguished from actually enabled infrastructure.

## Verification

Commands below are to run after this task and its dependencies supply the tooling.
Do not claim that planned commands already exist.

```sh
cargo xtask check
cargo xtask package --check
```

Run the perf command from 05 on the packaged binary path emitted by package --check; record that exact path/checksum. Local package checks never publish. No bot/runner creation, push, or release is authorized merely by planning.

## Handoff

Record base SHA, worktree path, changed files, decisions, exact commands/results, unrun checks, artifact locations, and blockers.
Update this plan and the index status only when supported by evidence. Do not mark performance-dependent work complete on headless checks alone.

