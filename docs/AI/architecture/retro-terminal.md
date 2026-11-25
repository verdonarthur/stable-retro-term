# Rust retro terminal: architecture and implementation plan

Status: shared architecture reference. Execute the ordered subtask plans in `../implementation/`; performance is unverified.
Date: 2026-11-24.

## Goal and acceptance criteria

Build a Rust terminal inspired by cool-retro-term, with Linux as the first supported platform.
Keep CPU use, RAM use, and GPU use each at or below 1.20 times Alacritty on matched workloads.
Apply this limit to every shipped preset, including optional animated presets.
Provide text configuration and built-in tabs and splits; a graphical settings editor has lower priority.
Use a static CRT appearance by default, and reuse an existing terminal engine when suitable.
Make local development and AI-assisted work reproducible through isolated worktrees and shared project commands.
Use GitHub Actions for CI and publish releases automatically after a generated release PR is merged and release checks pass.

## Decisions confirmed by the user

- Linux first; preserve a practical path to macOS and Windows later.
- Resource consumption: high priority.
- Text configuration: medium priority.
- Native tabs and splits: medium priority.
- More user-friendly configuration: low priority.
- Reuse a performant, practical Rust terminal engine. Write a custom engine only if existing choices are unsuitable.
- Static CRT effects by default. Animation is opt-in.
- Alacritty is the resource baseline. The limit is a 20% increase.
- Measure CPU, RAM, and GPU separately. Do not use a combined score.
- The limit applies to animated presets too; they do not get an exception.
- No reference hardware was selected. The requested comparison is relative, on the same machine and workload.
- Do not commit, push, or open a merge request without a further user request.
- New branch work must use a Git worktree.
- Developer experience and AI workflow are part of the architecture, including worktrees, CI, and automated release.
- GitHub hosts the repository; GitHub Actions runs CI.
- Merging a generated release PR triggers automatic versioning, tagging, packaging, and publication.
  The release PR contains the proposed version/changelog changes before merge.
- This request extends the plan. It does not request deployment of workflows, creation of credentials, or publication now.

## Current workspace and evidence

The project directory is `/home/arthur/Documents/perso/retro-term`.
Task 01 created the Cargo workspace (three product crates plus `xtask`), root docs, and checks. There is no terminal or test suite yet.
Base commit: `bc5f6ab` on `main`.
There is no existing application behavior or branch diff to preserve.
Plans live under `docs/AI/<category>/` and are tracked in Git.
See `../README.md` for the task order and status.

Read the root `AGENTS.md` before implementation.

## Feasibility and unresolved engineering risks

Planning confidence: 75/100, a subjective engineering estimate rather than a measured probability.
Engine reuse, text configuration, and pane layout are feasible. The strict resource budget remains unproven.

- B1 — Repository setup: resolved. Base commit `bc5f6ab` exists; task 01 is done.
- B2 — Relative idle budget: when Alacritty's GPU work is zero, 1.20 times zero is still zero.
  Independent continuous animation cannot meet that case. Do not silently add an absolute allowance.
  Effects must be optimized, scheduled only when permitted by measurements, or excluded if they fail.
- B3 — Renderer overhead: `wgpu` and general text-shaping libraries may exceed the RAM or GPU budget before CRT effects.
  Treat the renderer stack as a candidate until the first prototype is measured.
  If it fails, investigate allocations, font loading, graphics backend, and rendering approach before adding features.
- B4 — GPU measurement: driver support for per-process GPU counters varies.
  Missing or incomparable counters mean the GPU result is unverified, not a pass.
- B5 — Hardware coverage: a passing result on one machine does not prove the limit on all machines.
  Publish the test configuration and measured support matrix. Do not claim a universal runtime guarantee.
- B6 — Release infrastructure: automatic enforcement of the GPU gate requires a controlled GPU runner and comparable counters.
  Provision this before enabling publication; do not substitute a software-rendered hosted-runner result.
  GitHub App credentials for the release bot and repository rules also need setup when the workflows are implemented.

B2 follows from the accepted ratio. B3–B5 must be resolved through the prototype and benchmark suite below.

## Recommended dependencies

- `alacritty_terminal`: terminal state, parser, selection/scrollback support, and existing PTY/event-loop machinery.
  Keep it behind the `terminal` crate's small API. Do not fork Alacritty's whole application.
  Window key events, application shortcuts, and input encoding still need integration work.
- `winit`: windows, input, display scaling, and the main event loop.
  Design for Linux Wayland and X11; record which backend each test actually exercises.
- `wgpu`: candidate GPU API; use WGSL for new CRT shaders.
- `glyphon` / `cosmic-text`: candidate text stack. Prove grid alignment and resource cost before committing to it.
- `serde` and `toml`: typed configuration loading and validation.
- Consider `egui` only when a settings editor is implemented; it is not required for the first terminal prototype.

Use compatible published releases, pin a tested toolchain, and keep `Cargo.lock`.
Check that all graphics/text dependencies use compatible `wgpu` versions; do not independently pick every latest version.
No dependency combination has been compiled as part of this plan.
Reuse Alacritty's PTY implementation first rather than introducing a second PTY library.
No async runtime is needed for the initial design; the existing PTY worker and main UI loop are sufficient.

## Repository structure

Names below are the proposed Cargo package names. The application binary is `retro-term`.

```text
retro-term/
  Cargo.toml                         # Workspace; shared dependency versions/lints
  Cargo.lock
  rust-toolchain.toml
  README.md
  AGENTS.md                          # Shared instructions for AI agents
  CONTRIBUTING.md                    # Setup and contribution workflow
  CHANGELOG.md                       # Maintained through release PRs
  .editorconfig
  .cargo/config.toml                 # cargo xtask alias; no global target directory
  .github/
    workflows/
      ci.yml                         # PR/default-branch checks
      performance.yml                # Reusable controlled GPU measurements
      release-please.yml             # Release PR and post-merge orchestration
      release.yml                    # Reusable build, verify, publish pipeline
    pull_request_template.md
    dependabot.yml                   # Reviewable dependency/action updates
  release-please-config.json
  .release-please-manifest.json
  dist-workspace.toml                # Pinned dist packaging configuration
  deny.toml                          # Dependency/license policy
  xtask/                             # Development-only package; never shipped
    Cargo.toml
    src/
      main.rs
      doctor.rs                      # Check setup without changing the machine
      check.rs                       # Shared local/CI checks
      worktree.rs                    # Safe task checkout setup and cleanup
      perf.rs                        # Matched Alacritty comparison and reports
      release.rs                     # Local package checks; CI publication boundary
  crates/
    terminal/                        # Package: retro-terminal
      Cargo.toml
      src/
        lib.rs                       # Public session API and IDs
        session.rs                   # PTY creation, workers, resize, exit
        engine.rs                    # Alacritty integration
        input.rs                     # Platform-neutral input encoding
        snapshot.rs                  # Visible cells, modes, cursor, damage
      tests/
        terminal.rs                  # Escape sequences, Unicode, resize
        session.rs                   # PTY round-trip and cleanup
    renderer/                        # Package: retro-renderer
      Cargo.toml
      src/
        lib.rs                       # GPU resources and drawing entry point
        scene.rs                     # Renderer-owned drawing data
        text.rs                      # Fonts, glyph cache, fixed cell placement
        effects.rs                   # Effect selection and GPU resources
      shaders/
        crt.wgsl                     # Begin with inexpensive static effects
      tests/
        rendering.rs                 # Deterministic image/geometry checks
    app/                             # Package: retro-app
      Cargo.toml
      src/
        main.rs                      # Small entry point
        lib.rs                       # App logic available to integration tests
        app.rs                       # Window event loop and coordination
        tabs.rs                      # Tabs and active-tab state
        layout.rs                    # Split tree, pane rectangles and focus
        scheduler.rs                 # Redraw reasons and animation deadlines
        config.rs                    # TOML schema, validation, reload
        platform.rs                  # Clipboard and OS integration boundary
        ui.rs                        # Tab bar, split handles, messages
      tests/
        layout.rs
        config.rs
        scheduler.rs
  assets/
    presets/                         # Only presets that pass the resource gate
  benchmarks/
    scenarios.toml                   # Workloads and measurement parameters
    fixtures/                        # Fixed output streams/configurations
  docs/
    architecture.md
    performance.md                   # Reproduction procedure and measured limits
    development.md                   # Command catalog, editor/debug setup
    ai-workflow.md                   # Task brief and handoff contract
    releasing.md                     # Bot setup, release gates, recovery
    decisions/                      # Durable accepted architecture decisions
  docs/AI/
    README.md                        # Plan index and execution order
    architecture/                    # Shared architecture reference
    implementation/                  # Ordered subtask plans
  .worktrees/                        # Ignored task checkouts
```

Add modules and tests when their milestone is implemented; do not create empty scaffolding for every future file.
Do not introduce a generic shared/core crate, plugin system, or backend trait framework without a concrete need.
There remain three product crates. `xtask` is a fourth workspace package for development automation only.

## Developer experience and AI workflow

### One command interface

Use a small Rust `xtask` package with a Cargo alias. Keep it independent of the GPU/application crates so it starts quickly.
The underlying commands are ordinary Cargo commands.
Use the same task implementations locally and in GitHub Actions. YAML should orchestrate jobs rather than duplicate checks.

Proposed task contract:

| Command | Purpose |
| --- | --- |
| `cargo xtask doctor` | Report pinned tools, OS libraries, display availability, and optional GPU/release prerequisites |
| `cargo xtask check` | Format check, Clippy, headless unit/integration/doc tests, and config/shader/workflow validation |
| `cargo xtask check --package retro-terminal` | Focused edit/test loop; CI still runs the full required set |
| `cargo xtask run` | Launch the app with explicit development configuration and logs |
| `cargo xtask worktree create <task> --base <ref>` | Create an isolated branch/worktree; tracked plans come with it |
| `cargo xtask worktree list` | Show worktree paths, branches, and dirty state |
| `cargo xtask worktree remove <task>` | Remove only a finished, clean checkout; preserve uncommitted work and notes |
| `cargo xtask perf --candidate <binary> --baseline <binary> --suite <file> --output <file>` | Run matched resource measurements and emit results |
| `cargo xtask package --check` | Build/test distribution layout locally, with no tagging, uploading, or publishing |

Implement these as maintained project tooling with a help command and useful exit codes.
Pass process arguments directly; do not assemble shell command strings from task names or paths.
Support a stable JSON summary alongside human output for check/performance tasks, including failures, skips, and artifact paths.
Write logs/results beneath the current worktree's `target/`; distinguish unsupported hardware from a test failure.
Pin the Rust toolchain, lockfile, auxiliary tool versions, and CI actions. Use locked dependency resolution in checks/builds.
`doctor` must report missing tools and exact setup instructions without installing system packages or changing global settings.
Keep standard `cargo test`, `cargo clippy`, and editor Rust tooling usable without a custom environment.
Document rust-analyzer, debugger setup, runtime system-library requirements, and the same tasks for any editor.
Optional commit hooks may call the quick checks, but CI is authoritative and hooks must not create a second ruleset.

### Worktree lifecycle

Use one task/branch per worktree under `.worktrees/<task>`, with the main checkout kept available for review.
Use the agreed base reference; do not hardcode `origin/main` for dependent work or assume a fetch happened.
Validate branch/path names, refuse an existing destination, and use Git's worktree commands rather than copying repositories.
Each worktree gets its own Cargo target directory, runtime config, logs, snapshots, and benchmark artifacts.
Share Cargo registry/download caches; do not share a mutable target directory between concurrent tasks.
An optional compiler cache can be added after measurement if compilation cost warrants it.
Never alter global Git configuration or run a force-clean as part of setup/cleanup.
Before removal, check tracked changes, untracked files, and task notes; refuse destructive cleanup instead of using `--force`.
Do not delete branches automatically. Record any retained worktree and why it remains.

Plans under `docs/AI/` are tracked by Git, so every worktree gets them without a transfer step.
`worktree remove` refuses when the worktree has uncommitted, untracked, or ignored notes. With `--force` it first copies
changed, untracked, and ignored files under `docs/AI/` to `target/xtask/rescued/` in the main checkout.
Do not copy arbitrary secrets, local application settings, or other ignored directories.
Keep durable decisions in tracked `docs/decisions/` and architecture documentation so a fresh clone has essential context.
The main README must explain the initial clone/doctor/worktree/check sequence and the prerequisite of a usable Git base.

### Shared instructions and handoff

Use a concise root `AGENTS.md` as the shared source for architecture boundaries, commands, tests, and worktree rules.
Record the user's no-unasked-commit/push/MR rule and requirement to use project verification tooling.
Do not require an agent's private memory, installed connector, or a particular model to execute a task.
Where an AI client requires another instruction file, keep that file a thin reference to the shared instructions.
Add crate-local instructions only when a crate has a distinct rule; avoid copied command catalogs that drift.

The task brief must state the goal, acceptance criteria, scope, base commit, relevant files, and exact verification tasks.
The handoff must state changed files, decisions, commands actually run, results, unrun checks, and remaining blockers.
Include a patch/diff reference when work was not committed. Never claim that unrun GPU checks passed.
Provide small deterministic terminal fixtures and optional renderer captures to make regressions reproducible without a live personal shell.
Logs must omit terminal contents and sensitive environment values by default; explicit diagnostic captures remain local and ignored.
AI agents use the same checks and review criteria as humans. No automatic agent spawning or autonomous merge is required.

## GitHub CI architecture

Use a short feedback loop for each PR and a separate controlled performance lane.

1. `ci.yml`: run on PRs and the protected default branch, including generated release PRs.
   Invoke shared `xtask` checks for formatting, lint, tests, shader/config validation, dependency policy, and workflow validation.
   Select and pin a workflow linter and dependency checker (for example `actionlint` and `cargo-deny`) during setup.
   Headless checks must explicitly select headless tests; do not silently skip a required GPU test inside a green job.
   Give the aggregate required check a stable name and ensure filtered/skipped jobs cannot leave it falsely green or permanently pending.
2. Build/cache: key caches by platform, toolchain, target, dependency lockfile, and relevant build configuration.
   Cancel obsolete PR runs, bound job duration, and upload actionable logs on failures.
   Keep release builds reproducible and independent of untrusted PR cache writes.
3. `performance.yml`: a reusable workflow on a controlled Linux GPU/display machine.
   Serialize runs per physical GPU. Match the pinned Alacritty baseline and capture system metadata.
   Test real display backends as available; software rendering can test correctness but cannot satisfy the hardware budget.
   Routine trusted default-branch runs find regressions; the release run is mandatory for the exact artifact being published.
4. Repository rules: require CI on PRs and disable direct release-tag changes outside the release process.
   If a merge queue is enabled later, add the required merge-group trigger rather than assuming PR checks cover it.
   Keep dependency/action updates small and reviewable; do not auto-merge changes merely because a bot created them.

Run untrusted/fork PR jobs on hosted disposable workers with read-only repository permissions and no release credentials.
Do not execute fork code on the persistent GPU runner automatically.
The protected-branch release path may use the controlled runner; it has no publication credentials.
Keep publish permissions in a separate hosted job that consumes validated artifacts and does not execute their code.
An unavailable GPU runner or unsupported counters block publication and produce an explicit status.
Once additional OS releases are requested, add their build/runtime jobs; cross-compilation alone does not establish support.

## Automated release architecture

Recommend Release Please for the release PR/version/changelog lifecycle, and pinned `dist` for artifact building.
Use a single application release version and `vX.Y.Z` tag namespace, with one changelog.
The three product crates inherit the workspace version and remain unpublished on crates.io.
Exclude `xtask` and internal libraries from distribution; explicitly include the `retro-term` application.
Validate Release Please's Rust workspace updater against the actual inherited versions and lockfile in a dry run.
Recommend squash merges with Conventional Commit PR titles so release notes/version changes are predictable.
Check those titles in PR CI without requiring every local work-in-progress commit to follow the convention.

Flow after ordinary development merges:

1. `release-please.yml` maintains one generated release PR with the proposed workspace version, lockfile changes, and changelog.
2. The user reviews and merges that PR. Its normal required checks must already pass.
3. Release Please creates the version tag and a draft GitHub release for that merge commit.
   Configure `draft: true` and `force-tag-creation: true` with a pinned version supporting those settings;
   draft GitHub releases alone do not guarantee that the tag exists yet.
4. Use Release Please's returned tag and commit SHA to call reusable `release.yml` directly.
   Verify that tag, release, source SHA, and workspace version agree. Never build whichever default-branch HEAD exists later.
5. Recheck/build/package that exact revision with the pinned toolchain and locked dependencies.
   Produce Linux archives, checksums, version metadata, licenses, example configuration, and required application resources.
   Document runtime libraries and glibc compatibility. Select target architectures from actual build/test coverage.
   Distribution-specific formats such as AppImage/deb/rpm are later additions, not assumed to work from a tarball build.
6. Run package smoke tests and the full resource gate against the actual packaged executable, for every shipped preset.
   Bind reports to the source SHA, executable checksum, toolchain/configuration, hardware, and baseline version.
   Missing, stale, failing, or inconclusive evidence cannot authorize publication.
7. The publication job attaches the validated artifacts/checksums and publishes the existing draft release automatically.
   No extra manual publishing step is needed after the release PR merge when all gates pass.

Use a narrowly scoped GitHub App installation token for Release Please to make bot-created PR checks run automatically.
GitHub's default token has event-trigger restrictions; do not depend on an implicit tag/release event chain.
The reusable release call is the sole publication entry point for a normal release; do not also publish from a tag-push workflow.
Use `dist` as an artifact builder in this orchestration, without a competing tag/version/release creator.
Keep credentials in GitHub configuration, never in the repository or AI prompts. Credential setup is a deployment task for later.

Serialize releases per version and make retries idempotent.
A failure leaves the draft unpublished with diagnostics; a retry validates the same tag and artifact identity.
Never move a published tag or overwrite different released bytes. A defective published version needs a new corrective release.
Provide a manual recovery workflow that selects an existing release tag and reruns the same gates; it is not a bypass.
Exercise the pipeline with a non-publication rehearsal before enabling the bot's publication job.
The configured bot is authorized to perform its release lifecycle after merge; this does not grant an interactive AI agent blanket push/merge rights.

## Ownership and data flow

The app depends on `terminal` and `renderer`. Those crates do not depend on each other.
The app maps terminal snapshots to renderer-owned scene data.
No `wgpu` or `winit` types should leak into the terminal API.
The renderer does not spawn shells or interpret terminal escape sequences.

Output path: shell -> PTY -> terminal parser/state -> visible snapshot -> scene -> text image -> CRT effects -> window.
Input path: window event -> focused pane/action routing -> terminal input encoding -> PTY -> shell.
Terminal protocol replies must also reach the PTY; they are not drawing events.

Use stable `TabId`, `PaneId`, and `SessionId` values.
A window owns tabs; each tab owns a split tree; each leaf refers to a pane with one terminal session.
A split stores an axis, a ratio, and two children. This supports nested horizontal and vertical splits.
Closing one pane cleans up its session and collapses the remaining tree as needed.
Tabs and splits are provided by the application and do not require an external multiplexer.
Do not add session persistence or a daemon as part of this scope.

The main thread handles window events, app state, and GPU submission.
Start one existing PTY I/O worker per session as the initial implementation.
Use the engine's synchronization mechanism for terminal state; hold its lock only for bounded state operations/snapshots.
Release the lock before text shaping, GPU preparation, presentation, clipboard calls, or other blocking work.
Coalesce wakeups and redraw requests. Do not copy the entire scrollback on each output event.
Large pastes and output bursts must not create an additional unbounded application queue.

Hidden tabs continue to read and parse shell output. They do not prepare text or submit GPU work.
On activation, take a fresh snapshot and fully invalidate the visible view.
Resize both terminal state and PTY dimensions using the same calculated rows/columns.
Keep cell coordinates, physical pixels, and display scaling distinct.

## Rendering and resource policy

Share one GPU device/queue and a bounded font/glyph cache across panes.
Render visible panes into the active tab's terminal area. Start with one effect pipeline over that area.
Draw the tab bar and other controls after CRT effects so controls stay readable.
Do not allocate a full independent graphics stack for each pane.

Separate text-cache invalidation, effect invalidation, and the need to present a frame.
Use terminal damage to update changed content, but also invalidate for cursor, selection, scrolling,
configuration, resizing, font changes, and device/surface recovery.
Alacritty's damage API does not include every UI-controlled element, particularly selection.
Reuse cached content where correct; redraw neighboring pixels or the whole effect surface when an effect requires it.
Never apply row-level damage blindly to a blur or distortion pass.

Use `winit` wait/deadline scheduling. Do not run an unconditional redraw loop.
With unchanged content and no active visual timer, submit no new application frames.
Cursor blinking is a timed visual change; match cursor settings in comparisons and test both blinking and non-blinking cases.
Pause rendering when the window cannot be presented. Resume with proper invalidation.

Start the default preset with tint, scanlines, and other inexpensive static operations.
Add blur/bloom, history textures, distortion, or temporal effects only after measurement.
Pass the original text image through directly when effects are disabled.
If distortion is included, use the matching coordinate transform for selection and mouse hit testing.

Opt-in animation does not override the resource ceiling.
Start by evaluating effects on frames that are already required for visible terminal changes.
Independent animation timers need separate measured proof that they fit the limit.
Do not promise continuously animated idle presets. Do not silently disable animation only during benchmarks.
Document any adaptive behavior as normal product behavior and verify it in the same way.
Clamp supported effect parameters to tested ranges; unbounded user-selected costs cannot carry the same performance claim.

Keep scrollback bounded and glyph caches evictable. Release pane/session resources after close.
Preserve the engine's terminal cell widths when shaping text; do not treat a terminal row as freely flowing prose.
Check combining marks, wide characters, fallback glyphs, and box drawing before adding text features.

## Text configuration

Recommend TOML at the Linux user config location (`$XDG_CONFIG_HOME/retro-term/config.toml`,
falling back to `$HOME/.config/retro-term/config.toml`). Resolve paths through a platform boundary.
Use one typed configuration model for the file, runtime state, and any future settings UI.
Define defaults -> selected preset -> explicit user values as the precedence order and test it.
Provide a commented example and clear errors for invalid keys, types, and ranges.

Add an explicit reload command first. Parse and validate the whole candidate configuration before applying it.
On a reload error, retain the last valid settings and display a useful error.
Document which changes affect existing sessions and which apply only to new shells.
Apply font/layout changes and PTY resizing together.
A file watcher and graphical settings editor are later conveniences.
If a GUI eventually writes the file, preserve user comments with a suitable TOML editor and use atomic replacement.

## Performance acceptance and measurement

This is a release test, not a promise that the app can measure an invisible Alacritty baseline at runtime.
For each matched scenario and shipped preset, require independently:

```text
candidate CPU time <= 1.20 * baseline CPU time
candidate RAM      <= 1.20 * baseline RAM
candidate GPU time <= 1.20 * baseline GPU time
```

Operationalize CPU as emulator-process user plus system CPU time over the measured interval, across all threads.
Measure steady-state resident memory and peak resident memory; compare each against its matching baseline statistic.
Keep shell/workload process costs separate so they cannot hide terminal overhead.
Use attributable GPU engine busy time over the same interval; report individual engines and the chosen aggregation.
Record GPU memory as supporting evidence where counters are available.
GPU busy time is a utilization proxy, not an energy measurement; record frequency/power conditions as well.

On supported Linux drivers, DRM fdinfo can supply client GPU counters.
Deduplicate shared client handles and follow driver counter semantics.
Use the same measurement backend for candidate and baseline, not candidate-only shader timestamps against unrelated baseline data.
If hardware lacks suitable counters, implement a supported profiler integration in the project tooling or mark the GPU gate unverified.

Match hardware, driver, desktop/display backend, display scale, refresh rate, font, cell size,
terminal dimensions, scrollback bound, shell/workload, cursor behavior, and run duration.
Use release builds and a recorded released Alacritty version/configuration.
Run candidates sequentially under controlled conditions; record warm-up and repeat enough runs to assess noise.
Keep raw samples and ratios. A noisy or unresolved result is inconclusive, not a pass.
Do not hide a failing scenario inside an average across presets or workloads.

If a baseline is zero, do not divide by zero or invent a minimum denominator.
A genuine zero baseline permits no added work for that metric.
If it is below measurement resolution, extend/adjust the measurement and report uncertainty rather than granting an allowance.
Both-zero readings at inadequate resolution do not prove a percentage target.

Required scenarios: startup, unchanged idle, cursor blinking, normal typing, sustained output/scrolling,
full-screen redraw, resize, multiple visible panes, hidden tabs receiving output, and repeated pane open/close.
Include non-blinking idle for every animated preset to expose independent animation cost.
For multiple sessions, compare equivalent Alacritty sessions/windows with the same total visible cell area and workloads;
record the topology difference and process aggregation rather than comparing many shells against one idle shell.
Hidden-session baselines must have equivalent visibility. If the environment cannot provide a fair comparison, report it unverified.

Also record output completion time, responsiveness, and correctness.
Do not meet the CPU/GPU budget by dropping terminal output, reducing required functionality, or allowing unbounded latency.
No performance result is available yet. No percentage improvement may be advertised until this suite runs.

## Subtask plans

The ordered implementation steps have moved to individual plans under `../implementation/`.
Start at [the execution index](../README.md). Each plan contains prerequisites, scope, files, steps, acceptance criteria, verification, and handoff requirements.
Do not execute all subtasks merely because one subtask is requested.
The first task is [repository setup](../implementation/01-repository-setup.md).

## Verification commands for the future implementation

These are commands the implementation must provide. They are not runnable against today's empty workspace.
Run them from the implementation worktree, using the project's own Cargo tests, checks, and maintained performance CLI.

```sh
cargo xtask doctor
cargo xtask check
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-targets
cargo test --locked --workspace --doc
cargo build --locked --release -p retro-app --bin retro-term
cargo xtask run
cargo xtask perf --candidate target/release/retro-term --baseline alacritty --suite benchmarks/scenarios.toml --output target/perf/report.json
cargo xtask package --check
```

The `xtask` commands are the proposed automation contract; implement them before using them as verification.
The raw Cargo checks document the core operations behind `xtask check`; do not run both sets redundantly.
`xtask package --check` is a local check and never publishes. GitHub CI calls the same `cargo xtask` commands.
The performance suite needs a desktop session and suitable GPU counters. Do not report it as passed on a headless run that skipped it.
Run appropriate checks once after each meaningful change; rerun broader checks only for new changes or unresolved failures.
No extra servers, ad-hoc verification scripts, or fabricated benchmark numbers.
Record what ran, what did not run, and why.

## Out of scope

- Implementing a new escape-sequence parser/terminal engine without evidence that reuse is unsuitable.
- Continuous idle animation that violates the agreed resource cap.
- Guaranteed performance on untested hardware or unbounded effect parameters.
- A plugin system, remote session daemon, session persistence, or web frontend.
- macOS/Windows release support in the Linux milestone.
- A full settings editor before performance, text configuration, tabs, and splits.
- Interactive-agent commits, pushes, publication, or merge requests without a further user request.
  The designed release bot's post-merge lifecycle is in scope once configured and enabled.

## Primary references checked during planning

- [cool-retro-term](https://github.com/Swordfish90/cool-retro-term)
- [Alacritty terminal library](https://docs.rs/alacritty_terminal/latest/alacritty_terminal/)
- [PTY event loop](https://docs.rs/alacritty_terminal/latest/alacritty_terminal/event_loop/struct.EventLoop.html)
- [Terminal state and damage API](https://docs.rs/alacritty_terminal/latest/alacritty_terminal/term/struct.Term.html)
- [Terminal event callbacks](https://docs.rs/alacritty_terminal/latest/alacritty_terminal/event/enum.Event.html)
- [winit event-loop scheduling](https://docs.rs/winit/latest/winit/event_loop/enum.ControlFlow.html)
- [wgpu](https://docs.rs/wgpu/latest/wgpu/)
- [glyphon](https://github.com/grovesNL/glyphon)
- [TOML configuration parsing](https://docs.rs/toml/latest/toml/)
- [TOML editing](https://docs.rs/toml_edit/latest/toml_edit/)
- [Linux DRM client usage counters](https://docs.kernel.org/gpu/drm-usage-stats.html)
- [Alacritty configuration](https://alacritty.org/config-alacritty.html)
- [Git worktrees](https://git-scm.com/docs/git-worktree)
- [Cargo xtask pattern](https://github.com/matklad/cargo-xtask)
- [Release Please action](https://github.com/googleapis/release-please-action)
- [Release Please manifest configuration](https://github.com/googleapis/release-please/blob/main/docs/manifest-releaser.md)
- [Release Please draft/tag configuration schema](https://github.com/googleapis/release-please/blob/main/schemas/config.json)
- [GitHub workflow triggering and token behavior](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow)
- [GitHub self-hosted runners](https://docs.github.com/en/actions/concepts/runners/self-hosted-runners)
- [dist packaging](https://github.com/axodotdev/cargo-dist)
- [dist configuration](https://axodotdev.github.io/cargo-dist/book/reference/config.html)
- [GitHub Actions workflow validation](https://github.com/rhysd/actionlint)
- [Cargo dependency checks](https://embarkstudios.github.io/cargo-deny/)

