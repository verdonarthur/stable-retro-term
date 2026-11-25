# Developer tools, worktrees, and CI

Status: pending. Planning only; implementation has not started.
Dependencies: 01 completed.

## Context and shared rules

Read [the architecture](../architecture/retro-terminal.md) and [the execution index](../README.md) before work.
Linux first. Reuse alacritty_terminal. Three product crates plus development-only xtask.
CPU, RAM, and GPU must each stay within 1.20 times Alacritty for every shipped preset under matched tests.
Static CRT is default; animation is opt-in but gets no budget exception.
Use a task worktree and the agreed base. Do not commit, push, or open a PR unless asked.
Use project tooling only. Report skipped/unavailable checks; never invent a passing result.
Record progress and handoff here under doc/AI/implementation/. This plan overrides the old plan-directory convention.

## Goal

Make local and GitHub checks identical and provide reliable worktree/task handoff tools.

## Files

- xtask/src/{main,doctor,check,worktree}.rs and integration tests.
- .github/workflows/ci.yml, .github/pull_request_template.md, .github/dependabot.yml.
- deny.toml, pinned auxiliary-tool configuration, .cargo/config.toml.
- AGENTS.md, CONTRIBUTING.md, docs/{development,ai-workflow}.md.

## Steps

1. Implement doctor, check, package-filtered check, and worktree create/list/remove with documented help and exit codes.
2. Keep xtask independent of app/GPU dependencies. Invoke processes with argument arrays. Produce human output and optional JSON summaries with artifacts under the active worktree's target/.
3. Doctor reports missing tools/system libraries; it does not install packages or change global configuration. Check uses locked Cargo operations and pinned workflow/dependency validators.
4. Require an explicit worktree base, validate names/paths, refuse collisions, and keep each worktree's target/config/logs separate. Share only download caches. Transfer the selected plan plus architecture/index references under doc/AI/. Refuse destructive cleanup or replacement of newer notes.
5. Test worktree behavior in temporary repositories, including plan transfer, dirty/untracked/ignored-note preservation, collisions, and failed subprocess propagation. Test JSON and exit-code contracts.
6. Add PR/default-branch GitHub Actions using cargo xtask check, stable required-check names, cache keys, timeout, obsolete-run cancellation, and useful failure artifacts. No release credentials in PR jobs.
7. Add dependency update configuration and contribution/AI task and handoff templates. Document rust-analyzer/debug setup and which checks need a display.
8. Validate workflow files locally. Record GitHub-side rules/settings that still need activation; do not push or create remote resources without user authorization.

## Acceptance criteria

- Local and CI checks share an implementation; ordinary Cargo/editor tooling still works.
- Worktree helpers cannot silently discard a task's work or notes.
- Bot dependency PRs get ordinary checks; no automatic merge is enabled.
- Headless checks are explicit; GPU tests are not silently presented as passed.

## Verification

Commands below are to run after this task and its dependencies supply the tooling.
Do not claim that planned commands already exist.

```sh
rtk cargo xtask doctor
rtk cargo xtask check
rtk cargo test --locked -p xtask
```

Run the focused xtask tests while developing; the final check should include them so a redundant rerun is not required. No terminal or release implementation.

## Handoff

Record base SHA, worktree path, changed files, decisions, exact commands/results, unrun checks, artifact locations, and blockers.
Update this plan and the index status only when supported by evidence. Do not mark performance-dependent work complete on headless checks alone.

