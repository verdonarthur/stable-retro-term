# Automatic packaging and release

Status: implemented locally (uncommitted). All local checks pass. Perf measurement not run (Alacritty missing). See Handoff.
Base: `41200c3` on `feat/static-crt` (open PR #7, stacked on #6 and #5). Tasks 04, 05, and 06 are in the base.
Dependencies: 02 (CI, xtask) and 05 (performance harness). The user approved running this task now,
in parallel with Task 07, instead of after 08/09. See D1.

## Context and shared rules

Read [the architecture](../architecture/retro-terminal.md) ("GitHub CI architecture" and
"Automated release architecture"), [the execution index](../README.md), [the AI workflow](../../ai-workflow.md),
[development](../../development.md), and [performance](../../performance.md).

Linux first. Three product crates plus development-only `xtask`. `xtask` never depends on product crates;
it starts external programs (Cargo, `dist`, `git`, `tar`) with argument arrays through
[process.rs](../../../xtask/src/process.rs). Pin every tool and action. Use locked dependency resolution.
Use project tooling only for verification. Report checks that did not run. Never claim a performance pass.
Do not commit, push, or open a PR unless the user asks. The PR targets `feat/static-crt`.

### Current state at the base (read before work)

- [Cargo.toml](../../../Cargo.toml): `[workspace.package] version = "0.1.0"`, `license = "MIT"`,
  `publish = false`. Members are the glob `crates/*` plus `xtask`. All four packages inherit
  `version.workspace = true` and `publish.workspace = true`. There is no `[profile.dist]`.
- Binary: package `retro-app` builds `retro-term` ([main.rs](../../../crates/app/src/main.rs)).
  It accepts `--development`, `--plain`, `--help`/`-h` (exit 0, prints `Usage: retro-term ...`). There is no `--version`.
- No `assets/`, no example configuration (Task 07 adds configuration), no `CHANGELOG.md`, no release config.
  [LICENSE](../../../LICENSE) (MIT) and [README.md](../../../README.md) exist.
- [tools.toml](../../../tools.toml) pins `cargo-deny`, `actionlint`, `zizmor`. [doctor.rs](../../../xtask/src/doctor.rs)
  checks each key as a binary name; `install_hint` prints `cargo install --locked <key>@<version>`
  except for `actionlint`.
- [check.rs](../../../xtask/src/check.rs) steps: fmt, check, clippy, test, deny, actionlint, zizmor.
  `zizmor --offline .github/workflows` and `actionlint` lint every workflow file, so new workflows must pass both.
- [report.rs](../../../xtask/src/report.rs): `Report`/`Step` with statuses `passed|failed|missing|skipped|not_run`,
  writes `target/xtask/<command>.json`. Exit codes: 0, 1 failed/refused, 2 usage, 3 missing tool.
- [perf.rs](../../../xtask/src/perf.rs) + [perf/run.py](../../../xtask/src/perf/run.py): the report has
  `status`, `preflight`, `scenario`, `source` (HEAD), `dirty`, `candidate_mode` (`static-crt`|`plain`),
  `suite`, and (after preflight succeeds) `measurement.identity.candidate_sha256`, `suite_sha256`,
  `harness_sha256`, `baseline_version`, plus `measurement.host`. Only a full non-preflight run can return `passed`.
- [perf/metrics.py](../../../xtask/src/perf/metrics.py) `compare()` hardcodes the ratio limit 7/5 (1.40).
  The report does not record the limit. [Decision 05](../../decisions/05-renderer-baseline.md) set 1.40
  for the current phase; the architecture target stays 1.20.
- [ci.yml](../../../.github/workflows/ci.yml): `pull_request` and push to `main`; jobs `check`
  (`cargo xtask check --skip deny`) and `deny`. Actions are pinned by SHA with a version comment.
- [performance.yml](../../../.github/workflows/performance.yml): reusable (`workflow_call`, input `gpu_label`),
  runs only when `github.ref_protected` and the event is `push` or `workflow_dispatch`. It **builds from source**
  and measures only the static CRT mode. It does not take a packaged artifact.
- Host: this machine is the measurement machine (hardware AMD Radeon 890M, RADV). Alacritty 0.17.0 is not
  installed, so the perf gate cannot run here yet.

### Facts checked against upstream (2026-11-25)

- `googleapis/release-please-action` latest is **v5.0.0** (bundles release-please 17.6.0, Node 24).
  The config schema of 17.6.0 has `draft`, `force-tag-creation`, `include-component-in-tag`,
  `extra-files`, `bootstrap-sha`, `changelog-sections`. The action outputs `release_created`,
  `tag_name`, `sha`, `version` for the root path `.`.
- release-please's `rust` strategy **does not fit this workspace** (source of 17.6.0, `strategies/rust.ts`,
  `updaters/rust/cargo-toml.ts`): it reads `members` literally (the glob `crates/*` gives no manifest),
  and it throws "is not a package manifest" for the root `Cargo.toml` (no `[package]`). It also writes
  `package.version`, which breaks `version.workspace = true`.
- The `simple` strategy updates `version.txt` only if the file exists (`createIfMissing: false`).
  `extra-files` with `{"type": "toml", "jsonpath": ...}` uses `jsonpath-plus` and `replaceTomlValue`.
  JSONPath filters on `Cargo.lock` arrays are **not verified**.
- `cargo-dist` latest is **v0.34.0** (2026-11-01). Command name `dist`; Linux archives default to `.tar.xz`;
  `checksum = "sha256"` default; `auto-includes` adds README, LICENSE, CHANGELOG; `ci = []` is the default;
  `dist = true` in package metadata overrides `publish = false`. dist names archives after the
  **package** (`retro-app-<target>.tar.xz`), not after the binary.
  `dist build` flags (`--tag`, `--artifacts`, `--target`, `--output-format`) are not in the config reference;
  confirm them with `dist build --help` after install.

## Goal

Prepare, in local files only, the GitHub release-PR flow: Release Please proposes version and changelog,
dist packages the tagged revision, the packaged binary is smoke-tested and measured, a gate binds the
performance reports to that exact binary, and a credential-isolated job publishes the existing draft.
Publication without valid passing evidence needs the explicit manual override `allow_failed_gate` (decision 2),
and the release notes then record that the performance gate was not met.

## Settled decisions

- D1. Run now, in parallel with Task 07 (user-approved reordering). Scope: release-please config and
  manifest, dist config, workflows (`release-please.yml`, `release.yml`, `performance.yml` changes, `ci.yml`
  package job), xtask `package`/`release` code, contract tests, `CHANGELOG.md`, `docs/releasing.md`,
  and a local dry-run rehearsal.
- D2. Local files only. No bot identity, GitHub App, secrets, repository rules, GPU runner, tags, release
  publication, or test-repository rehearsal. `docs/releasing.md` lists the external setup as a checklist.
- D3. The resource gate cannot run on this host. Perf evidence stays unverified. The release gate must
  refuse to publish without valid evidence, unless the manual override of decision 2 is used.
  Never record a pass that did not happen.
- D4. Presets are a data list in `benchmarks/release-gate.toml`. Today the list has the two shipped modes
  (`static-crt`, `plain`). Wiring of Task 07 presets and the Task 09 feasibility outcome is follow-up F1.
- D5. `xtask` only starts external tools. It does not depend on product crates. New dependency: `sha2`
  (MIT/Apache-2.0, already in the deny allowlist) for checksums. Archive extraction uses system `tar`.
- D6. One application version (`[workspace.package] version`), one `vX.Y.Z` tag namespace, one `CHANGELOG.md`.
  Only `retro-app` (binary `retro-term`) is distributed. xtask and libraries are not.
- D7. `dist` is only an artifact builder: `ci = []`, no installers, no GitHub release creation by dist.
  Release Please is the only creator of tags and draft releases. `release.yml` is the only publisher.
  No workflow triggers on tag push.
- D8. Publication runs in a separate hosted job with `contents: write`. It does not check out or execute
  the packaged code. The GPU job and build jobs have `contents: read` only and no secrets.
- D9. Prepared versus enabled: `release-please.yml` runs only when the repository variable
  `RETRO_RELEASE_ENABLED` is `true` (see Q7). Until the user sets it, merges to `main` do nothing.

## Recorded decisions (user, 2026-11-25)

The user accepted the recommended option for Q1 and Q3–Q12 and decided Q2 with an explicit override.

1. **Version updater:** (a) `release-type: simple` with `extra-files` TOML JSONPaths for `Cargo.toml`
   and the members in `Cargo.lock`. `version.workspace = true` stays. `cargo --locked` in release-PR CI
   catches a lockfile miss.
2. **Release gate ratio limit:** 1.20 (`limit = [6, 5]` in `benchmarks/release-gate.toml`).
   **Override:** the gate blocks by default. A manual input `allow_failed_gate: true`
   (`workflow_dispatch` or the reusable-workflow input of `release.yml`) lets the release publish even
   when the performance gate is not met or not verified. Rules:
   - `cargo xtask release gate --allow-failed-gate` still runs every check. Integrity checks
     (`archive`, `identity`) still block. Only failed preset (performance) steps are overridden.
   - When the override is used and a preset step failed, the gate exits 0, records `"overridden": true`
     in `target/xtask/release-gate.json`, and writes `target/xtask/release-gate-notes.md`.
   - The publish job adds those notes to the release notes and attaches the gate summary and the
     performance reports (`performance-evidence-<tag>-run<id>.tar.gz`).
   - With the override, the gate job also runs when the performance job failed or did not run.
   - Push-triggered releases (`release-please.yml`) always pass `allow_failed_gate: false`.
   - Contract tests cover both paths: blocked by default; allowed with the override and recorded.
3. **Presets:** (a) `static-crt` and `plain`.
4. **Targets:** (a) `x86_64-unknown-linux-gnu` only, built on `ubuntu-24.04` (glibc 2.39 minimum).
5. **dist:** (a) dist 0.34.0, `.tar.xz`, `sha256`, no installers. Archive name `retro-app-<target>.tar.xz`
   is accepted; a package rename is a separate decision.
6. **First version:** (a) manifest `{".": "0.1.0"}` plus `"release-as": "0.1.0"`, removed after the first
   release; `bootstrap-sha` is filled when this task lands on `main`; `bump-minor-pre-major: true`.
7. **Trigger and permissions:** (a) push to `main`, guarded by `vars.RETRO_RELEASE_ENABLED == 'true'`,
   GitHub App token, workflow-level `permissions: {}`.
8. **Binding reports to checksums:** (a) the per-preset rules listed in Step 5. No attestations now.
9. **CHANGELOG:** (a) Release Please default Markdown with feat/fix/perf sections.
10. **`package --check` wraps dist:** (a).
11. **PR title check:** (b) deferred.
12. **`--version`:** (b) smoke only `--help`.

## Follow-ups (not in this task)

- F1. **Gate wiring after Task 09/07.** Replace the `presets` list in `benchmarks/release-gate.toml` with the
  presets that Task 07 ships and that Task 09 marks feasible. Add per-preset config (`config = "..."`) to
  `release measure` when presets become files. Until F1, the list holds `static-crt` and `plain`.
- F2. External setup from the `docs/releasing.md` checklist (user action).
- F3. A GitHub dry-run rehearsal (`workflow_dispatch` with `dry_run: true`) after F2.
- F4. Package example configuration and `assets/presets/` after Task 07 (dist `include`).

## Start the implementation worktree

The worktree exists: `/home/arthur/Documents/perso/retro-term/.worktrees/10-automatic-release` on branch
`10-automatic-release`, base `41200c3`. Check with `git -C <worktree> log -1 --format=%H`. If it is
missing, run from the main checkout:

```sh
rtk cargo xtask worktree create 10-automatic-release --base 41200c3
```

## Steps

1. **Pin dist and teach doctor about it.**
   - `tools.toml`: add `dist = "0.34.0"` (key = binary name).
   - `xtask/src/doctor.rs` `install_hint`: map `dist` to `cargo install --locked cargo-dist@<version>`.
     Add a unit test next to `version_matches_whole_words_only` for the hint.
   - Install locally only if the user agrees; otherwise record `missing`.
   - Run `dist --version` and `dist build --help`; record the real output in the handoff. If the flags
     differ from Step 4, adapt Step 4 and note it.

2. **Release Please configuration (Q1, Q6, Q9).**
   - `release-please-config.json`:
     ```json
     {
       "$schema": "https://raw.githubusercontent.com/googleapis/release-please/v17.6.0/schemas/config.json",
       "release-type": "simple",
       "include-component-in-tag": false,
       "include-v-in-tag": true,
       "draft": true,
       "force-tag-creation": true,
       "bump-minor-pre-major": true,
       "bootstrap-sha": "<fill when this lands on main; see Q6>",
       "changelog-sections": [ ...Q9 ... ],
       "packages": {
         ".": {
           "release-as": "0.1.0",
           "extra-files": [
             {"type": "toml", "path": "Cargo.toml", "jsonpath": "$.workspace.package.version"},
             {"type": "toml", "path": "Cargo.lock",
              "jsonpath": "$.package[?(@.name==='retro-app' || @.name==='retro-renderer' || @.name==='retro-terminal' || @.name==='xtask')].version"}
           ]
         }
       }
     }
     ```
     Leave `bootstrap-sha` out until the user gives the SHA; record it as pending in the handoff.
   - `.release-please-manifest.json`: `{".": "0.1.0"}`.
   - `CHANGELOG.md`: a title line only (`# Changelog`). Release Please prepends entries.

3. **dist configuration (Q4, Q5).**
   - `dist-workspace.toml`:
     ```toml
     [workspace]
     members = ["cargo:."]

     [dist]
     cargo-dist-version = "0.34.0"
     ci = []
     installers = []
     targets = ["x86_64-unknown-linux-gnu"]
     unix-archive = ".tar.xz"
     checksum = "sha256"
     create-release = false
     ```
   - `Cargo.toml`: add `[profile.dist]` with `inherits = "release"` only. Do not add LTO: the measured
     binary must match the release profile that perf docs use.
   - `crates/app/Cargo.toml`: `[package.metadata.dist] dist = true`.
     `xtask/Cargo.toml`: `[package.metadata.dist] dist = false`. The libraries have no binaries.
   - Run `dist plan` once; it must list only `retro-app` with binary `retro-term`. Record the output.

4. **`cargo xtask package --check` (Q10).** New `xtask/src/release.rs` (or `package.rs` + `release.rs` if
   `release.rs` passes ~300 lines). Add `Package(release::PackageArgs)` and a `Release` subcommand group to
   [main.rs](../../../xtask/src/main.rs).
   - Args: `--check` (required; without it exit 2 with "xtask never publishes; use --check"),
     `--tag vX.Y.Z` (optional), `--source-sha <40-hex>` (optional, requires `--tag`), `--json`.
   - Steps, each a `report::Step`, logs under `target/xtask/logs/package-*.log`:
     1. `dist` present (else `missing`, exit 3, note `run cargo xtask doctor`).
     2. `lock`: `cargo fetch --locked` must pass.
     3. `identity` (only with `--tag`): run the `release verify` checks of Step 5 in-process.
     4. `dist`: `dist build --artifacts=local --target x86_64-unknown-linux-gnu [--tag <tag>]`
        (flags from Step 1). Output goes to `target/distrib/`.
     5. `unchanged`: `git status --porcelain -- Cargo.lock Cargo.toml` is empty after the build.
     6. `layout`: exactly one `retro-app-x86_64-unknown-linux-gnu.tar.xz` and its `.sha256`. Recompute the
        archive sha256 with `sha2` and compare. Extract with `tar -xJf <archive> -C target/xtask/package/`
        into a fresh directory. Require `retro-term` (executable bit), `LICENSE`, `README.md`, `CHANGELOG.md`.
        Reject absolute paths, `..` entries, and symlinks (`tar -tvJf` listing first).
     7. `smoke`: run the extracted `retro-term --help` with `stdin` null; exit 0 and stdout starts with
        `Usage: retro-term`. No display needed.
   - Write `target/xtask/package/identity.json`:
     `{schema: 1, version, tag|null, source_sha (git rev-parse HEAD), dirty, rustc (rustc -V), dist (dist --version),
       target, archive: {path, sha256}, binary: {path, sha256}}`. Paths are relative to the workspace root.
   - The summary is `target/xtask/package.json` (existing `Report`). Never tag, upload, or publish.

5. **`cargo xtask release` subcommands.**
   - `release verify --tag <tag> --source-sha <sha>`: tag must match `^v\d+\.\d+\.\d+$` (else exit 2).
     Checks (each a step, failure exit 1): `git rev-parse HEAD == sha`; `git rev-parse refs/tags/<tag>^{commit} == sha`;
     workspace version (`cargo metadata --locked --no-deps`, all four packages) == tag version;
     `Cargo.lock` entries of the four members == tag version; `.release-please-manifest.json` `"."` == tag version;
     `CHANGELOG.md` has a heading for that version. Summary `target/xtask/release-verify.json`.
   - `release measure --identity <file> --gate benchmarks/release-gate.toml --baseline <alacritty> --output-dir <dir>`:
     for each preset in the gate file, call the existing perf entry point in-process (build `perf::Args`)
     on `identity.binary.path` with `--suite <gate.suite>`, the preset mode, and the gate limit (Step 6),
     output `<dir>/<preset>.json`. Run preflight first per preset into `<dir>/<preset>-preflight.json`.
     Exit 0 only when every preset returns `passed`. It never decides publication.
   - `release gate --identity <file> --reports <dir> --gate <file> --source-sha <sha>`: reads only files;
     it never executes the binary. Steps: `archive` (recompute archive sha256 == identity), `identity`
     (`identity.source_sha == sha`, `dirty == false`, `tag` set), then one step per preset with the Q8 rules.
     A missing, unreadable, stale (other SHA), partial, preflight, unverified, failed, wrong-mode,
     wrong-limit, or wrong-checksum report gives `failed` with a precise note. Extra report files are
     ignored. Exit 0 only when every step passes. Summary `target/xtask/release-gate.json`.
   - `benchmarks/release-gate.toml` (data, D4):
     ```toml
     version = 1
     # Ratio limit as numerator / denominator: 6/5 = 1.20 (Q2).
     limit = [6, 5]
     suite = "scenarios.toml"

     [[presets]]
     name = "static-crt"
     candidate_plain = false

     [[presets]]
     name = "plain"
     candidate_plain = true
     ```
     Parse with `serde(deny_unknown_fields)`; reject `version != 1`, empty or duplicate names, and a limit
     with a zero part.

6. **Record the ratio limit in perf reports.** Required by Q8; keep it surgical.
   - `perf.rs`: add `--limit <N/D>` (optional; default `7/5` so Task 05/06 commands keep their meaning).
     Pass `limit` in the request and write `"limit": [n, d]` at the top of the report.
   - `perf/metrics.py` `compare(pairs, metric, gpu_resolution=None, limit=(7, 5))`: replace the fixed
     `5 * ... - 7 * ...` with `d * ... - n * ...`. Update the `CPU_FLOOR_NS` comment (no fixed ratio).
     `run.py` passes `request["limit"]`.
   - Tests: in `perf/test_perf.py` add the exact 6/5 boundary (pass at ratio 1.20, fail just above) and keep
     existing 7/5 cases. In `xtask/tests/perf.rs` assert that a report written for a missing binary contains
     `"limit": [7, 5]` by default and `[6, 5]` with `--limit 6/5`; `--limit 0/5` exits 2.
   - `docs/performance.md`: document `--limit` and the recorded field.

7. **Contract tests** in new `xtask/tests/release.rs` (headless, `#![cfg(target_os = "linux")]`). Use temp dirs
   and fixture JSON written by the test. Required tests:
   - `package_refuses_without_check` → exit 2. `package_help_says_never_publishes` → `--help` text.
   - `package_reports_missing_dist` with an empty `PATH` → exit 3, `target/xtask/package.json` step `dist` is `missing`.
   - `release_config_is_consistent`: `release-please-config.json` has `draft`, `force-tag-creation` true,
     `include-component-in-tag` false; every `extra-files` path exists; manifest `"."` == workspace version;
     the `Cargo.lock` JSONPath names exactly the workspace members from `cargo metadata --no-deps`.
   - `dist_config_matches_pins`: `cargo-dist-version` == `tools.toml` `dist`; `ci` empty; `installers` empty;
     `create-release = false`; only `retro-app` has `dist = true`; `xtask` has `dist = false`.
   - `gate_passes_only_with_bound_evidence`: build a temp gate dir with identity + archive + two passing reports →
     exit 0. Then one test case each for: missing report, `unverified`, `failed`, preflight report,
     single-scenario report, `dirty: true`, other `source` SHA (stale), other `candidate_sha256`,
     wrong `candidate_mode`, `limit` [7,5] under a [6,5] gate, archive bytes changed after identity → exit 1,
     and the failing step named in `target/xtask/release-gate.json`.
   - `gate_rejects_invalid_gate_file` → exit 2 for unknown field, duplicate preset, zero limit.
   - `verify_rejects_mismatches` in a temp Git repo with a minimal workspace: malformed tag → 2;
     tag/version mismatch, lock mismatch, HEAD ≠ sha, tag on another commit → 1; matching → 0.
   - `workflows_keep_the_publication_boundary` (text checks, no YAML dependency): no workflow has a `tags:`
     push trigger; `contents: write` appears only in the `release.yml` publish job and the `release-please.yml` caller job; `release.yml` publish job
     has no `actions/checkout`; `release-please.yml` mentions `RETRO_RELEASE_ENABLED`.
     Keep these checks few; actionlint and zizmor do structural linting.

8. **Workflows.** Pin every action by full commit SHA with a `# vX.Y.Z` comment, like `ci.yml`. Resolve SHAs with
   `gh api repos/<owner>/<repo>/commits/<tag> --jq .sha`. Use `persist-credentials: false`, top-level
   `permissions: {}` (job-level grants), pass every `inputs.*`/`steps.*` value to `run:` through `env:` (zizmor
   template-injection rule), `timeout-minutes` on every job, `runs-on: ubuntu-24.04` for build/publish jobs.
   - `ci.yml`: add job `package` (hosted): install pinned dist (`taiki-e/install-action` with `tool: cargo-dist@<pin>`
     if supported; otherwise `cargo install --locked cargo-dist@<pin>`), run `cargo xtask package --check`,
     upload `target/xtask/` on failure. Release PRs then get packaging checked before merge.
   - `release-please.yml`: `on: push: branches: [main]`; job `release-please` with
     `if: vars.RETRO_RELEASE_ENABLED == 'true'`; steps: `actions/create-github-app-token` (secrets
     `RELEASE_APP_ID`, `RELEASE_APP_PRIVATE_KEY`), `googleapis/release-please-action@<sha> # v5.0.0` with that
     token, `config-file`, `manifest-file`. Outputs `release_created`, `tag_name`, `sha`. Job `release`:
     `if: needs.release-please.outputs.release_created == 'true'`, `uses: ./.github/workflows/release.yml`,
     `with: {tag, sha, dry_run: false}`, no `secrets: inherit`. A called workflow cannot exceed its caller
     job's permissions, so this caller job sets `permissions: contents: write`; every job inside `release.yml`
     except `publish` reduces it to `contents: read`.
   - `release.yml`: `on: workflow_call` (inputs `tag`, `sha`, `dry_run` bool) and `workflow_dispatch`
     (inputs `tag`, `dry_run` default `true`) for recovery and rehearsal. `concurrency: release-${{ inputs.tag }}`,
     `cancel-in-progress: false`. Jobs:
     1. `verify` (`contents: read`): checkout `sha` (for dispatch, resolve `refs/tags/<tag>^{commit}`),
        fetch tags, `cargo xtask release verify --tag --source-sha`.
     2. `package` (`contents: read`, needs verify): `cargo xtask check --skip deny`, `cargo xtask check --step deny`,
        `cargo xtask package --check --tag --source-sha`; upload artifact `package-<tag>` with
        `target/distrib/*.tar.xz*` and `target/xtask/package/identity.json`.
     3. `performance` (needs package): `uses: ./.github/workflows/performance.yml` with `gpu_label` from
        `vars.RETRO_GPU_LABEL`, `package_artifact: package-<tag>`, `sha`.
     4. `gate` (`contents: read`, hosted, needs performance): download both artifacts, checkout `sha` for the
        gate file and xtask, `cargo xtask release gate ...`. Runs `if: always()` on the needs results is **not**
        allowed; a skipped/failed performance job means the gate job does not pass.
     5. `publish` (`contents: write`, needs gate, `if: inputs.dry_run == false`): no checkout. Download
        `package-<tag>`; `sha256sum -c` each `.sha256`; `gh release view <tag> --json isDraft,tagName`
        must show a draft; `git ls-remote` / `gh api` must show the tag at `sha`. For each file: if an asset
        with the name exists, download it and compare sha256 (same → skip, different → fail; never
        `--clobber`); otherwise `gh release upload`. Then `gh release edit <tag> --draft=false`.
        Publishing an already published release with equal bytes exits 0 (idempotent retry).
   - `performance.yml`: add optional inputs `package_artifact` and `sha`. When set: checkout `sha`, download
     the artifact, check its `.sha256`, extract, then `cargo xtask release measure --identity ... --gate
     benchmarks/release-gate.toml --baseline alacritty --output-dir target/perf/release`; upload
     `target/perf/release/` as `performance-<tag>`. When not set, keep today's build-from-source behaviour.
     Keep `concurrency` per GPU label and the `github.ref_protected` guard; allow `workflow_call` from the
     release path (the caller's event is `push` or `workflow_dispatch`, so the guard still holds).

9. **Documentation.**
   - New `docs/releasing.md`: flow diagram in text (merge → release PR → merge → tag + draft → verify →
     package → perf → gate → publish); prepared vs enabled table; **external setup checklist** (GitHub App with
     contents/pull-requests write on this repo only; secrets `RELEASE_APP_ID`, `RELEASE_APP_PRIVATE_KEY`;
     variable `RETRO_RELEASE_ENABLED`; variable `RETRO_GPU_LABEL`; self-hosted GPU runner with Alacritty 0.17.0
     and the `docs/performance.md` prerequisites; branch protection on `main` so `github.ref_protected` is true;
     tag ruleset restricting `v*` creation/update/deletion to the app; squash-merge only; required checks
     `check`, `deny`, `package`); supported target and runtime libraries (glibc ≥ 2.39 from ubuntu-24.04,
     Wayland client, libxkbcommon, Vulkan loader and driver, fonts — from `docs/development.md`);
     recovery: same-tag retry via `workflow_dispatch` (same tag, same bytes), corrective release for defects,
     never move a published tag or replace published bytes; local commands.
   - `docs/development.md`: commands table (`package --check`, `release verify|measure|gate`), CI section (`package` job).
   - `docs/performance.md`: `--limit`, and that release measurement uses the packaged binary.
   - `docs/AI/README.md`: row 10 status and prerequisites per D1. This plan: progress and handoff.

10. **Local dry-run rehearsal** (no GitHub, no publication). Record each command and exit code.
    - `rtk cargo xtask package --check` (needs dist; if missing, record exit 3 and stop this sub-step).
    - `rtk cargo xtask release gate --identity target/xtask/package/identity.json --reports target/perf/release --gate benchmarks/release-gate.toml --source-sha $(git rev-parse HEAD)`
      with no reports: expect exit 1 with `missing report` for both presets. This shows refusal without evidence.
    - Do not run `release measure` here (Alacritty 0.17.0 not installed). Record it as not run.

## Acceptance criteria

- Release PRs get normal CI plus the `package` job and show reviewable version and changelog changes.
- After enablement (F2), a release-PR merge starts verify → package → performance → gate → publish with no
  manual step. Until enablement, nothing runs on `main`.
- Any failure leaves the draft unpublished. Missing, stale, partial, unverified, or failing reports, a SHA or
  checksum mismatch, or an unavailable runner block publication (contract tests prove the gate side).
- The archive and binary checksums are the same from packaging through measurement, gate, and upload.
- Presets are a data list; the limit is data and recorded in every perf report.
- `docs/releasing.md` separates prepared local configuration from enabled infrastructure.
- `cargo xtask check` passes; dist, actionlint, zizmor statuses are reported truthfully.

## Verification

```sh
rtk cargo xtask doctor
rtk cargo xtask check
rtk cargo xtask package --check
rtk cargo test --locked -p xtask --test release
```

Then the gate refusal command from Step 10. If `actionlint`, `zizmor`, `cargo-deny`, or `dist` is missing,
`check`/`package` exit 3; report them as missing, not passed. The perf gate (`release measure`) needs the
Task 05 GPU host and Alacritty 0.17.0; it does not run on this host. Never mark it passed.

## Out of scope

Creating the GitHub App, secrets, variables, rulesets, branch protection, or GPU runner; pushing tags;
publishing releases; test-repository rehearsals; macOS/Windows; AppImage/deb/rpm; installers; attestations;
example configuration and preset files (Task 07, F4); final preset list (F1).

## Handoff

- Base: `41200c3` on `feat/static-crt`
- Worktree: `.worktrees/10-automatic-release` (branch `10-automatic-release`). Nothing staged or committed.
- Status: Steps 1–10 done locally. Only the perf measurement (Alacritty 0.17.0 missing) and GitHub-side work remain.
- Changed files:
  - Step 1: `tools.toml` (`dist = "0.34.0"`), `xtask/src/doctor.rs` (hint + unit test).
  - Step 2: `release-please-config.json`, `.release-please-manifest.json`, `CHANGELOG.md`. `bootstrap-sha` pending.
  - Step 3: `dist-workspace.toml`, `Cargo.toml` (`[profile.dist]`, `repository`), `crates/app/Cargo.toml` (`repository.workspace`), `xtask/Cargo.toml`
    (`dist` metadata, `sha2 = "0.10"`), `Cargo.lock` (sha2 and its dependencies).
  - Steps 4–5: `xtask/src/package.rs`, `xtask/src/release.rs`, `xtask/src/main.rs`, `xtask/src/report.rs`
    (`overridden` field, omitted when false), `benchmarks/release-gate.toml`.
  - Step 6: `xtask/src/perf.rs` (`--limit`, `Limit`, fields `pub(crate)` for `release measure`),
    `xtask/src/perf/metrics.py`, `xtask/src/perf/run.py` (2 lines in `run_suite`), `xtask/src/perf/test_perf.py`,
    `xtask/tests/perf.rs`.
  - Step 7: `xtask/tests/release.rs`.
  - Step 8: `.github/workflows/{ci.yml,performance.yml,release.yml,release-please.yml}`.
  - Step 9: `docs/releasing.md`, `docs/development.md`, `docs/performance.md`, `docs/AI/README.md`,
    `docs/AI/architecture/retro-terminal.md` (override rule), this plan.
- Decisions and deviations:
  - Decision 2 override: see "Recorded decisions". Tests `gate_override_allows_failed_performance_and_records_it`,
    `gate_override_never_skips_integrity_checks`, `only_a_manual_input_can_override_the_gate`.
  - `create-github-app-token` v3.4.0 deprecates `app-id`; the workflow uses `client-id` from the variable
    `RELEASE_APP_CLIENT_ID` (not the secret `RELEASE_APP_ID`) and limits the token to contents and pull-requests write.
  - `taiki-e/install-action` v2.90.8 does not support cargo-dist: CI uses `cargo install --locked cargo-dist@<pin>`.
  - `release.yml` package job runs `cargo xtask check --skip actionlint --skip zizmor` (deny included) instead of
    `check --skip deny` + `check --step deny`: the workflow linters already ran in PR CI.
  - The identity file records `binary.member` (path inside the archive). The package step finds `retro-term` in the
    archive listing instead of assuming dist's directory layout; the gate re-hashes the binary from the archive.
  - The gate job uploads a fixed-layout artifact `gate-<tag>` (`release-gate.json`, notes, `reports/`).
  - `workflow_dispatch` also has an optional `sha` input; the verify job resolves the tag commit and checks it.
  - `workflows_keep_the_publication_boundary` counts lines equal to `contents: write`
    (`permission-contents: write` of the App token is a token scope, not a job permission).
  - dist 0.34.0 refuses `dist plan`/`manifest` without `repository` in the distributed package. Added
    `repository = "https://github.com/verdonarthur/stable-retro-term"` (the `origin` URL) to `[workspace.package]`,
    inherited by `retro-app` only. `dist build --help` confirms `--artifacts`, `--target`, `--tag`, `--output-format`.
  - The `unchanged` step compares `Cargo.toml`/`Cargo.lock` bytes before and after `dist build`, not
    `git status`: `git status` fails in any worktree with uncommitted manifest edits.
  - zizmor 1.33.1 asks for the `$/` self-repository syntax, but actionlint 1.7.12 rejects it. The two local
    reusable-workflow calls keep `./` with `# zizmor: ignore[self-repository]`.
  - Tools installed on this host (user-approved 2026-11-25): `cargo install --locked cargo-dist@0.34.0`.
    cargo-deny 0.22.1 and zizmor 1.33.1 need rustc ≥ 1.95/1.97 (host has 1.94), and Go is not installed, so
    actionlint 1.7.12, cargo-deny 0.22.1, and zizmor 1.33.1 came from their GitHub release binaries
    (checksums verified: actionlint checksums file, cargo-deny `.sha256`, zizmor release asset digest) into `~/.local/bin`.
- Commands run and results (2026-11-25, after tool install):
  - `cargo xtask doctor` → exit 0: git, rustup, toolchain, actionlint, cargo-deny, dist, zizmor passed.
  - `cargo xtask check` → exit 0: fmt, check, clippy, test, deny, actionlint, zizmor passed; 1 ignored (display) test not run.
  - `cargo test --locked -p xtask` → all passed (release: 12, perf: 5, unit: 6, cli: 5, worktree: 10).
  - `dist plan` → only `retro-app 0.1.0`: `retro-app-x86_64-unknown-linux-gnu.tar.xz` with `[bin] retro-term`,
    `[misc] CHANGELOG.md, LICENSE, README.md`, and its `.sha256`.
  - `cargo xtask package --check` → exit 0: dist, lock, build, unchanged, layout, smoke, identity-file passed.
    Archive member `retro-app-x86_64-unknown-linux-gnu/retro-term`. Identity has `dirty: true` (uncommitted work).
  - Gate refusal rehearsal (`release gate ... --source-sha $(git rev-parse HEAD)`) → exit 1: archive passed;
    identity failed (dirty worktree); `static-crt` and `plain` failed (missing report).
  - `release verify --tag v0.1.0 --source-sha HEAD` → exit 1 (no tag, no changelog heading), as expected.
- Not run, and why: `release measure` and the perf gate (this AMD Radeon 890M RADV host is the measurement
  machine, but Alacritty 0.17.0 is not installed; never claimed as passed); GitHub rehearsal (out of scope, F3).
- Artifacts: `target/distrib/`, `target/xtask/package/identity.json`, `target/xtask/{doctor,check,package,release-verify,release-gate}.json`.
- Blockers and next step: install Alacritty 0.17.0 and run `release measure` on this host; external setup (F2)
  with `RELEASE_APP_CLIENT_ID` (variable, user-approved) and `RELEASE_APP_PRIVATE_KEY` (secret); gate wiring (F1).
