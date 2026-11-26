# First release

Release Retro Term v0.1.0 for Linux x86-64 through the workflow in [Releasing](../../releasing.md).

## Progress

- Set `bootstrap-sha` to the merge commit that first added the release configuration to `main` (`2210858533cc3b9f577a403d56c40e4bbc740034`).
- Confirmed that GitHub has no releases, release variables, repository Actions secrets, registered self-hosted runners, or rulesets. GitHub reports that `main` is unprotected.

## Steps

1. Run `cargo xtask check` from this worktree.
2. Merge the bootstrap change after review.
3. Configure the `RELEASE_TOKEN` repository Actions secret and the repository settings in the external setup checklist. Leave `RETRO_GPU_LABEL` unset for v0.1.0.
4. Enable Release Please. A push to `main` starts it. Review and merge the generated v0.1.0 release PR to create the tag and draft.
5. Dispatch the release workflow from `main` for that tag and SHA with `dry_run: true` and `allow_failed_gate: true`. Confirm that verification, packaging, and the gate pass and that the gate records the override.
6. Dispatch again with `dry_run: false` and the same override. Confirm that the published release has the Linux archive, checksum, and notice about missing performance evidence. Remove the first-release version override.

## Handoff

- Base: `9adc5e6933ea5b8f9f5fa288da2bd560bfa39a85` on `origin/main`.
- Worktree: `.worktrees/first-release`.
- Changed files: Release Please configuration, release workflow, and this plan.
- Checks: `cargo xtask check` passed with access to the Cargo advisory database. One ignored test did not run.
- Release packaging, GPU measurement, and publication are pending.
- Blockers: The `RELEASE_TOKEN` secret and workflow enablement are pending. The user approved skipping GPU measurement for v0.1.0 using the manual override.
