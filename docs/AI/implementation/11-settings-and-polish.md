# Settings UI and documentation polish

Status: implemented locally (uncommitted). `cargo xtask check` passed. Live smoke check not run; performance unverified (D5).
The user accepted every recommended option for Q1–Q13 (see "Decisions on Q1–Q13").
Dependencies: 08 implemented locally (PR #10, `08-tabs-and-splits`). 04–07 are in the same stack.
Task 10 (PR #9) is not in the base; see D3.

## Context and shared rules

Read [the architecture](../architecture/retro-terminal.md) (section "Text configuration"),
[the execution index](../README.md), [the AI workflow](../../ai-workflow.md),
[task 07](07-text-configuration.md) (the config model) and [task 08](08-tabs-and-splits.md) (`ui.rs`, scene composition).

Linux first. Reuse `alacritty_terminal`. Three product crates plus development-only `xtask`.
Keep the crate boundaries in `CLAUDE.md`: the settings panel, file editing, and file writing live in
`retro-app`. The renderer gets one `Scene` per frame and must not learn about the panel.
Make surgical changes. Do not commit, push, or open a PR unless the user asks.
Use project tooling only. No ad-hoc scripts, no extra servers. Report checks that did not run.

## Goal

1. A small keyboard-driven settings panel inside the window. It edits a few keys of the user
   file, writes the file safely (comments and unrelated values kept, atomic replace), and
   applies the result through the existing reload path.
2. User documentation that matches the product as it is now: platform, presets, shortcuts,
   configuration, settings panel, performance evidence, and known limits.

## Settled decisions

- D1. Base: `a03bd1b` (`origin/08-tabs-and-splits`, PR #10, stacked on #8 (07) -> #7 (06) -> #6 -> #5).
  The PR for this task targets `08-tabs-and-splits`.
- D2. Worktree: `.worktrees/11-settings-and-polish` on branch `11-settings-and-polish`. It exists.
- D3. Reordering approved by the user: task 11 runs now, in parallel with task 09, before task 10 merges.
  Task 10 (release infra, `docs/releasing.md`, `cargo xtask package --check`) is in PR #9 on
  `feat/static-crt` and is **not** in this base. Work that needs task 10 goes to the follow-up
  section F1. Read PR #9 files only with `git show origin/10-automatic-release:<path>`; do not copy them.
- D4. Task 09 (animations) is planned in parallel on a sibling branch from the same base.
  It can add config keys, presets, and effects. Expect merge conflicts in `config.rs`,
  `input.rs`, `app.rs`, `assets/config.example.toml`, `docs/configuration.md`, `README.md`.
  Keep the panel rows in one table so a new key is one new row. Rebase before the PR.
- D5. Performance ceiling for this phase: 1.40× Alacritty (`docs/decisions/05-renderer-baseline.md`).
  The 1.20× target stays the long-term and release target (task 10). This host (AMD Radeon 890M, RADV)
  is the measurement machine, but Alacritty 0.17.0 is not installed. `cargo xtask perf` cannot
  produce a valid result. Record performance as **unverified**. Never claim a pass.
- D6. One settings model: 07's `config::Config` (and `Layer`, `Overrides`, `Config::resolve`,
  `validate`). The panel holds no second settings struct. Its preview is
  `Config::resolve(Layer::parse(edited_text)?, overrides)`.
- D7. One GPU context: the panel is plain `Scene` fills and glyphs, drawn by the existing
  `SurfaceRenderer`, the same glyph cache, the same CRT pass. No new device, surface, pipeline, or toolkit.
- D8. Linux only. Do not advertise macOS or Windows. Native Wayland only (winit is built with the
  `wayland` feature only), so do not advertise X11 either.
- D9. Tools installed: `cargo xtask check` runs all steps (fmt, check, clippy, test, deny,
  actionlint, zizmor). A missing tool is a real problem, not an expected skip.
- D10. Single-pane identity (task 08 D5) stays: with the panel closed, the window `Scene` is
  unchanged. So the task 05/06 numbers and the perf capture probe stay valid.

## Code facts at the base (read before work)

- `crates/app/Cargo.toml`: platform-neutral `[dependencies]` `serde 1` (derive) and `toml 1.2`.
  Linux-only: `pollster`, `retro-renderer`, `retro-terminal`, `unicode-width`, `winit`, `wl-clipboard-rs`.
  No dev-dependencies.
- `Cargo.lock` already has `toml_edit 0.25.20+spec-1.1.0` (through `proc-macro-crate`, features
  `parse` only) and `toml_writer 1.2.0` (through `toml`). `toml_edit`'s `display` feature uses
  `toml_writer`. So `toml_edit = { version = "0.25.20", default-features = false, features = ["parse", "display"] }`
  adds no new crate version (verify with `git diff Cargo.lock`). `tempfile 3.27.0` is in the lock as an `xtask` dev-dependency.
- `crates/app/src/config.rs` (platform-neutral, `pub mod config`):
  - `Config { preset, effects: Effects { crt }, font: Font { family, size, line_height }, terminal: Terminal { scrollback_lines }, shell: Shell { program, args, working_directory } }`,
    `Serialize + Deserialize`, `deny_unknown_fields`.
  - `Layer` (all `Option`), `Layer::parse(text) -> Result<Layer, ConfigError>`.
  - `Config::resolve(user: Layer, overrides: &Overrides)`: defaults -> preset -> user -> CLI, then `validate()`.
    Ranges: `font.size` 6..=72 (`FONT_SIZE`, private), `font.line_height` `size..=2*size`,
    `terminal.scrollback_lines` 0..=100000 (`MAX_SCROLLBACK_LINES`, private), `shell.args` needs `shell.program`.
  - `PRESETS: [(&str, &str); 2]` = `static-crt`, `plain` (from `assets/presets/`).
  - `Overrides { plain: bool, program: Option<(String, Vec<String>)> }`.
  - `Changes::between(old, new) -> { renderer, layout, new_sessions }`. `reload(current, candidate)`.
  - `Source::{Default(Option<PathBuf>), Explicit(PathBuf)}`, `Source::path()`. `load(source, overrides)`:
    a missing default file = empty text; any I/O error on an explicit file is `ConfigError::Io`.
  - `ConfigError::{Io, Parse, Range, UnknownPreset, Font}` with `Display`.
- `crates/app/src/app.rs` (Linux only):
  - `State` holds `config`, `source`, `overrides`, `banner: String`, `scheduler`, `cell: (u16, u16)`, `tabs`, `panes`.
  - `State::reload()` loads the file, calls `renderer.reconfigure` when `Changes.renderer`, keeps the
    old config on error and shows it with `show()` (banner + stderr), resizes on `Changes.layout`,
    and shows "applies to new panes and tabs" on `Changes.new_sessions`.
  - `State::show(message)` sets the banner, prints to stderr, invalidates.
  - `window_event`: a key press (no preedit) clears the banner, then `input::key(..)` -> `State::action`.
    `WindowEvent::Ime` commits text to the focused pane. Left press -> `press()` (tab bar click,
    pane focus, selection). Wheel -> `scroll()`.
  - `compose()`: panes, then `ui::dividers`, then `ui::tab_bar` (only with two or more tabs). The banner
    is drawn per pane by `scene::pane` on the focused pane's last row.
  - `about_to_wait`: `ControlFlow::Wait` unless `scheduler.deadline()` is set (blink or retry).
- `crates/app/src/input.rs`: `Action { Copy, Paste, ReloadConfig, NewTab, ClosePane, NextTab, PreviousTab, Split, Focus, Resize, Bytes }`.
  Ctrl+Shift+ C V R T W E O are app shortcuts. Any other Ctrl+Shift+letter (for example P) sends the
  same control byte as Ctrl+letter.
- `crates/app/src/ui.rs` (Linux only): stateless helpers `tab_bar`, `tab_at`, `dividers`, with unit tests.
  Colors `DIVIDER`, `TAB (40,40,40)`, `TAB_TEXT (200,200,200)`.
- `crates/app/src/scene.rs`: private `overlay(scene, area, background, text)` drops glyphs on the
  same row inside `area`, adds one fill and one glyph cluster. The renderer draws all fills first and
  all glyphs after (`renderer/src/lib.rs` `GridRenderer::draw`), so an overlay must remove the glyphs below it.
- `crates/app/src/scheduler.rs`: frames only on `invalidate()`, blink, or retry. Test
  `coalesces_redraws_and_does_not_poll_nonblinking_idle` shows the idle rule (`deadline() == None`).
  `cursor_visible(now, false)` clears the blink deadline.
- `app.rs` `draw()` computes `blinking = snapshot.cursor.blinking && cursor_position.is_some() && shape != Hidden && preedit.is_empty()`
  and calls `scheduler.cursor_visible(now, blinking)`. So a blinking shell cursor keeps a 500 ms
  deadline. An open idle panel must turn this off (design below), or it is not idle.
- IME: `State::new` calls `window.set_ime_allowed(true)`. `input::Composition::update` clears the
  preedit on `Ime::Disabled`. Key presses are ignored while the preedit is not empty.
- `ConfigError::Font` comes only from `State::reload()` (`renderer.reconfigure` fails), not from
  `Config::resolve`. So a panel save can pass validation and the following reload can still fail.
- D10 guard: `scene.rs` test `single_pane_scene_equals_convert`.
- `main.rs` CLI: `retro-term [--development] [--plain] [--config PATH] [-- COMMAND [ARG...]]`.
- `toml_edit 0.25.20` features: `default = ["parse", "display"]`, `display = ["dep:toml_writer"]`;
  it needs `toml_writer 1.2.0` (in the lock) and `rust-version 1.85` (workspace is 1.94).
  The lock lists `toml_edit` without `toml_writer` today; adding `display` adds that one edge only.
- `crates/app/tests/config.rs`: 15 integration tests. Temp files use
  `std::env::temp_dir().join(format!("retro-term-{pid}-{name}.toml"))`.
- `xtask` has `doctor`, `check`, `perf`, `run`, `worktree`. There is **no** `package` command in this base.
- Stale user docs at the base:
  - `README.md`: "Status: early setup. There is no runnable terminal yet." and "within 1.20 times Alacritty" (no phase note).
  - `docs/architecture.md`: "Planned milestones ... None of these exist yet." and the 1.20 principle without the 1.40 phase limit.
  - `docs/development.md`: user shortcuts (copy, paste, tabs, splits) live in a developer section "Run the plain terminal".
  - `docs/configuration.md`: correct for 07/08; no settings panel.

## Design (final; Q1–Q13 answered with the recommended options)

### Panel behavior

- Ctrl+Shift+P opens the panel (Q2). Escape or Ctrl+Shift+P again closes it without saving.
- While open, the panel takes every key press. No key reaches the shell; app shortcuts
  (Ctrl+Shift+R, tabs, splits) do nothing until the panel closes.
- IME: on open call `window.set_ime_allowed(false)` (winit sends `Ime::Disabled`, the preedit clears,
  so panel keys are not blocked by the preedit guard). On close call `set_ime_allowed(true)`.
  Any `Ime::Commit` that still arrives while open is dropped.
- Mouse: left press and drag are ignored while open (`CursorMoved` still updates `pointer`).
  The wheel still scrolls the pane under the pointer.
- Idle: while open, `draw()` passes `blinking = false` to `scheduler.cursor_visible`
  (add `&& self.settings.is_none()` to the existing `blinking` expression). So an open panel
  with no input and no shell output has no deadline and requests no frames.
- Resize while open: after `resize()`, if `ui::settings_box` returns `None` (box no longer fits),
  close the panel without saving and show the banner "window too small for settings; closed without saving".
- Rows (Q3), each with the effective value from the preview `Config`:

  | Row | Key | Left / Right |
  | --- | --- | --- |
  | Preset | `preset` | previous / next name in `PRESETS` (wraps) |
  | CRT effects | `effects.crt` | toggle |
  | Font size | `font.size` | -1 / +1, stops at `max(6, ceil(line_height/2))` and `min(72, line_height)` (Q13) |
  | Line height | `font.line_height` | -1 / +1, stops at `size` and `2*size` of the preview |
  | Scrollback (new shells) | `terminal.scrollback_lines` | -1000 / +1000, clamps to 0 and 100000 |

  Read-only lines: `font.family` (or "system monospace"), the file path, and "Edit the file for shell settings".
- Stepper bounds come from the same limits as `Config::validate` (`FONT_SIZE`, `MAX_SCROLLBACK_LINES`,
  `size..=2*size`). With the Q13 default, a stepper can never produce an invalid file, so the
  preview is always a valid `Config` and the rows always show `preview()` values. When a stepper stops
  at a bound set by another row, the status line says which row to change first
  (for example "font.size cannot exceed font.line_height (19); raise line height first").
- Defensive path: if `Config::resolve` of the edited text still fails, keep the last valid preview,
  show the `ConfigError` text on the status line, and block Enter until a later change resolves.
- Up/Down move the selection (no wrap). Left/Right change the selected row. Enter saves. Escape cancels.
- `--plain`: the CRT row shows the preview value (off) plus "(--plain; file: on|off)". The file value is
  `Config::resolve(layer, &Overrides { plain: false, ..overrides.clone() })`. A toggle still writes the
  file (it applies after a restart without `--plain`). Same `Config` type; no second model.
- A key that changes nothing (Left at the minimum, Up on the first row) returns `Outcome::Nothing`
  and does not invalidate the scheduler. The panel has no timer and no blinking cursor.
- Enter with no edits (text equals the original) returns `Outcome::Close`; nothing is written and no reload runs.
- Status line: the last stepper hint or validation error, or a write error.
- Save: build the new text (below), check it with `Layer::parse` + `Config::resolve`, write it
  (below), close the panel, then call the existing `State::reload()`. Reload owns every runtime
  change (renderer, resize, "new panes and tabs" message). On a save error the panel stays open with
  the edits and shows the error; the file and the running state do not change.
- Reload after save can still fail (for example `ConfigError::Font` from `renderer.reconfigure`).
  Then the file is already saved, `reload()` keeps the old config and shows its error in the banner,
  the same as Ctrl+Shift+R on that file. Document this.
- Live preview of the renderer: none (Q5). Changes show in the panel rows only until Enter.

### Editing the file text

- At open: `Settings::open(source_path, original: Option<String>, overrides)`. `original` is the file
  text, or `None` when the file does not exist. Parse it into `toml_edit::DocumentMut`.
  If the text does not parse or does not resolve, the panel opens read-only with the error and
  "fix the file, then reload with Ctrl+Shift+R"; Enter does nothing (Q8).
  If there is no path (`Source::Default(None)`, no `$HOME`), the panel opens read-only with "no configuration path".
- Each change sets one dotted key in the `DocumentMut`:
  - An existing value keeps its decor (leading space and trailing `# comment` on the same line):
    copy `value.decor().clone()`, replace the value, restore the decor.
  - A missing table is created as a normal `[table]` at the end; a missing key is appended to its table.
  - Works for normal tables, inline tables (`font = { size = 14 }`), and dotted keys (`font.size = 14`).
  - Never remove keys. Never reorder. Never touch keys that the user did not change in this panel session.
- The preview is `Config::resolve(Layer::parse(&doc.to_string())?, overrides)`. So the panel always shows
  the result of the real precedence rules: selecting `preset = "plain"` while the file has
  `effects.crt = true` still shows CRT on, the same as a later load.
- A new file (no original) gets only the changed keys. No header, no full example copy (Q9).

### Writing the file (Q6, Q7)

`config::save(path: &Path, expected: Option<&str>, text: &str) -> Result<(), SaveError>`:

1. Resolve the path: if `fs::symlink_metadata(path)` says symlink, write to `fs::canonicalize(path)`
   (dotfile managers use symlinks). A dangling symlink makes `canonicalize` fail: return `SaveError::Io`.
2. Read the current file. `NotFound` means "no file". Any other read error returns `SaveError::Io`
   (for example the target is a directory, or a parent is a regular file). If the text is not
   `expected` (or a file exists when `expected` is `None`, or no file exists when `expected` is `Some`),
   return `SaveError::Changed`. Do not write.
3. `fs::create_dir_all(parent)`.
4. Create `<parent>/.<file name>.<pid>.tmp` with `OpenOptions::new().write(true).create_new(true)`.
   Write the text, copy the permissions of the old file (if any), `sync_all()`.
5. `fs::rename(temp, target)`. Then open the parent directory and `sync_all()` it (best effort; ignore its error).
6. On any error after step 4: remove the temp file, return `SaveError::Io`. The old file is untouched.

`SaveError { Changed, Io(io::Error) }` with `Display` ("the file changed on disk; close the panel,
press Ctrl+Shift+R, and open it again" / the I/O error). The "no path" case never reaches `save`:
the panel opens read-only (below). A small race remains between step 2 and step 5; it is accepted
and documented (no file locking). A stale temp file with the same pid makes `create_new` fail with
`AlreadyExists` -> `SaveError::Io`; accepted.

### Code layout

- `crates/app/src/settings.rs` (new, platform-neutral, `pub mod settings`): `Settings` (document, original
  text, path, selected row, status), `Row` table, `Key { Up, Down, Left, Right, Enter, Escape }`,
  `Outcome { Nothing, Redraw, Close, Save(String) }`, `Settings::key(Key) -> Outcome`,
  `Settings::open(path: Option<PathBuf>, original: Option<String>, overrides: Overrides) -> Settings`,
  `Settings::original() -> Option<&str>` (the `expected` for `config::save`), `Settings::path() -> Option<&Path>`,
  `Settings::preview() -> &Config`, `Settings::status() -> &str`,
  `Settings::lines() -> Vec<(String, bool)>` (text, selected) for drawing.
  No winit, no renderer types, so it is tested headless in `crates/app/tests/settings.rs`.
  `Settings::set_status(String)` lets the app show a save error.
- `crates/app/src/config.rs`: `save`, `SaveError`; make `FONT_SIZE` and `MAX_SCROLLBACK_LINES` `pub`
  (the panel steppers use the same limits). No change to `Config`.
- `crates/app/src/ui.rs`: `settings_box(window: Rect, lines: &[(String, bool)], cell: [u32; 2]) -> Option<Rect>`
  (centered, whole cells, one cell of padding, width = longest line; `None` if it does not fit) and
  `settings_panel(scene, window, lines, cell)` — uses `settings_box`, drops every glyph whose bounds
  intersect the box (x **and** y, unlike `scene::overlay` which checks one row), one background fill,
  one glyph cluster per line, selected line inverted (same colors as the tab bar). `window` is the whole
  surface (`renderer.size()`), so the panel also covers the tab bar. Fills are drawn in push order and all
  glyphs after all fills (`GridRenderer::draw`), so pushing the panel last puts it on top.
- `crates/app/src/input.rs`: `Action::Settings` for Ctrl+Shift+P (one more `eq_ignore_ascii_case("p")` arm);
  `settings_key(key: Key<&str>, modifiers) -> Option<settings::Key>`: arrows, Enter, Escape, and
  Ctrl+Shift+P -> `Escape`; everything else `None`.
- `crates/app/src/app.rs`: `State.settings: Option<Settings>`.
  - Open (`Action::Settings`): read the file (`fs::read_to_string`; `NotFound` -> `None`; other error ->
    read-only panel with the error), `Settings::open(self.source.path().map(Into::into), text, self.overrides.clone())`.
    If `ui::settings_box` is `None`, do not open; `show("window too small for settings")` (Q4).
    Else `set_ime_allowed(false)`, invalidate.
  - Key routing: at the top of the pressed-key branch (after the banner clear), if the panel is open,
    map with `settings_key`, call `Settings::key`, handle the `Outcome`, and return `Ok(false)` for every key.
  - `Outcome::Nothing` -> nothing. `Redraw` -> invalidate. `Close` -> close (IME on, invalidate).
    `Save(text)` -> `config::save(path, settings.original(), &text)`; `Ok` -> close, then `self.reload()?`;
    `Err(e)` -> `settings.set_status(e.to_string())`, `eprintln!`, invalidate, panel stays open.
  - Guards: `Ime::Commit` dropped while open; `press()` and `drag()` return early while open.
  - `draw()`: `blinking` also needs `self.settings.is_none()`.
  - `resize()`: close the panel if it no longer fits (see Panel behavior).
  - `compose()` calls `ui::settings_panel` last, only when open; the banner is not drawn while open
    (pass `""` instead of `&self.banner`).

## Steps

Execute in order after the user answers Q1–Q13. Run `cargo xtask check -p retro-app` after each app step (workspace-wide steps are skipped with `-p`; the full `cargo xtask check` runs at the end).

1. **Dependency.** Add `toml_edit = { version = "0.25.20", default-features = false, features = ["parse", "display"] }`
   to the platform-neutral `[dependencies]` of `crates/app/Cargo.toml` (per Q6). Run `cargo check --locked -p retro-app`,
   then `git diff Cargo.lock`: only `retro-app`'s and `toml_edit`'s dependency lists may change; no new crate versions.
   Run `cargo xtask check --step deny`.
2. **File writing.** In `config.rs` add `save` and `SaveError` (design above). Make the two range constants `pub`.
   Add the save tests (list below) to `crates/app/tests/config.rs`.
3. **Settings model.** Add `settings.rs` and `pub mod settings;` in `lib.rs` (no `cfg`). Add
   `crates/app/tests/settings.rs` (list below).
4. **Drawing.** Add `ui::settings_panel` with unit tests in `ui.rs`.
5. **Input.** Add `Action::Settings` and `settings_key` with unit tests in `input.rs`.
6. **App integration.** In `app.rs` (see Code layout): `settings` field, open/close with the IME switch,
   key routing, IME commit/press/drag guards, blink gate in `draw()`, close-on-resize, compose, save -> `reload()`.
   Errors go to the panel status line and stderr; the banner is not drawn while the panel is open.
   Run the full `cargo xtask check` after this step.
7. **User docs** (per Q10, Q11):
   - `README.md`: replace the stale status. What it is, supported platform (Linux, native Wayland, x86_64;
     no X11, macOS, or Windows), build from source, first run, presets, links. Performance: the 1.20×
     target, the 1.40× phase limit, the accepted plain-renderer evidence (`docs/decisions/05-renderer-baseline.md`),
     and that static CRT, configuration, tabs, and settings are **unverified** on the gate. Keep the contributor "First steps".
   - `docs/usage.md` (new): keyboard shortcuts (all, from `input.rs`), mouse, tabs and splits, settings panel,
     clipboard requirements, known limits (list in Q11).
   - `docs/configuration.md`: a "Settings panel" section (keys it edits, save rules, external change,
     symlink, read-only cases, `--plain` note). Add Ctrl+Shift+P next to the reload shortcut.
   - `docs/development.md`: replace the user shortcut table with a link to `docs/usage.md`; keep developer content.
   - `docs/architecture.md`: replace "None of these exist yet." with the current state; add the 1.40× phase limit.
   - `assets/config.example.toml`: add "Ctrl+Shift+P opens the settings panel." to the header comment
     (the test `example_file_equals_defaults` must still pass).
8. **Index and handoff.** Update this plan's status and the task 11 row in `docs/AI/README.md`
   (note the approved reorder, D3). Fill in the handoff from evidence only.

## Tests to add

All headless; they run in `cargo xtask check`. Use the existing `temp_path` pattern; for directories use
`std::env::temp_dir().join(format!("retro-term-{pid}-{name}"))` and remove them at the end.

`crates/app/tests/config.rs` (persistence and write failure):

- `save_creates_a_missing_file_and_its_directory`: `expected = None`, nested missing dir -> file has the text.
- `save_replaces_the_file_and_leaves_no_temp_file`: directory listing after save has only `config.toml`.
- `save_keeps_file_permissions` (`#[cfg(unix)]`): mode 0o600 before -> 0o600 after.
- `save_writes_through_a_symlink`: `config.toml` -> `real/config.toml`; after save the link is still a link and the target has the new text.
- `save_refuses_an_externally_changed_file`: `expected` differs from disk -> `SaveError::Changed`, file unchanged.
- `save_refuses_a_file_created_since_open`: `expected = None`, file exists -> `Changed`.
- `save_refuses_a_file_deleted_since_open`: `expected = Some(..)`, no file -> `Changed`, still no file.
- `save_failure_keeps_the_old_file`: (1) target path is an existing directory -> `SaveError::Io`
  (read fails with `EISDIR`), directory untouched, no temp file in the parent. (2) the parent path is a
  regular file -> `SaveError::Io` (`ENOTDIR`), that file unchanged. (3) dangling symlink -> `SaveError::Io`.
  These do not depend on file permissions, so they also pass as root. A failure after the temp file
  exists (rename or `sync_all`) is not reachable without permissions or fault injection; the code path
  (remove temp, return `Io`) is covered by review only. Say so in the handoff.

`crates/app/tests/settings.rs` (model, invalid values, reload interaction, idle no-op):

- `edit_keeps_comments_and_unrelated_keys`: input with a header comment, a comment above `[font]`,
  a trailing `# comment` on `size`, `[shell] program = "zsh"`, a blank line; change `font.size`.
  The output equals the input with only the number changed (exact string compare).
- `edit_supports_inline_tables_and_dotted_keys`: `font = { size = 14 }` and `font.size = 14` both change in place.
- `edit_appends_missing_keys_and_tables`: empty file + crt toggle -> `[effects]\ncrt = false\n`.
- `enter_without_edits_closes_without_saving`: open, Up/Down only, Enter -> `Outcome::Close`.
- `toggle_twice_saves_the_original_text`: CRT toggled twice, Enter -> `Close` (text equals original).
- `steppers_stop_at_the_config_limits`: file `font.size = 72`, `line_height = 72`; size Right -> `Nothing`.
  `scrollback_lines = 0`, Left -> `Nothing`; `= 99500`, Right -> text has `100000`; `= 500`, Left -> `0`.
- `steppers_never_produce_an_invalid_file` (invalid values): defaults (size 14, line height 19);
  press size Right 5 times -> 19; the 6th -> `Nothing`, status names `font.line_height`.
  Line height Left at 14 -> `Nothing`. Size Left at 10 (line height 19, `ceil(19/2) = 10`) -> `Nothing`.
  After every key, `Config::resolve(Layer::parse(text))` of the current text is `Ok`.
- `invalid_file_opens_read_only` (invalid values): three inputs — bad syntax, unknown key, `font.size = 100`.
  Status shows the `ConfigError` text (line/column or key and range); every key except Escape and
  Ctrl+Shift+P-as-Escape returns `Nothing`; Escape -> `Close`.
- `missing_path_opens_read_only`: `Settings::open(None, None, ..)` -> Enter -> `Nothing`; status names the reason.
- `preview_uses_the_real_precedence`: file `effects.crt = true`, select preset `plain` -> preview `crt == true`.
- `cli_override_is_shown_and_file_still_changes`: `Overrides { plain: true }`, toggle CRT -> text has the
  new `crt` value, preview `crt == false`, row marked overridden.
- `saved_text_loads_to_the_preview`: write `Outcome::Save` text with `config::save`, then
  `config::load(&Source::Explicit(path), &overrides)` equals `preview()` — the reload after save applies exactly what the panel showed.
- `saved_text_after_external_change_is_refused` (reload interaction): open with text A, write B to the
  file, `config::save(path, Some(A), panel_text)` -> `Changed`; file still B; `config::load` gives B's config.
- `no_op_keys_do_not_redraw` (idle no-op): Up on row 0, Down on the last row, Left at a minimum,
  any key in a read-only panel -> `Nothing`; a real change -> `Redraw`.

Unit tests in existing modules:

- `ui.rs` `settings_panel_covers_glyphs_and_fits_cells`: box is in whole cells, centered, glyphs that were inside the box
  (also on rows other than the first) are gone, glyphs outside stay, selected line inverted;
  too-small window -> `settings_box` is `None`, no fills, no glyphs added.
- `input.rs`: Ctrl+Shift+P -> `Action::Settings`; Ctrl+P still sends `0x10`; `settings_key` maps arrows/Enter/Escape,
  maps Ctrl+Shift+P to `Escape`, and ignores letters.
- `scheduler.rs`: no change; `coalesces_redraws_and_does_not_poll_nonblinking_idle` already proves
  `cursor_visible(now, false)` leaves `deadline() == None`, which is what an open panel passes.
- `scene.rs`: `single_pane_scene_equals_convert` must stay green unchanged (D10).

Not automatable here: the `app.rs` wiring (open, routing, IME switch, blink gate, close on resize)
runs only in a live Wayland window. Cover it in the smoke check and report it as not run by an agent.

## Acceptance criteria

- The panel edits only the rows in Q3, uses `config::Config` for every value it shows, and adds no GPU
  resource, toolkit, or second settings type.
- Saving keeps comments, key order, formatting, and unrelated keys; writes atomically; refuses an
  externally changed file; leaves the old file and the running state unchanged on any failure.
- After a save, the running window equals a Ctrl+Shift+R reload of the same file.
- With the panel closed, the scene and frame scheduling are unchanged (D10; `single_pane_scene_equals_convert` green).
  An open, idle panel requests no frames, also when the shell cursor blinks (smoke check).
- The panel never writes a file that fails `Config::resolve` (Q13) and never writes when nothing changed.
- No key, IME commit, or mouse press reaches the shell while the panel is open.
- User docs describe only Linux on native Wayland, the two presets, all shortcuts, the configuration and panel,
  the measured evidence and what is unverified, and the known limits. Nothing claims a performance pass.
- `cargo xtask check` passes with every step.
- Performance: **unverified on this host** (D5). Not a pass.

## Verification

Run in the worktree:

```sh
cargo xtask check
cargo build --locked --release -p retro-app --bin retro-term
cargo xtask run
cargo xtask perf --candidate target/release/retro-term --baseline alacritty --suite benchmarks/scenarios.toml --output target/perf/preflight.json --preflight
```

- No renderer code changes in this task, so the GPU test (`cargo test --locked -p retro-renderer --test gpu_render -- --ignored`)
  is optional. Run it if the renderer diff is not empty.
- `cargo xtask run` needs a native Wayland session. An agent can only start it with a timeout and check
  `target/xtask/logs/run.log` for the adapter and "frame presented". Smoke check (a person): open the panel;
  change font size and CRT; Enter; check the window and `stty size` change; reopen; edit the file in another
  editor; save in the panel -> "changed on disk" error; Escape; Ctrl+Shift+R; reopen and save -> works;
  check comments in the file stay; with a blinking shell cursor (`printf '\e[1 q'`) and the panel open and idle,
  the `frame presented` lines stop in `run.log`; resize the window very small -> panel closes with the banner;
  start an IME composition, press Ctrl+Shift+P -> the preedit clears and arrows work.
- The perf preflight is expected to fail because Alacritty 0.17.0 is not installed (exit 3 = missing tool
  in `xtask/src/main.rs`). Record the actual exit code and `target/perf/preflight.json` (status `unverified`). Never report a pass.
- `cargo xtask package --check` does **not** exist in this base. Do not run or cite it here; see F1.

## Follow-up F1 (after PR #9 merges; not part of this task's PR)

- Rebase on the merged release work. Run `cargo xtask package --check`.
- README: add the install-from-archive section and link `docs/releasing.md`. Do not copy its content.
- Add `docs/usage.md` (and the panel) to the archive file list if the user wants it shipped.
- Recheck the packaged binary on the resource suite before any release (`benchmarks/release-gate.toml`, 1.20×).
  On this host the result stays unverified until Alacritty 0.17.0 is installed.

## Follow-up F2 (after rebasing on task 09)

- Add the "Animation" toggle row for `effects.animation`: one entry in `settings::ROWS`
  (a `Kind::Toggle` with the key path and a getter on `Config`), then a test like
  `cli_override_is_shown_and_file_still_changes` if `--plain` also overrides it.
- Mention the row in `docs/configuration.md` ("Settings panel") and `docs/usage.md`.

## Out of scope

Live preview, mouse control of the panel, editing `font.family` or `shell.*` in the panel, a file watcher,
key-binding configuration, color/palette configuration, user preset files, new perf scenarios,
release docs and packaging (F1), macOS/Windows/X11 ports.

## Decisions on Q1–Q13 (user, final)

The user accepted the recommended option (a), marked **rec**, for every question:
Q1 in-scene panel; Q2 Ctrl+Shift+P; Q3 preset, CRT, font size, line height, scrollback;
Q4 do not open in a too-small window; Q5 apply on Enter through `reload()`; Q6 `toml_edit 0.25.20`
(`parse` + `display`); Q7 std-only atomic write that follows symlinks; Q8 refuse on external change,
invalid file opens read-only; Q9 a missing file gets only the changed keys; Q10 README + `docs/usage.md`
+ panel section in `docs/configuration.md`; Q11 the full known-limits list; Q12 no new perf scenario;
Q13 steppers stop before an invalid combination. The options below stay for traceability.

Extra decision (user): an "animation" toggle row is wanted, but `effects.animation` comes from task 09
(sibling branch, not in this base). Do not add the key here. The rows are one data table in
`settings.rs` (`ROWS`), so the row is a one-line addition. See follow-up F2.

- Q1. UI approach.
  (a) **rec** In-scene, cell-based panel built from `Scene` fills and glyphs in `ui.rs`, like the tab bar. No new crate, no new GPU resources, zero cost when closed.
  (b) `egui` + `egui-wgpu` + `egui-winit` overlay on the same device. Needs a version that matches `wgpu 29` and `winit 0.30` (not checked); large dependency tree; extra RAM and a second render pass; text would not use the terminal font or the CRT pass consistently.
  (c) `iced`: owns its own event loop/renderer; does not fit one `SurfaceRenderer`. Not recommended.
- Q2. Open shortcut. (a) **rec** Ctrl+Shift+P (free; today it only duplicates Ctrl+P). (b) Ctrl+Shift+, (layout-dependent: Shift+, is `<` on US). (c) Ctrl+Shift+F2. Escape always closes.
- Q3. Editable settings. (a) **rec** `preset`, `effects.crt`, `font.size`, `font.line_height`, `terminal.scrollback_lines` (steps of 1000). (b) (a) without scrollback. (c) (a) plus `font.family` as a text field (needs text input, IME, and a font check; much larger).
- Q4. Too-small window. (a) **rec** Do not open; show the banner "window too small for settings". (b) Clip the panel.
- Q5. When changes apply. (a) **rec** On Enter only, through the existing `reload()`. (b) Live preview on each change (renderer reconfigure per key press; more GPU work and a revert path on Escape).
- Q6. TOML writer. (a) **rec** `toml_edit 0.25.20`, `default-features = false`, features `parse` + `display`; already in the lockfile, no new versions. (b) Hand-written line edits (fragile with inline tables and dotted keys). (c) `toml::to_string(&Config)` (loses comments; violates the architecture rule).
- Q7. Atomic write. (a) **rec** std only: temp file `create_new` in the same directory, copy permissions, `sync_all`, `rename`, best-effort directory `fsync`, remove the temp file on error; follow symlinks to the target. (b) `tempfile::NamedTempFile::persist` (new runtime dependency for `retro-app`; same version as `xtask`). (c) As (a) but replace a symlink with a regular file.
- Q8. External change and invalid file. (a) **rec** Compare the whole file text read at open with the text on disk at save; on a difference refuse and ask to close, reload, and reopen. An invalid file opens the panel read-only. (b) Compare mtime and size only. (c) Overwrite without a check.
- Q9. Missing file. (a) **rec** Create it (and its directory) with only the changed keys. (b) Create it from `assets/config.example.toml` with the changes applied (all comments, every key).
- Q10. Documentation layout. (a) **rec** User-facing `README.md` (status, platform, presets, perf statement, links) + new `docs/usage.md` (shortcuts, mouse, panel, known limits) + panel section in `docs/configuration.md`; `docs/development.md` keeps only developer content. (b) Put shortcuts and limits in `README.md`, no new file. (c) Keep shortcuts in `docs/development.md`.
- Q11. Known-limits list. (a) **rec** Linux + native Wayland only; x86_64 only tested; clipboard needs `ext-data-control`/`wlr-data-control`; no key-binding or color configuration; no file watcher (reload by shortcut); scrollback/shell settings apply to new panes; Ctrl+Shift+W closes without confirmation; performance on the gate unverified for static CRT, configuration, tabs, and settings; IME and live smoke checks still pending. (b) A shorter list without the unverified items (not recommended: hides evidence state).
- Q12. Performance work in this task. (a) **rec** No new perf scenario; the panel is closed in every scenario and D10 keeps the scene unchanged; run the preflight and record "unverified". (b) Add a `settings-idle` scenario (panel open, no input, expect no frames) to the planned multi-session harness list in `docs/performance.md` only.

- Q13. Dependent stepper bounds. (a) **rec** Each stepper stops where the result would fail `validate()`
  (size cannot pass line height; line height stays in `size..=2*size`); the status line says which row to change first.
  The panel never writes an invalid file and touches only the row the user changed. (b) Free steppers within the
  fixed ranges; an invalid combination shows the `ConfigError` and blocks Enter (more states to show and test).
  (c) Changing size also moves line height to stay valid (writes a key the user did not touch).

## Contradictions found while planning

- Ceiling: the old version of this plan, `README.md`, `docs/architecture.md`, and plan 09 say 1.20×;
  `docs/decisions/05-renderer-baseline.md`, `docs/performance.md`, and plans 07/08 use 1.40× for this phase.
  The user set 1.40× for this task (D5); 1.20× stays the release target.
- Order: `docs/AI/README.md` and the old plan say task 11 depends on "10 completed" and the old
  verification lists `cargo xtask package --check`. The user approved running 11 before 10 merges (D3);
  that command does not exist in this base (F1).
- The architecture says "Consider `egui` only when a settings editor is implemented". This plan recommends
  no `egui` (Q1) because of dependency and resource cost; the user decides.
- `docs/AI/README.md` calls task 11 "after the initial release pipeline"; it now runs before it (D3).
- `README.md` says "There is no runnable terminal yet"; tasks 04–08 have a runnable terminal. Step 7 fixes it.
- `docs/architecture.md` says "None of these [milestones] exist yet"; 03–08 exist locally. Step 7 fixes it.
- User shortcuts are documented in `docs/development.md` (a developer doc) only. Q10 moves them.
- The earlier draft said an open idle panel requests no frames, but `draw()` keeps a 500 ms blink
  deadline when the shell cursor blinks. The design now passes `blinking = false` while the panel is open.
- The earlier draft cleared the preedit by hand; the key guard ignores keys while a preedit exists.
  The design now disables IME while the panel is open (`Ime::Disabled` clears the preedit).
- The earlier draft listed `SaveError::NoPath`, but `save` takes a `&Path`; "no path" is a read-only panel instead.
- The earlier draft's failure test said "rename fails" for a directory target; the read in step 2 fails first (`EISDIR`).
  A failure after the temp file exists is not testable without permissions or fault injection.
- Plan 09 on `09-optional-animation` still has the old text (1.20× for every preset, no reorder note);
  it is being replanned in parallel. Recheck after it lands.
- Architecture "Text configuration" asks for comment preservation and atomic replace if a GUI writes the file;
  this plan follows it (Q6, Q7). No contradiction, recorded for traceability.

## Progress

- Planning done. Q1–Q13 answered (recommended options); animation row deferred to F2.
- Steps 1–8 done in the worktree. Nothing staged or committed.

## Handoff

- Base: `a03bd1b` on `08-tabs-and-splits`. Worktree `.worktrees/11-settings-and-polish`, branch `11-settings-and-polish`.
- Changed files, by step:
  1. `crates/app/Cargo.toml`, `Cargo.lock` (two dependency edges only: `retro-app -> toml_edit`, `toml_edit -> toml_writer`; no new crate versions).
  2. `crates/app/src/config.rs` (`save`, `SaveError`, `FONT_SIZE` and `MAX_SCROLLBACK_LINES` now `pub`; appended at the end of the file), `crates/app/tests/config.rs` (8 save tests).
  3. `crates/app/src/settings.rs` (new; rows in one `ROWS` table), `crates/app/src/lib.rs`, `crates/app/tests/settings.rs` (14 tests).
  4. `crates/app/src/ui.rs` (`settings_box`, `settings_panel`, unit test).
  5. `crates/app/src/input.rs` (`Action::Settings`, `settings_key`, unit test).
  6. `crates/app/src/app.rs` (panel state, open/close with IME switch, key routing, IME commit/press/drag guards, blink gate, close on resize, compose, save -> `reload()`).
  7. `README.md`, `docs/usage.md` (new), `docs/configuration.md`, `docs/development.md`, `docs/architecture.md`, `assets/config.example.toml`.
  8. This plan, `docs/AI/README.md`.
- Deviations:
  - `ui::settings_box` truncates lines wider than the window (long file path or status) instead of refusing to open;
    it returns `None` when fewer than 40 inner columns or not enough rows fit. Q4 still applies to that case.
  - A stepper that stops at a bound set by another row returns `Redraw` the first time (the status line changes and must be
    drawn) and `Nothing` on a repeat press. The plan said `Nothing` with a status; that would not show the status.
  - Defensive path: an edit whose text fails `Config::resolve` is not applied (the status shows the error) instead of
    keeping an invalid document and blocking Enter. Unreachable with the Q13 steppers.
  - `Settings::unreadable(path, error, overrides)` added for a file that exists but cannot be read (read-only panel).
  - Read-only panels hide the editable rows; they show the reason, the font family, and the path.
  - A new file (no original) where a value is toggled and toggled back is saved with that explicit key, because the text differs from the empty original.
  - The open panel reuses the tab bar colors and also covers the tab bar.
- Commands run and results:
  - `cargo xtask check`: fmt, check, clippy, test, deny, actionlint, zizmor passed; `ignored-tests` not_run (1 display test).
  - `cargo build --locked --release -p retro-app --bin retro-term`: passed.
  - `cargo test --locked -p retro-renderer --test gpu_render -- --ignored`: 1 passed (hardware AMD Radeon 890M; no renderer change).
  - `timeout 15 cargo xtask run`: exit 124 (timeout, expected). `run.log`: adapter `AMD Radeon 890M Graphics (RADV STRIX1)`, Vulkan, Mesa 26.2.7; 6 `frame presented` lines; no error.
  - `cargo xtask perf ... --preflight`: exit 3; `target/perf/preflight.json` status `unverified` (Alacritty 0.17.0 not installed). Not a pass.
- Not run, and why:
  - Live smoke check of the panel (open, edit, save, external change, idle frames with a blinking cursor, resize close, IME): needs a person in the Wayland session.
  - A save failure after the temp file exists (rename or `sync_all`): needs permissions or fault injection; covered by review only.
  - `cargo xtask package --check`: not in this base (F1).
- Follow-ups: F1 (release docs after PR #9), F2 (animation toggle row after rebasing on 09).
- Open questions: none.
