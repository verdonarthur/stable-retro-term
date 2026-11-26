# Text configuration and reload

Status: implemented locally (steps 1–8). Interactive smoke check and performance are unverified.
Dependencies: 06 implemented locally (PR #7, `feat/static-crt`). 04 and 05 are in the same stack.

## Context and shared rules

Read [the architecture](../architecture/retro-terminal.md) (section "Text configuration"),
[the execution index](../README.md), and [the AI workflow](../../ai-workflow.md).

Linux first. Reuse `alacritty_terminal`. Three product crates plus development-only `xtask`.
Keep the crate boundaries in `CLAUDE.md`: the config model, file paths, and reload live in
`retro-app`. The renderer gets plain renderer types (mode, font choice), never config types.
The terminal crate gets `SessionOptions`, never config types. `xtask` does not depend on product crates.
Make surgical changes. Do not commit, push, or open a PR unless the user asks.
Use project tooling only. No ad-hoc scripts, no extra servers. Report checks that did not run.

## Goal

One typed TOML model for the configuration file, the built-in presets, the runtime state,
and the future settings UI (task 11). An explicit reload action applies a valid file and
keeps the last valid configuration when the file is invalid.

## Settled decisions

- D1. Base: `41200c3cca804010356bb1941115cddd61f3634e` (`origin/feat/static-crt`, open PR #7,
  which stacks on #6 and #5). The PR for this task targets `feat/static-crt`.
- D2. Worktree: `.worktrees/07-text-configuration` on branch `07-text-configuration`. It exists.
- D3. Performance: this host is the measurement machine (AMD Radeon 890M, RADV) but has no Alacritty 0.17.0. `cargo xtask perf` cannot
  produce a valid result here. Record perf as **unverified**. Never claim a pass.
  The default settings must keep the rendered output identical to the base, so the
  Task 05/06 measurements stay comparable.
- D4. Task 10 (release) is planned in parallel in another worktree. Task 08 (tabs and splits)
  comes after this task and builds on this config model. Task 11 (settings UI) reuses the
  same model. Design for that: one `Config` type, serde `Serialize` + `Deserialize`, no
  second settings store, no UI-specific fields.
- D5. Precedence (architecture): built-in defaults -> selected preset -> user file -> CLI flags.
  The CLI layer is only the existing flags (`--plain`, program after `--`). See Q6.
- D6. No file watcher, no settings editor, no TOML writing (no `toml_edit`) in this task.
- D7. Validate the whole candidate (parse, unknown keys, types, ranges, preset name, font
  family) before any change to the running state. On failure keep the old state and show the error.

## Code facts at the base (read before work)

- `crates/app/src/main.rs`: `parse()` accepts `--development`, `--plain`, `--help`/`-h`,
  and `-- COMMAND ARGS`. Duplicates are rejected. `main()` calls
  `retro_app::run_with_mode(development, plain, program)`. Test:
  `keeps_default_shell_and_forwards_arguments_after_separator`.
- `crates/app/src/lib.rs`: all modules are `#[cfg(target_os = "linux")]`. Exports
  `run`, `run_with_mode`, `run_with_program` (only `main.rs` calls `run_with_mode`).
- `crates/app/src/app.rs`:
  - `App { development, plain, program, .. }` and `State { renderer, session, grid, font_size, plain, .. }`.
  - `State::new` creates the window, calls `create_renderer(events, &window, development, plain)`,
    then `scene::font_metrics(scale)`, `renderer.cell_width(..)`, `scene::grid_size(..)`,
    and `Session::spawn(SessionOptions { size, program, working_directory: None, scrollback_lines: 10_000 }, ..)`.
  - `State::resize(size)` recomputes font metrics from the scale factor, reuses
    `self.grid.cell_width` only if `font_size` and `line_height` did not change, calls
    `renderer.resize` and then `session.resize(grid)` (terminal grid + PTY together),
    and clears `selection_start`.
  - `State::draw` handles `Frame::RecreateDevice` by calling `create_renderer` again with
    `self.plain`. `State::window_event` routes keys through `input::key(..)` -> `State::action`.
  - The window title is `TITLE` and the shell changes it with `TitleChange`.
- `crates/app/src/scene.rs`: `font_metrics(scale) -> (font_px, line_px)` hard-codes 14 px font
  and 19 px line at scale 1. `convert(snapshot, font_size, cursor_visible, preedit)` already
  draws a multi-cell preedit overlay (one fill + one glyph cluster), a pattern the error
  banner can reuse (Q8). Test `physical_grid_and_selection_use_the_same_cells` asserts
  `font_metrics(1.5) == (21, 29)`.
- `crates/app/src/input.rs`: `Action { Copy, Paste, Bytes }`. Ctrl+Shift+C/V are app
  shortcuts. Ctrl+Shift+<other letter> currently sends the same control byte as Ctrl+<letter>.
- `crates/app/src/scheduler.rs`: `BLINK` is a fixed 500 ms. Not configurable in this task.
- `crates/app/src/platform.rs`: Wayland graphics instance and clipboard. Linux-only.
- `crates/renderer/src/effects.rs`: `RenderMode { Plain, StaticCrt }`. `SCALE = 1.025` and
  `CURVATURE = 0.018` are constants. `shaders/crt.wgsl` hard-codes tint `(0.95, 1, 1)` and
  scanline gain `0.88`. `xtask/src/perf/wayland.py` (`crt_colors`, `crt_point`) models the same
  constants for frame capture. Changing them breaks the perf capture probe.
- `crates/renderer/src/lib.rs`: `SurfaceRenderer::with_mode(instance, target, mode)` always
  uses `Fonts::system()`. `recreate_surface` rebuilds `GridRenderer` with `Fonts::system()`.
  `cell_width(font_size, line_height)` measures "M".
- `crates/renderer/src/text.rs`: `Fonts::system()` picks the fontconfig monospace family
  (with a fallback to the first monospaced face). `Fonts::from_data` is for tests.
  There is no way to select a family by name.
- `crates/terminal/src/session.rs`: `SessionOptions { size, program, working_directory, scrollback_lines }`.
  Scrollback above `i32::MAX - u16::MAX` is `InvalidScrollback`. `Session::resize(GridSize)`
  resizes the PTY and the grid together. There is no API to change scrollback or colors
  of a live session. `engine.rs::palette_color` hard-codes the 16-color palette.
- `xtask/src/perf/run.py::terminal()` launches the candidate as
  `[candidate, *candidate_flags(request), "--", fixture...]` with the caller's environment
  (`terminal_env` copies `os.environ`). After this task, a user config file in
  `$XDG_CONFIG_HOME` would silently change perf runs. Step 7 closes this hole.
  `test_perf.py::test_candidate_mode_flag` covers `candidate_flags`. `xtask/tests/perf.rs`
  runs those Python tests inside `cargo xtask check`.
- `benchmarks/fixtures/alacritty.toml`: Source Code Pro 10.5 pt (14 px at scale 1),
  scrollback 10 000, block cursor, no blink. Defaults must stay equal to this.
- `xtask/src/run.rs`: `cargo xtask run` runs `retro-term --development` (static CRT, not plain).
- Lockfile already has `serde 1.0.229`, `toml 1.2.2+spec-1.1.0` (from `xtask`). No `assets/`
  directory exists. `crates/app/tests/` does not exist.
- Workspace lints: `unsafe_code = "deny"`, Clippy `all = warn`, and the check runs Clippy with `-D warnings`.

## Configuration model

File (recommended defaults shown; final key set depends on Q3):

```toml
preset = "static-crt"          # built-in preset name, see Q4

[effects]
crt = true                     # false = plain bypass, same as --plain

[font]
family = ""                    # "" = system monospace (current behavior), see Q9
size = 14                      # pixels at scale 1, see Q9 for units and range
line_height = 19               # pixels at scale 1, >= size

[terminal]
scrollback_lines = 10000       # 0..=100000, new sessions only

[shell]
program = ""                   # "" = $SHELL / passwd shell (current behavior), new sessions only
args = []
working_directory = ""         # "" = inherit, new sessions only
```

Types in `crates/app/src/config.rs` (platform-neutral, no `cfg(target_os)`):

- `Config` — the resolved, validated model. `Default` = current hard-coded behavior.
  Derives `Clone, Debug, PartialEq, Serialize, Deserialize`. Sub-structs `Effects`, `Font`,
  `Terminal`, `Shell`. This is the only settings type the app, task 08, and task 11 use.
- `Layer` — one partial layer (preset file or user file): every field `Option<_>`,
  `#[serde(deny_unknown_fields)]` on every struct. `Layer::parse(text) -> Result<Layer, ConfigError>`.
- `Config::resolve(user: Layer, overrides: &Overrides) -> Result<Config, ConfigError>`:
  `Config::default()` <- preset layer (from `user.preset` or the default preset) <- user layer
  <- `Overrides`, then `validate()`.
- `Overrides { plain: bool, program: Option<Program-like (String, Vec<String>)> }` — CLI layer,
  kept for every reload.
- `ConfigError` — `Display` gives the file path, the TOML line/column from `toml::de::Error`,
  the dotted key, and the allowed range or names. Variants: `Io`, `Parse`, `Range { key, message }`,
  `UnknownPreset { name, known }`, `Font { family }`.
- `Changes::between(old: &Config, new: &Config) -> Changes { renderer, layout, new_sessions }`
  — what a reload must do. `renderer`: `effects.crt` or `font.family` changed.
  `layout`: `font.size` or `font.line_height` changed (or `renderer`). `new_sessions`:
  `terminal` or `shell` changed (applies at next session spawn only).
- `user_config_path(xdg_config_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf>` —
  pure XDG rule: use `$XDG_CONFIG_HOME/retro-term/config.toml` when the value is non-empty and
  absolute, else `$HOME/.config/retro-term/config.toml`, else `None`.
  `platform::config_path()` calls it with `std::env::var_os` (the platform boundary).
- `load(source: &Source, overrides) -> Result<Config, ConfigError>`. `Source::Default(Option<PathBuf>)`:
  a missing file gives defaults. `Source::Explicit(PathBuf)` (from `--config`): a missing file is an error.

Presets: TOML files with the `Layer` schema minus `preset`, embedded with `include_str!`
(location per Q4). Initial set: `static-crt` (empty layer = defaults) and `plain`
(`[effects] crt = false`). Both match the two modes that Task 05/06 already measure.

## Steps

Decisions are recorded below. Execute in order.
Run `rtk cargo xtask check -p retro-app` after each app step to keep the loop short.

1. **Dependencies.** In `crates/app/Cargo.toml` add a plain `[dependencies]` section (not the
   Linux-only one) with `serde = { version = "1", features = ["derive"] }` and `toml = "1.2"`
   (the same requirements as `xtask/Cargo.toml`; per Q2). Run `rtk cargo check -p retro-app`
   once to update `Cargo.lock`, then `rtk git diff Cargo.lock`: only the `retro-app`
   dependency list may change, with no new crate versions. Do not add `dirs`, `toml_edit`, or a file-watcher crate.
2. **Config model.** Add `crates/app/src/config.rs` with the types above and
   `pub mod config;` in `lib.rs` without a `cfg` gate, so tests run headless. Keep
   `Program` out of this file (it is Linux-only); store the shell as `String` + `Vec<String>`
   and convert in `app.rs`. Validation ranges (per Q3/Q9): `font.size` 6..=72,
   `font.line_height` `size..=2*size`, `terminal.scrollback_lines` 0..=100_000,
   `shell.args` only when `shell.program` is set. Preset files: add `presets/static-crt.toml`
   and `presets/plain.toml` at the Q4 location; look them up in a `const PRESETS: [(&str, &str); 2]`.
3. **Example and user docs.** Add the example file (Q5 location) with every key at its default
   value and a short comment per key. Add `docs/configuration.md`: path resolution, precedence,
   every key with type/default/range, presets, `--config`, the reload shortcut, what applies live
   and what applies to new sessions only, and how errors show. Link it from `README.md`
   and `docs/development.md` (command table and "Run the plain terminal" section).
4. **CLI.** In `crates/app/src/main.rs` add `--config PATH` (Q6). Reject a duplicate flag and a
   missing value with the existing usage error (exit 2). Update `USAGE`. Replace the call to
   `run_with_mode` with a new `retro_app::run_with_config(development, source, overrides)`
   that takes a `config::Source` and `config::Overrides`. Keep `run`, `run_with_program`, and
   `run_with_mode` as thin wrappers (they are public API). Startup with an invalid file: per Q7.
5. **Renderer font and mode switch.** In `crates/renderer`:
   - `text.rs`: add `Fonts::system_family(family: Option<&str>) -> Result<Fonts, FontError>`
     that keeps the current `system()` logic for `None` and checks that `family` exists in
     the fontdb for `Some`. `system()` calls it with `None`.
   - `lib.rs`: `SurfaceRenderer` stores the chosen family. Add
     `SurfaceRenderer::reconfigure(&mut self, mode: RenderMode, fonts: Fonts)` that rebuilds
     `GridRenderer` on the existing device and queue. `recreate_surface` reuses the stored
     family instead of `Fonts::system()`. Change `with_mode` to take the `Fonts` (or add a
     `with_fonts` constructor and keep `with_mode` as a wrapper).
   - Keep `effects.rs` constants unchanged (Q10). Add a unit test that `system_family(Some("no such font"))`
     returns an error; it must not depend on installed fonts.
6. **App integration and reload.** In `crates/app/src/`:
   - `scene.rs`: change `font_metrics(scale)` to `font_metrics(scale, size, line_height)`.
     Keep the existing assertion with `(14, 19)`.
   - `input.rs`: add `Action::ReloadConfig` for the Q11 shortcut, checked in the same
     Ctrl+Shift block as copy/paste.
   - `app.rs`: `App` and `State` keep `config: Config`, `source: Source`, `overrides: Overrides`
     instead of `plain`. `State::new` uses the config for fonts, mode, metrics,
     `SessionOptions::scrollback_lines`, `program`, and `working_directory`. The device-loss
     branch in `State::draw` recreates the renderer from `self.config`.
   - Add `State::reload(&mut self) -> AppResult<()>`:
     1. `config::load(&self.source, &self.overrides)`; on `Err` show it (Q8) and return `Ok(())`.
     2. `Changes::between(&self.config, &candidate)`.
     3. If `changes.renderer`: build the new `Fonts` first (error -> show, keep old state, return);
        then `renderer.reconfigure(mode, fonts)`.
     4. Store `self.config = candidate` (only after steps 1–3 succeeded).
     5. If `changes.layout`: force a cell-width recompute (the cache in `resize` keys on
        `font_size`/`line_height`; also reset it when the family changed) and call
        `self.resize(self.window.inner_size())`, which resizes renderer, grid, and PTY together.
        Clear `previous_ime_cursor` and the selection start.
     6. `self.scheduler.invalidate()`. Clear any shown config error.
     7. If `changes.new_sessions`: show an info line "applies to new shells" (Q8 channel).
   - `State::action` handles `Action::ReloadConfig` by calling `reload`.
   - Error display per Q8. Recommended: a one-row banner on the last grid row, built in
     `scene::convert` like the preedit overlay, shown until the next successful reload or the
     next key press. Also print the full message to stderr.
7. **Perf isolation.** Add `benchmarks/fixtures/retro-term.toml` that pins every key to the
   defaults (or is the example file copy; per Q12). In `xtask/src/perf/run.py`:
   `candidate_flags(request)` returns `["--config", <fixture path>]` plus `--plain` when requested;
   record the fixture's sha256 in `report["identity"]` like `alacritty_config_sha256`.
   Pass the fixture path from `xtask/src/perf.rs` in the request (resolve it relative to the
   suite like `alacritty_config`; add `candidate_config` to `Suite` and `benchmarks/scenarios.toml`
   only if Q12 says so). Update `test_perf.py::test_candidate_mode_flag`.
   Update `docs/performance.md` with one line on the candidate config.
8. **Index and handoff.** Set this plan's status and the row in `docs/AI/README.md` from evidence
   only. Fill in the handoff below.

## Tests to add

All headless; they run in `cargo xtask check`.

`crates/app/tests/config.rs` (integration, uses `retro_app::config`):

- `defaults_match_the_base_behavior`: `Config::default()` = crt on, font family system,
  14/19 px, scrollback 10 000, no shell override.
- `empty_file_and_missing_default_file_give_defaults`.
- `missing_explicit_file_is_an_error` (`Source::Explicit` with a temp path that does not exist;
  use `std::env::temp_dir()` + a unique name, or add `tempfile` as a dev-dependency like `xtask`).
- `precedence_is_defaults_then_preset_then_user_then_cli`: preset `plain` sets crt off;
  user file sets `effects.crt = true` -> on; `Overrides { plain: true }` -> off;
  user `font.size` overrides the default; preset choice comes from the user file.
- `unknown_key_reports_key_and_line` (top level and nested, e.g. `[font] sizee = 3`).
- `wrong_type_reports_key` (`font.size = "big"`).
- `out_of_range_values_name_the_range` (font size 0 and 200, line height below size,
  scrollback above the limit, `shell.args` without `shell.program`).
- `unknown_preset_lists_known_names`.
- `example_file_equals_defaults` (`include_str!` the example; parse and resolve; compare with
  `Config::default()`).
- `every_builtin_preset_resolves`.
- `config_path_follows_xdg`: absolute XDG, relative XDG ignored, empty XDG ignored, HOME
  fallback, neither -> `None`.
- `changes_classify_live_and_new_session_settings`: crt -> renderer+layout; font size ->
  layout only; scrollback/shell -> new_sessions only; equal configs -> nothing.
- `failed_reload_keeps_the_previous_config`: a pure helper used by `State::reload`
  (for example `config::reload(current: &Config, result) -> (Config, Option<ConfigError>)`),
  so the keep-last-valid rule is tested without a window.
- `config_round_trips_through_toml` (`toml::to_string(&Config)` -> parse -> equal), so task 11
  can write the same model.

Unit tests in existing modules:

- `main.rs`: extend the CLI test: `--config path` accepted; `--config` without a value,
  duplicated `--config`, and `--config` after `--` (must be forwarded to the child) behave as specified.
- `scene.rs`: `font_metrics(1.5, 14, 19) == (21, 29)` and one non-default size;
  a banner test if Q8 picks the banner (last row, clipped to the grid width, no panic for a 1-column grid).
- `input.rs`: reload shortcut gives `Action::ReloadConfig`; Ctrl+R alone still sends `0x12`;
  copy/paste unchanged.
- `renderer/src/text.rs`: unknown family is an error.
- `xtask/src/perf/test_perf.py`: `candidate_flags` includes `--config <fixture>` in both modes.

Not automatable here: the live reload of a running window (needs Wayland). Cover it in the smoke check.

## Acceptance criteria

- With no config file, the app behaves exactly as at the base (same font metrics, mode,
  scrollback, shell). The perf candidate uses a pinned config file and ignores the user's file.
- A text file can select a preset and set every supported key. `--config` loads another file.
- The reload shortcut applies effects, font, and line-height changes to the running window,
  and resizes the grid and PTY together. Scrollback and shell changes apply to new shells
  only, and the app says so.
- An invalid file at reload (syntax, unknown key, wrong type, range, unknown preset, missing
  font family) leaves the session, renderer, and grid unchanged and shows a useful error.
- `config::Config` is the single settings model; tasks 08 and 11 can use it without a second store.
- `cargo xtask check` passes (or only fails on missing pinned tools, reported as such).
- Performance: **unverified on this host** (D3). Not a pass.

## Verification

Run in the worktree:

```sh
rtk cargo xtask check
rtk cargo xtask doctor            # only if check reports a missing tool
rtk cargo build --locked --release -p retro-app --bin retro-term
rtk cargo test --locked -p retro-renderer --test gpu_render -- --ignored --nocapture
rtk cargo xtask run
rtk cargo xtask perf --candidate target/release/retro-term --baseline alacritty --suite benchmarks/scenarios.toml --output target/perf/preflight.json --preflight
```

- The GPU test: run it on the hardware adapter. Optional software run with `RETRO_TERM_TEST_SOFTWARE_GPU=1` for an optional
  correctness run (`rtk proxy env RETRO_TERM_TEST_SOFTWARE_GPU=1 cargo test --locked -p retro-renderer --test gpu_render -- --ignored --nocapture`).
  The hardware run passed on this host (see Handoff).
- `cargo xtask run` needs a native Wayland session. If it cannot connect, record the log
  `target/xtask/logs/run.log` and mark the smoke check as not run. Smoke check when a session
  exists: start with no file; write a file that changes `font.size` and `effects.crt`; press the
  reload shortcut; confirm the grid and PTY resize (`stty size` changes) and the mode changes;
  add an unknown key and reload; confirm the error shows and the shell still works; fix the
  file and reload; confirm the error clears.
- The perf preflight is expected to exit 3 (Alacritty unavailable). Record the exit code and
  `target/perf/preflight.json`. Do not run the full suite as evidence on this host.

## Out of scope

File watcher, settings UI, TOML writing, user-defined preset files, CRT parameter tuning
(unless Q10 says otherwise), color/palette configuration, key-binding configuration,
cursor blink settings, tabs and splits (task 08), animation (task 09), packaging (task 10).

## Recorded decisions (user answers to Q1–Q12)

The user accepted every recommended option.

- **Q1. Config file path:** (a) `$XDG_CONFIG_HOME/retro-term/config.toml` when the value is
  non-empty and absolute, else `$HOME/.config/retro-term/config.toml`, else no file. Hand-written, no crate.
- **Q2. TOML crate:** (a) `serde 1` + `toml 1.2` in `retro-app`, same requirements as `xtask`. No new crates.
- **Q3. Settings in v1:** `preset`, `effects.crt`, `font.family`, `font.size`, `font.line_height`,
  `terminal.scrollback_lines`, `shell.program`, `shell.args`, `shell.working_directory`. Nothing else.
- **Q4. Presets:** (a) `static-crt` (default) and `plain`, TOML files in `assets/presets/`,
  embedded with `include_str!`. No user-defined preset files in v1.
- **Q5. Example file:** (a) `assets/config.example.toml`, tested to equal the defaults.
- **Q6. CLI:** (a) add `--config PATH`. `--plain` beats the file and stays active across reloads.
  The program after `--` beats `shell.program`. No `--no-config`.
- **Q7. Invalid file at startup:** (a) start with defaults and show the error in the window.
  An explicit `--config` path that does not exist: print the error and exit with failure.
- **Q8. Error display:** (a) one-row banner on the last grid row, built in `scene::convert`,
  dismissed by the next successful reload or key press, plus stderr.
- **Q9. Font:** (a) `family` (optional, must exist), `size` and `line_height` in pixels at
  scale 1, defaults 14/19. Ranges: size 6..=72, line height `size..=2*size`. No bold/italic families.
- **Q10. CRT parameters:** (a) only `effects.crt` on/off.
- **Q11. Reload shortcut:** (a) Ctrl+Shift+R. Key bindings are not configurable.
- **Q12. Perf candidate config:** (a) `benchmarks/fixtures/retro-term.toml`, passed with `--config`
  from `run.py`, sha256 recorded, `candidate_config` in the suite file. `cargo xtask run` uses
  the real user config.
- **Ceiling:** task 07 reports against 1.40× (current phase limit). The 1.20× target applies
  to the release gate (task 10). On this host the result is unverified (D3).
- **`docs/development.md` sentence on `cargo xtask run`:** fix it when step 3 edits the file.

## Contradictions found while planning

- Ceiling: the old version of this plan and `README.md` state 1.20× Alacritty. Task 06 and
  `docs/decisions/05-renderer-baseline.md` set 1.40× for the current phase. Ask the user which
  ceiling task 07 reports against. Either way the result here is unverified (D3).
- `docs/development.md` says `cargo xtask run` launches "with CRT effects disabled";
  `xtask/src/run.rs` passes only `--development`, so it launches static CRT. Out of scope;
  fix the sentence when step 3 edits that file, if the user agrees.
- `docs/AI/README.md` says task 07 needs "06 completed"; 06 is implemented locally but its
  hardware verification is pending. The user accepted stacking on `feat/static-crt` (D1).
- The architecture lists `assets/presets/` with "only presets that pass the resource gate".
  No preset has a passing gate on this host. `static-crt` and `plain` are the two modes that
  Task 05/06 already measure; task 10 must still check the gate before shipping them.

## Handoff

- Base: `41200c3cca804010356bb1941115cddd61f3634e` on `feat/static-crt`.
- Worktree: `.worktrees/07-text-configuration` (branch `07-text-configuration`). Nothing committed.
- Changed files:
  - Step 1: `crates/app/Cargo.toml`, `Cargo.lock` (only the `retro-app` dependency list; no new crate versions).
  - Step 2: `crates/app/src/config.rs`, `crates/app/src/lib.rs`, `assets/presets/static-crt.toml`, `assets/presets/plain.toml`.
  - Step 3: `assets/config.example.toml`, `docs/configuration.md`, `README.md`, `docs/development.md`
    (also fixes the "CRT effects disabled" sentence).
  - Step 4: `crates/app/src/main.rs` (`--config PATH`), `crates/app/src/app.rs` (`run_with_config`).
  - Step 5: `crates/renderer/src/text.rs` (`Fonts::system_family`, `FontError`), `crates/renderer/src/lib.rs`
    (`SurfaceRenderer::with_family`, `reconfigure`, stored family).
  - Step 6: `crates/app/src/scene.rs`, `crates/app/src/input.rs`, `crates/app/src/app.rs`, `crates/app/src/platform.rs`.
  - Step 7: `benchmarks/fixtures/retro-term.toml`, `benchmarks/scenarios.toml` (`candidate_config`),
    `xtask/src/perf.rs` (`Suite.candidate_config`, request field), `xtask/src/perf/run.py`
    (`candidate_flags`, `candidate_config_sha256`), `xtask/src/perf/test_perf.py`, `docs/performance.md`.
  - Tests: `crates/app/tests/config.rs` (15 tests), unit tests in `main.rs`, `scene.rs`, `input.rs`, `renderer/src/text.rs`.
- Decisions and deviations:
  - Q1–Q12 and the ceiling: see "Recorded decisions".
  - `SurfaceRenderer::reconfigure(mode, family: Option<&str>) -> Result<(), FontError>` builds the
    `Fonts` itself before it replaces anything, instead of taking a `Fonts` argument. Same atomicity,
    and the renderer can rebuild the fonts after a surface format change. `with_mode` stays as a wrapper.
  - `ConfigError` has no path; the app prefixes `config <path>:`. Parse errors show line, column, the
    source line (which names the key), and the TOML message.
  - At startup, any I/O error on an explicit `--config` file exits (not only "not found").
    A missing font family at startup falls back to the defaults (with CLI overrides) and shows the error.
  - `run`, `run_with_program`, and `run_with_mode` now read the user file (they wrap `run_with_config`
    with `Source::Default(config_path())`). The user confirmed this.
  - `shell.working_directory` is not checked at load. A bad directory fails the shell spawn at startup
    (same as any spawn error).
- Commands run and results:
  - `cargo xtask check`: fmt, check, clippy, test passed; 1 ignored test not run;
    deny, actionlint, zizmor missing (tools not installed). Rerun after the tool install request: same result, the three tools are still missing.
  - `cargo build --locked --release -p retro-app --bin retro-term`: passed.
  - `cargo test --locked -p retro-renderer --test gpu_render -- --ignored --nocapture` (hardware, RADV): passed (1 test).
  - `RETRO_TERM_TEST_SOFTWARE_GPU=1 cargo test --locked -p retro-renderer --test gpu_render -- --ignored --nocapture`: passed (1 test).
  - `cargo xtask perf ... --preflight`: exit 3, `target/perf/preflight.json` status `unverified`,
    reason "baseline executable unavailable: alacritty".
  - `cargo xtask run` (stopped by `timeout 20`): the window started; `target/xtask/logs/run.log` shows
    "Wayland GPU adapter: AMD Radeon 890M Graphics (RADV STRIX1) (Vulkan, IntegratedGpu)" and "frame presented".
- Not run, and why:
  - Interactive smoke check (edit file, Ctrl+Shift+R, `stty size`, error banner): needs a person at the keyboard.
  - Performance: Alacritty is not installed. Unverified against the 1.40× limit. Not a pass.
- Artifacts: `target/xtask/check.json`, `target/xtask/logs/`, `target/perf/preflight.json`.
- Blockers and next step: run the interactive smoke check and the perf suite on the measurement host, then review and commit.
