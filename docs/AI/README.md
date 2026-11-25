# Retro terminal plans

In-progress plans live in `docs/AI/<category>/<plan>.md`.
The previous provider-specific plan location has been retired.
These plans are tracked by Git, so every worktree gets them. Tracking does not authorize commits.
Durable architecture/developer documentation will be kept in tracked `docs/` when implemented.

## Start here

Start with [01 — Repository setup](implementation/01-repository-setup.md).
[Shared architecture](architecture/retro-terminal.md) retains the accepted requirements and design decisions.
Each subtask includes context, prerequisites, files, steps, acceptance criteria, verification, and handoff.
The individual subtasks own execution steps; the architecture is the common design reference.

Base commit: `bc5f6ab` on `main`. Task 01 was done directly on `main` (setup exception).

## Execution order

| Plan | Status | Prerequisites |
| --- | --- | --- |
| [01 — Repository setup](implementation/01-repository-setup.md) | Done (uncommitted) | None. |
| [02 — Developer tools, worktrees, and CI](implementation/02-developer-tools-and-ci.md) | Implemented (uncommitted; auxiliary tools not run) | 01 completed. |
| [03 — Terminal engine and PTY sessions](implementation/03-terminal-engine.md) | Pending | 02 completed. |
| [04 — Plain graphical terminal](implementation/04-plain-terminal.md) | Pending | 03 completed. |
| [05 — Performance harness and baseline gate](implementation/05-performance-baseline.md) | Pending | 04 completed. |
| [06 — Static CRT rendering](implementation/06-static-crt.md) | Pending | 05 passed for the plain renderer. |
| [07 — Text configuration and reload](implementation/07-text-configuration.md) | Pending | 06 completed. |
| [08 — Native tabs and splits](implementation/08-tabs-and-splits.md) | Pending | 07 completed. |
| [09 — Optional CRT animations](implementation/09-optional-animation.md) | Pending | 08 completed and resource suite passing. |
| [10 — Automatic packaging and release](implementation/10-automatic-release.md) | Pending | 08 completed; 09's feasibility outcome recorded. The full suite must pass for every included preset. 02/05 supply CI and measurement tooling. |
| [11 — Settings UI and documentation polish](implementation/11-settings-and-polish.md) | Pending | 10 completed; higher-priority functionality and resource gates pass. |

Follow dependency order. This list is not authorization to implement all tasks.
Task 11 is optional low-priority work after the initial release pipeline.
Task 09 may produce an explicit infeasibility finding; do not claim animation support if nothing passes.
Task 10 may release only features/presets with valid passing evidence.

## Fresh-session workflow

1. Read the selected subtask, the shared architecture, and applicable repository instructions.
2. Check dependency results and current files; plans describe future files, not existing tooling.
3. Create a worktree from the agreed base. Until the helper exists in task 02, use Git directly.
4. Plans are tracked, so the worktree already has them. Commit plan updates with the task's work.
5. Implement only the requested scope; run project checks and record evidence in the subtask.
6. Update the task status and handoff. Keep unfinished or infrastructure-blocked work explicit.

No builds, tests, CI jobs, or releases have run for these plans.

