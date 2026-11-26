# Releasing

Releases are automatic after a release PR merge. Release Please proposes the version and changelog,
`dist` builds the archive, the packaged binary is measured, a gate binds the reports to that binary,
and one job publishes the existing draft release. Nothing in the repository publishes from a local machine.

## Flow

```text
merge to main
  -> release-please.yml: open or update the release PR (version, Cargo.lock, CHANGELOG.md)
merge the release PR
  -> release-please.yml: tag vX.Y.Z and a draft GitHub release for that merge commit
  -> release.yml (tag, sha)
       verify       cargo xtask release verify        tag, commit, versions, changelog agree
       package      cargo xtask check, cargo xtask package --check --tag --source-sha
       performance  performance.yml on the GPU runner: cargo xtask release measure
       gate         cargo xtask release gate          reports bound to the packaged binary
       publish      check checksums, draft, and tag; upload assets; publish the draft
```

Only `release-please.yml` creates tags and draft releases. Only the `publish` job of `release.yml` publishes.
No workflow runs on a tag push. `dist` only builds artifacts (`ci = []`, no installers, no release creation).
The `publish` job is the only job with `contents: write`. It does not check out or run the packaged code.

## Prepared and enabled

| Part | State in the repository | Needed to enable |
| --- | --- | --- |
| Release Please config, manifest, `CHANGELOG.md` | Prepared | Nothing |
| `dist-workspace.toml`, `[profile.dist]` | Prepared | Nothing |
| `ci.yml` `package` job | Active on pull requests | Nothing |
| `release-please.yml` | Inactive | `RETRO_RELEASE_ENABLED = true` and the `RELEASE_TOKEN` secret |
| `release.yml` | Runs only when called or dispatched | GPU runner, `RETRO_GPU_LABEL` |
| `benchmarks/release-gate.toml` | Limit 6/5 (1.20); presets `static-crt`, `plain` | Additional profiles pending measurement |

## External setup checklist

Do these in GitHub. The repository holds no credentials.

- [ ] Store a GitHub access token as the repository Actions secret `RELEASE_TOKEN`.
      A fine-grained personal access token needs access to this repository with Contents and Pull requests set to
      Read and write. An OAuth user token needs a scope that grants write access to this repository, such as
      `public_repo` for a public repository or `repo` for a private one. The token acts as its user.
- [ ] Self-hosted GPU runner with a unique label, Alacritty 0.17.0, and the prerequisites in
      [performance](performance.md). Set repository variable `RETRO_GPU_LABEL` to that label.
- [ ] Branch protection on `main`, so that `github.ref_protected` is true. Required checks: `check`, `deny`, `package`.
- [ ] Squash merges only. PR titles follow Conventional Commits (`feat:`, `fix:`, `perf:`); they drive the changelog.
- [ ] Tag ruleset: allow the release token's user to create `v*` tags and protect the tags from updates and deletions.
- [ ] Rehearse: run `release.yml` by hand with `dry_run: true` on a test tag.
- [ ] Set repository variable `RETRO_RELEASE_ENABLED` to `true`.
- [ ] After the first release, remove `release-as` from `release-please-config.json`.

## Supported target

`x86_64-unknown-linux-gnu`, built on `ubuntu-24.04` (glibc 2.39 or newer at runtime).
The archive is `retro-app-x86_64-unknown-linux-gnu.tar.xz` with a `.sha256` file. It holds `retro-term`,
`LICENSE`, `README.md`, and `CHANGELOG.md`.

Runtime needs: a native Wayland session, the Wayland client library, `libxkbcommon`, a Vulkan loader and driver,
and a system monospace font. See [development](development.md).

## The performance gate

`cargo xtask release gate` reads files only. It passes when:

- the archive matches the package identity, and the binary inside it matches the identity checksum;
- the identity has the release commit, a tag, and a clean worktree;
- for every preset in `benchmarks/release-gate.toml`, the report `<preset>.json` has status `passed`, is not a
  preflight or single-scenario run, has a clean worktree, has the release commit as `source`, has the packaged binary
  checksum, has the preset's candidate mode, records the gate limit, and has the gate suite checksum.

A missing, stale, partial, unverified, or failed report blocks publication.

### Manual override: `allow_failed_gate`

A person can publish even when the performance gate is not met. Run `release.yml` by hand
(`workflow_dispatch`) with `allow_failed_gate: true`, or pass the same input to the reusable workflow.
Push-triggered releases always pass `allow_failed_gate: false`.

With the override:

- the gate job runs even if the performance job failed or did not run;
- `cargo xtask release gate --allow-failed-gate` still runs every check. The archive and identity checks still block;
- when a preset failed, the gate exits 0, writes `"overridden": true` in `target/xtask/release-gate.json`,
  and writes `target/xtask/release-gate-notes.md`;
- the publish job adds those notes ("Performance gate not met") to the release notes and attaches
  `performance-evidence-<tag>-run<id>.tar.gz` with the gate summary and all available reports.

Use the override only on purpose. It does not make the evidence valid.

## Recovery

- A failure leaves the draft unpublished. Read the job logs and the uploaded `target/xtask/` artifacts.
- Retry: run `release.yml` by hand with the same tag. Assets with equal bytes are kept; different bytes fail.
  The publish step never replaces an asset.
- A defect in a published release needs a new corrective release. Never move a published tag or replace published bytes.

## Local commands

```sh
cargo xtask package --check
cargo xtask release verify --tag v0.1.0 --source-sha "$(git rev-parse HEAD)"
cargo xtask release gate --identity target/xtask/package/identity.json --reports target/perf/release \
  --gate benchmarks/release-gate.toml --source-sha "$(git rev-parse HEAD)"
```

`package --check` needs the pinned `dist` (`cargo xtask doctor` prints the install command).
`release measure` needs the GPU host of [performance](performance.md). None of these commands tag, upload, or publish.
