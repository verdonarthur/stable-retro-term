# AI workflow

Rules for AI agents and humans who run AI-assisted tasks. See also `AGENTS.md`.

## Task flow

1. Pick a plan in `docs/AI/<category>/<plan>.md`. Plans are tracked by Git.
2. `cargo xtask worktree create <task> --base <agreed base>`. There is no default base.
3. Work in `.worktrees/<task>`. Update the plan with progress as you go; commit it with the work.
4. `cargo xtask check`. Report skipped and missing steps; never claim a check passed if it did not run.
5. Write the handoff below in the plan. Do not commit, push, or open a pull request unless the user asks.
6. When the work is merged: `cargo xtask worktree remove <task>`.

## Handoff template

```markdown
## Handoff

- Base: <sha> on <branch>
- Worktree: .worktrees/<task>
- Changed files: ...
- Decisions and deviations: ...
- Commands run and results: ...
- Not run, and why: ...
- Artifacts: target/xtask/...
- Blockers and next step: ...
```
