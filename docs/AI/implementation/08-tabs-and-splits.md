# Native tabs and splits

Status: implemented locally. `cargo xtask check` passes. Live smoke check (needs a person) and performance unverified.
Dependencies: 07 implemented locally (PR #8, `07-text-configuration`). 04–06 are in the same stack.

## Context and shared rules

Read [the architecture](../architecture/retro-terminal.md) (sections "Ownership and data flow" and
"Rendering and resource policy"), [the execution index](../README.md), [the AI workflow](../../ai-workflow.md),
and [task 07](07-text-configuration.md) (the config model this task builds on).

Linux first. Reuse `alacritty_terminal`. Three product crates plus development-only `xtask`.
Keep the crate boundaries in `CLAUDE.md`: tabs, split tree, focus, and input routing live in
`retro-app`. The terminal crate already gives one `Session` per PTY. The renderer gets one
`Scene` per frame and must not learn about panes. `xtask` does not depend on product crates.
Make surgical changes. Do not commit, push, or open a PR unless the user asks.
Use project tooling only. No ad-hoc scripts, no extra servers. Report checks that did not run.

## Goal

Tabs and nested horizontal/vertical splits in one window, each pane with its own shell, all
sharing one GPU device, one surface, one glyph cache, and one CRT pass. Hidden tabs keep
parsing output but do no text or GPU work. One tab with one pane must render exactly as the base.

## Settled decisions

- D1. Base: `843716f0d7bdcfbbfb4e42eedddbb06ab56f4988` (`origin/07-text-configuration`, PR #8,
  stacked on #7 -> #6 -> #5). The PR for this task targets `07-text-configuration`.
- D2. Worktree: `.worktrees/08-tabs-and-splits` on branch `08-tabs-and-splits`. It exists.
- D3. Performance ceiling for this phase: 1.40× Alacritty (`docs/decisions/05-renderer-baseline.md`).
  The 1.20× target applies to the release gate (task 10).
- D4. This host is the measurement machine (AMD Radeon 890M, RADV), but Alacritty 0.17.0 is not
  installed. `cargo xtask perf` cannot produce a valid result here. Record performance as
  **unverified**. Never claim a pass.
- D5. Single-pane identity: with one tab and one pane, the window `Scene` must equal the base
  `scene::convert` output (no tab bar, no divider, same clear color). This keeps the task 05/06
  numbers and the perf capture probe (`xtask/src/perf/wayland.py`) valid without changes.
- D6. Tools installed: cargo-dist, cargo-deny, actionlint, zizmor. `cargo xtask check` runs all
  steps; a missing-tool skip is now a real problem, not an expected result.
- D7. Task 11 (settings UI) comes later. It reuses 07's `Config` and this task's `ui.rs`. Keep
  `ui.rs` a small set of scene-building helpers (tab bar, dividers, banner) with no state of
  its own, so task 11 can add controls next to them. Do not build any settings UI here.
- D8. No new crate dependencies. No daemon, no session persistence, no external multiplexer.
- D9. Architecture IDs: `TabId` and `PaneId` are new app types; `SessionId` already exists in
  `retro-terminal`. A window owns tabs; each tab owns a split tree; each leaf is one pane with
  one `Session`. A split stores an axis, a ratio, and two children.

## Code facts at the base (read before work)

- `crates/app/src/lib.rs`: `pub mod config;` has no `cfg` gate (headless tests in
  `crates/app/tests/config.rs`). `app`, `input`, `platform`, `scene`, `scheduler` are
  `#[cfg(target_os = "linux")]` and private. `crates/app/tests/` has only `config.rs`.
- `crates/app/src/app.rs`:
  - `enum Event { Terminal(SessionId), Paste(..) }`. The wake callback in `State::new` sends
    `Event::Terminal(id)`. `App::user_event` drops `Event::Terminal` for any id other than
    `state.session.id()` (single-session assumption).
  - `State` holds exactly one `session: Session`, one `grid: GridSize`, `selection_start`,
    `scroll_fraction`, `composition`, `banner`, `previous_ime_cursor`, `font_size`, `config`,
    `source`, `overrides`, `renderer: SurfaceRenderer`, `scheduler: Scheduler`.
  - `State::resize(size)`: `scene::font_metrics(..)`, cell-width cache keyed on
    `font_size`/`grid.cell_height`, `scene::grid_size(size, ..)`, `scheduler.set_presentable(..)`,
    `renderer.resize(size)`, then `session.resize(grid)` (grid and PTY together).
  - `State::reload` uses `config::Changes::between`; `changes.layout` forces `font_size = 0`
    and calls `resize`; `changes.new_sessions` shows "configuration reloaded; shell and
    scrollback settings apply to new shells".
  - `State::draw`: one `session.snapshot()`, IME cursor area from `mode.display_point`,
    `scene::convert(&snapshot, font_size, visible, preedit, banner)`, `renderer.draw(..)`.
  - `State::poll_terminal` sets the window title from `TitleChange` and returns
    `exit_status.is_some()`; `App::user_event` closes the app on `Ok(true)`.
  - `App::close` closes the single session (`Session::close`, may block).
  - `fn program(config)` builds the `Program` from `config.shell`.
- `crates/app/src/config.rs`: `Config::resolve` copies `Overrides.program` (the command after `--`)
  into `config.shell.program/args`. So today any new session would run the `--` program again (Q7).
  `Changes { renderer, layout, new_sessions }`. No layout/keys settings exist (07 Q11: key
  bindings are not configurable).
- `crates/app/src/scene.rs`: `convert` builds a `Scene` whose `clear` is `snapshot.background`
  and whose rects start at (0, 0). `bounds(position, grid)` has no origin. `overlay(..)` draws
  one fill + one glyph cluster (used by preedit and the banner on the last grid row).
  `pointer_cell(point, viewport, mode, grid, clamp)` maps through `mode.source_point` then
  `cell_at`. Tests: `physical_grid_and_selection_use_the_same_cells`,
  `distorted_selection_maps_edges_and_preserves_padding`,
  `wide_wrap_spacer_and_combining_mark_keep_fixed_positions`, `banner_covers_the_last_row_within_the_grid`.
- `crates/app/src/input.rs`: `Action { Copy, Paste, ReloadConfig, Bytes(Vec<u8>) }`. App
  shortcuts are only Ctrl+Shift+C/V/R, checked first. Every other Ctrl+Shift+letter sends the
  same control byte as Ctrl+letter. `special(named)` encodes modified arrows/PageUp/PageDown
  as `CSI 1;<mod>X` / `CSI <n>;<mod>~`. Ctrl+Tab sends `\t`, Shift+Tab sends `CSI Z`.
- `crates/app/src/scheduler.rs`: one `dirty` flag, cursor blink deadline (`BLINK` 500 ms),
  retry deadline. No per-pane state is needed.
- `crates/renderer/src/lib.rs`: one `SurfaceRenderer` (one device, queue, surface,
  `GridRenderer`). `GridRenderer::draw` draws all fills and glyphs of one `Scene`, then the
  optional CRT pass over the whole surface. Glyph clip bounds are the glyph rect. The renderer
  needs no change for panes when every pane is drawn into one window `Scene` (Q2, Q3).
- `crates/renderer/src/effects.rs`: `RenderMode::{source_point, display_point}` warp around the
  whole surface centre. `Crt::draw` clears and covers the whole surface.
- `crates/renderer/src/scene.rs`: `Scene { clear, font_size, line_height, fills, glyphs }`.
  `Rect`/`Fill` derive `Debug`, `Rect` derives `PartialEq`; `Glyph` derives `Clone, Debug`; `Scene` derives nothing.
- `crates/terminal/src/session.rs`: `Session::spawn(SessionOptions, wake)`, `id()`, `snapshot()`,
  `poll()` (must be called for hidden sessions too; forwards protocol replies), `input`,
  `resize(GridSize)` (PTY + grid), `scroll`, `scroll_to_bottom`, `select`, `clear_selection`,
  `selected_text`, `close()` (may block while Alacritty waits for the child), `Drop` calls `close`.
  `SessionId` is process-unique. The child PID is not exposed (matters for Q8).
- `crates/terminal/src/engine.rs`: `GridSize::is_valid` needs `columns >= MIN_COLUMNS` (2) and
  `rows >= MIN_SCREEN_LINES` (1).
- Perf harness (`xtask/src/perf.rs`, `xtask/src/perf/run.py`, `fixture.py`, `wayland.py`):
  - `perf.rs` `SCENARIOS` is a fixed list of 8; `Suite::parse` rejects any other scenario set.
  - `run.py::terminal()` starts one emulator with one fixture and `server.listen(1)` / one `accept()`.
  - `wayland.py::visible()` finds one solid-colored grid rectangle. The CRT probe
    (`visible_crt`, `crt_point`, `crt_evidence`) assumes the grid fills the whole warped surface.
  - `run.py::matching()` requires equal grids for both legs.
  - Conclusion: multi-session scenarios need harness work (Q13–Q15). The existing 8 single-pane
    scenarios keep working if D5 holds.

## Design (follows the Q1–Q16 decisions)

### New modules

`crates/app/src/layout.rs` — pure, no `cfg` gate, `pub mod layout;` in `lib.rs`:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PaneId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis { Columns /* side by side, vertical divider */, Rows /* stacked */ }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction { Left, Right, Up, Down }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect { pub x: u32, pub y: u32, pub width: u32, pub height: u32 }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits { pub cell: [u32; 2], pub divider: u32, pub min_cells: [u32; 2] }
enum Node { Leaf(PaneId), Split { axis: Axis, ratio: f32, first: Box<Node>, second: Box<Node> } }
pub struct Layout { root: Node, focus: PaneId }
```

`Layout` methods (all take the tab's terminal `area: Rect` and `limits` where geometry matters):
`new(pane)`, `focus()`, `panes() -> Vec<PaneId>` (tree order), `rects(area, limits) -> Vec<(PaneId, Rect)>`,
`dividers(area, limits) -> Vec<Rect>`, `split(new, axis, area, limits) -> bool` (splits the focused
pane, focuses `new`; refuses when either half is below `min_cells`), `close(pane) -> Option<PaneId>`
(collapses the parent into the sibling; returns the new focus, `None` when the tree is empty;
the sibling subtree that takes the parent's place gets focus at its first leaf), `set_focus(pane) -> bool`,
`focus_toward(direction, area, limits) -> bool`, `resize(direction, area, limits) -> bool` (moves the
nearest ancestor divider on that axis by one cell, clamped so no pane goes below `min_cells`),
`pane_at([x, y], area, limits) -> Option<PaneId>` (`None` on a divider).

Geometry rule: the first child gets `floor((size - divider) * ratio)` rounded down to a whole number
of cells (`limits.cell`), the divider follows, the second child gets the rest. Rounding the first
child to whole cells keeps text flush with the divider. Default ratio 0.5 (Q5).

`crates/app/src/tabs.rs` — pure, no `cfg` gate, `pub mod tabs;`:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TabId(pub u64);
pub struct Tab { pub id: TabId, pub layout: Layout }
pub struct Tabs { tabs: Vec<Tab>, active: usize, next_id: u64 }
```

Methods: `new() -> (Tabs, PaneId)`, `open() -> PaneId` (new tab after the active one, activated),
`split(axis, area, limits) -> Option<PaneId>`, `close(pane) -> bool` (true when no tab is left; an
empty tab is removed and the next tab, else the previous one, becomes active), `select(index)`,
`next()`, `previous()` (wrap), `active() -> &Tab`, `active_mut()`, `focused() -> PaneId`,
`is_visible(pane) -> bool` (in the active tab), `len()`, `iter()`, `tab_of(pane) -> Option<usize>`.
`Tabs` allocates both `TabId` and `PaneId` from `next_id` (deterministic for tests).

`crates/app/src/ui.rs` — Linux-only (uses `retro_renderer::scene`), stateless helpers:
`tab_bar(scene, area: Rect, labels: &[(String, bool)], grid_cell)` (one cell row, active tab inverted,
labels `"{n}: {title}"` truncated to fit), `dividers(scene, &[Rect])`, and the banner helper moved
from `scene::overlay` call sites if convenient. Task 11 adds controls here.

### Pane state in `app.rs`

```rust
struct Pane { session: Session, grid: GridSize, origin: [u32; 2], title: Option<String>, scroll_fraction: f64 }
```

`State` replaces `session`, `grid`, `scroll_fraction` with `tabs: Tabs`, `panes: HashMap<PaneId, Pane>`,
`sessions: HashMap<SessionId, PaneId>`, `cell: (u16 /*width*/, u16 /*line height*/)`, and
`selection_start: Option<(PaneId, Position)>`.

### Frame composition

- Terminal area = window minus the tab bar row (tab bar only when `tabs.len() >= 2`, Q2).
- One `Scene` per frame: `clear` = focused pane background. For each pane of the **active tab only**:
  take `snapshot()`, fill the pane rect with its background when it differs from `clear`, append
  its cells at the pane origin. Then dividers, then the tab bar, then the banner.
- Unfocused panes: cursor drawn as `HollowBlock` (unless `Hidden`), no preedit, no blink (Q4).
- Hidden tabs: never snapshotted or converted. Their `poll()` still runs on every wakeup; a
  `view_changed` from a hidden pane does not call `scheduler.invalidate()`.
- One CRT pass over the whole window (Q3). Pointer hit testing: `mode.source_point(point, window size)`,
  then `layout.pane_at` / tab bar hit test, then `cell_at` with the pane-relative point.

### Lifecycle

- New pane or tab: compute the new layout, spawn the session with the new pane's grid, then resize
  the panes whose rects changed. If the spawn fails, `tabs.close(new_pane)` and show the error in the banner.
- Shell exit in a pane: close that pane (`Session::close`, remove from `panes` and `sessions`,
  `tabs.close`), resize the remaining panes of that tab, invalidate. Last pane: exit the app as today (Q6).
- Close shortcut: same path, no confirmation (Q6).
- Window resize, scale change, layout-changing reload: resize every pane in every tab
  (hidden ones too, Q12), renderer once.

## Steps

Execute in order. After each app step run `cargo xtask check -p retro-app` for a short loop.

1. **Pure layout.** Add `crates/app/src/layout.rs` with the types and methods above and
   `pub mod layout;` (no `cfg` gate) in `lib.rs`. No `winit`, `retro_renderer`, or
   `retro_terminal` types in this file. Add `crates/app/tests/layout.rs` (tests below).
2. **Pure tabs.** Add `crates/app/src/tabs.rs` and `pub mod tabs;` (no `cfg` gate). Add its tests
   to `crates/app/tests/layout.rs` (one integration test file for both, as the architecture lists).
3. **Input actions.** In `crates/app/src/input.rs` extend `Action` with `NewTab`, `ClosePane`,
   `NextTab`, `PreviousTab`, `Split(layout::Axis)`, `Focus(layout::Direction)`,
   `Resize(layout::Direction)`. Map the Q1 keys **before** the `special(..)` and
   `control_byte(..)` paths, in the same block style as Copy/Paste/ReloadConfig. Keys not in Q1
   keep their current bytes. Add unit tests (below).
4. **Scene per pane.** In `crates/app/src/scene.rs` move the body of `convert` into
   `pub(crate) fn pane(scene: &mut Scene, snapshot: &Snapshot, origin: [u32; 2], focused: bool, cursor_visible: bool, preedit: &str, banner: &str)`.
   `bounds` takes the origin. `convert` stays as
   `Scene::new(..)` + `pane(.., [0, 0], true, ..)` so existing tests keep passing unchanged.
   Unfocused: hollow cursor, ignore `preedit`. Fill the pane's grid rect with its background when
   it differs from `scene.clear`. Add `ui.rs` (`tab_bar`, `dividers`) and `mod ui;` with the
   Linux `cfg` gate in `lib.rs`.
5. **App state.** In `crates/app/src/app.rs`:
   - Add `Pane`, replace the single session fields as described. Extract
     `fn spawn(&self, grid: GridSize, program: Option<Program>) -> AppResult<Session>` (wake callback
     sends `Event::Terminal(id)`).
   - `State::new`: create `Tabs::new()`, spawn the first pane (`--` program per Q7), register it.
   - `fn area(&self) -> Rect` (window minus tab bar) and `fn limits(&self) -> Limits` (cell size,
     divider thickness, min cells from Q5).
   - `State::resize`: keep the font metrics and cell-width cache, compute `area`, then for every tab
     `layout.rects(area, limits)`; per pane `scene::grid_size([w, h], ..)`; on `Some(grid) != pane.grid`
     call `session.resize(grid)` (ignore `SessionError::Closed` as today); store `origin`.
     A pane whose rect is too small keeps its old grid and is not drawn this frame.
   - `App::user_event`: look up `Event::Terminal(id)` in `sessions`; ignore unknown ids (a closed pane).
     `poll_pane(pane)` returns `exited`; title changes update `pane.title` and the window title only
     when the pane is focused in the active tab; `view_changed` invalidates only when `tabs.is_visible(pane)`.
     On exit call `close_pane(pane)`; when it returns "no tab left", `App::close`.
   - `State::action`: handle the new actions. `NewTab`/`Split`: spawn per Lifecycle. `ClosePane`:
     `close_pane(focused)`. `NextTab`/`PreviousTab`: switch, then `activate()`. `Focus`: `focus_toward`,
     then `activate()`. `Resize`: `layout.resize`, then `self.resize(window size)`.
   - `fn activate(&mut self)`: clear `selection_start`, `composition.preedit`, `previous_ime_cursor`;
     set the window title from the focused pane; `scheduler.invalidate()` (full redraw on reveal).
   - Input routing: `send_input`, `Copy`, `Paste`, IME commit, keyboard modes → focused pane.
     Mouse press → `pane_at`, focus that pane (`activate` if focus changed), start the selection there.
     Drag clamps inside the selection's pane. Wheel scrolls the pane under the pointer (Q10).
     Tab bar click selects the tab.
   - `State::draw`: compose the window `Scene` (Frame composition). IME cursor area uses the focused
     pane's origin. `Frame::RecreateDevice` path unchanged.
   - `State::reload`: `changes.layout` resizes all panes; change the `new_sessions` message to
     "configuration reloaded; shell and scrollback settings apply to new panes and tabs".
   - `App::close`: close every pane's session; return the first error.
6. **Terminal crate.** No API change expected. Add the lifecycle test below to
   `crates/terminal/tests/session.rs`. If Q16 picks a background close, confirm `Session: Send`
   with a compile-time assertion in that test file first.
7. **Docs.** `docs/configuration.md`: `--` program scope (Q7), "new shells" -> "new panes and tabs".
   `docs/development.md`: shortcut list next to Ctrl+Shift+C/V/R. `docs/architecture.md`: drop
   "tabs and splits" from "None of these exist yet" only if the feature is merged-ready.
   `docs/performance.md`: add the multi-session scenario definitions (below) marked "planned; harness not implemented"
   unless Q13 picks (b).
8. **Perf.** Per Q13. Default (a): no harness code in this task. Run the preflight (expected exit 3)
   and record that the existing single-pane suite is unchanged by D5 (the scene identity test).
9. **Index and handoff.** Set this plan's status and the row in `docs/AI/README.md` from evidence only.
   Fill in the handoff.

## Tests to add

All headless; they run in `cargo xtask check`.

`crates/app/tests/layout.rs` (uses `retro_app::layout` and `retro_app::tabs`). Use a fixed
`Limits { cell: [8, 20], divider: 2, min_cells: [10, 3] }` and area `Rect { 0, 0, 802, 600 }`:

- `single_pane_fills_the_area`.
- `column_split_snaps_first_pane_to_cells_and_leaves_a_divider` (exact rects: 400/2/400 for 802 wide).
- `nested_splits_tile_the_area_without_overlap` (rects + dividers cover the area exactly once).
- `split_is_refused_below_the_minimum` (narrow area; tree unchanged; focus unchanged).
- `close_collapses_the_parent_and_focuses_the_sibling` (nested 3-pane case, sibling is a subtree).
- `closing_the_last_pane_empties_the_layout`.
- `focus_moves_to_the_adjacent_pane` (2×2 grid: each direction, and no move at the edge).
- `resize_moves_the_nearest_divider_by_one_cell_and_clamps`.
- `pane_at_maps_points_and_divider_is_none`.
- `window_resize_keeps_ratios` (same tree, larger area, proportional rects).
- `repeated_split_and_close_returns_to_one_pane` (100 cycles; one leaf left; ids never reused).
- `tabs_open_after_active_and_activate`, `next_and_previous_wrap`,
  `closing_a_tab_activates_the_next_then_previous`, `closing_the_last_pane_of_the_last_tab_reports_empty`,
  `only_active_tab_panes_are_visible`.

Unit tests in existing modules:

- `input.rs`: every Q1 binding gives its action; the same keys without the extra modifier still
  send their bytes (for example Ctrl+T -> `0x14`, Alt+Left without Shift per Q1, PageDown without Ctrl);
  Copy/Paste/Reload unchanged.
- `scene.rs`: `single_pane_scene_equals_convert` (`pane(.., [0,0], true, ..)` vs `convert`, compare
  `format!("{:?}")` of `fills` and `glyphs`, and `clear`) — guards D5;
  `pane_origin_offsets_fills_and_glyphs`; `unfocused_pane_draws_a_hollow_cursor_and_no_preedit`.
- `ui.rs`: `tab_bar_marks_the_active_tab_and_truncates_labels` (1-column and narrow widths do not panic);
  `dividers_are_filled`.
- `crates/terminal/tests/session.rs`: `repeated_spawn_and_close_releases_descriptors` (20 cycles of
  `Session::spawn` + `close`; the count of entries in `/proc/self/fd` returns to its start value).

Not automatable here: live window behavior (needs Wayland and a person). Cover it in the smoke check.

## Multi-session benchmark scenarios (definitions)

Matched rule: equal total visible cells and equal per-session grids between legs; same fixture,
font, scrollback, cursor settings as the single-pane suite. Topology per Q15 (default: one Alacritty
process, N windows from `alacritty msg create-window -e <fixture>`; hidden = minimized window).

| Scenario | Candidate | Baseline | Measures |
| --- | --- | --- | --- |
| `split-idle` | 1 window, 2 panes side by side, non-blinking, 4 s | 2 windows, same grids | No frames, CPU/RSS/GPU at idle |
| `split-output` | 2 panes, 300-line bursts ×4 alternating | 2 windows | Output path with two visible sessions |
| `hidden-output` | 2 tabs, hidden tab outputs 300 lines ×4, then reveal | 2 windows, one minimized, then restored | Hidden work is parse-only; reveal shows the final frame |
| `pane-lifecycle` | 20× split + close | 20× create-window + close | Reaped children, RSS returns, no growth |

Recheck all 8 single-pane scenarios with the same build (D5 makes them comparable to task 06/07).
Harness work needed when implemented: a separate suite file (for example `benchmarks/multi-session.toml`)
or suite version 2 with its own scenario list in `perf.rs`; fixture server accepting N connections with
per-session identity; one marker color per session in `visible()`; key injection for the split/tab
shortcuts; CRT probe per Q14.

## Acceptance criteria

- Tabs and nested splits work in one window without an external multiplexer.
- Each pane has its own shell; its PTY and grid get the same rows and columns on every layout change.
- Input, IME, copy, and paste go to the focused pane only.
- Hidden tabs do no snapshot, text, or GPU work; revealing a tab redraws it fully.
- One `SurfaceRenderer` (one device and surface) for the window; no per-pane GPU resources.
- Closing a pane reaps its session and collapses the layout; the last pane exits the app.
- One tab with one pane renders exactly as the base (D5, tested).
- `cargo xtask check` passes with all steps (D6).
- Performance: **unverified on this host** (D4). Not a pass.

## Verification

Run in the worktree:

```sh
cargo xtask check
cargo xtask check -p retro-app          # short loop during steps 1-5
cargo build --locked --release -p retro-app --bin retro-term
cargo test --locked -p retro-renderer --test gpu_render -- --ignored --nocapture   # only if the renderer changed (Q2 c / Q3 b)
cargo xtask run
cargo xtask perf --candidate target/release/retro-term --baseline alacritty --suite benchmarks/scenarios.toml --output target/perf/preflight.json --preflight
```

- `cargo xtask run` needs a native Wayland session; log at `target/xtask/logs/run.log`. Smoke check
  (needs a person): open a tab, split both ways, nest a split, move focus, resize a divider, run
  `stty size` in each pane after each change, type into the focused pane only, run
  `yes | head -c 50M` in a hidden tab and switch back, close panes until the app exits, reload
  the config with a font size change and check `stty size` in every pane.
- The perf preflight is expected to exit 3 (Alacritty unavailable). Record the exit code and
  `target/perf/preflight.json`. Do not run the full suite as evidence on this host.

## Out of scope

Settings UI (task 11), configurable key bindings, divider mouse drag (unless Q10 says otherwise),
tab reordering, tab renaming, multiple windows, session persistence or daemon, per-pane CRT
(unless Q3 says otherwise), animation (task 09), packaging (task 10).

## Decisions on Q1–Q16 (user, final)

Follow-up decision: a refused split shows "pane too small to split" in the focused pane's banner
(the 07 config-error banner; cleared on the next key press or reload). Covered headlessly by
`split_is_refused_below_the_minimum` (refusal) and `banner_covers_the_last_row_within_the_grid`
(banner drawing); the `State` wiring needs a window and is part of the smoke check.

The user accepted every recommended option (marked **rec**). The other options stay listed for context only.

- **Q1. Key bindings.** (a) **rec**: Ctrl+Shift+T new tab, Ctrl+Shift+W close pane,
  Ctrl+Shift+E split side by side, Ctrl+Shift+O split stacked, Ctrl+PageDown/PageUp next/previous tab,
  Alt+Arrow focus, Ctrl+Shift+Arrow resize (Terminator-like; steals `CSI 1;3X`, `CSI 1;6X`, `CSI 5/6;5~`
  from programs). (b) letters only: Ctrl+Shift+T/W/E/O, Ctrl+Shift+H/J/K/L focus, Ctrl+Shift+[ / ]
  tabs, Ctrl+Alt+Shift+H/J/K/L resize (steals only duplicate control bytes). (c) other set.
- **Q2. Tab bar.** (a) none; window title shows `[2/3] title`. (b) **rec**: one cell row at the top,
  drawn in the window `Scene` (so it is warped by the CRT pass), shown only with 2+ tabs; no
  renderer change; keeps D5. (c) drawn after the CRT pass as the architecture says; needs a second
  text renderer and an overlay pass in `retro-renderer`, more GPU work per frame.
- **Q3. CRT with several panes.** (a) **rec**: one pass over the whole window (current code; the
  architecture's "one effect pipeline"). (b) one pass per pane (each pane curved; N textures and passes).
- **Q4. Dividers and focus indicator.** (a) **rec**: divider `max(1, round(2 × scale))` px, solid
  gray `Rgb(90, 90, 90)`; unfocused panes show a hollow cursor; no border. (b) also dim unfocused panes.
  (c) colored border on the focused pane.
- **Q5. Split geometry.** (a) **rec**: ratio 0.5; minimum pane 10 columns × 3 rows; resize step one
  cell; first child snapped to whole cells; split refused when a half would be below the minimum.
  (b) other values.
- **Q6. Closing.** (a) **rec**: a shell exit closes its pane; the last pane closes the window and the
  app (same as today); Ctrl+Shift+W closes without confirmation; a closed tab activates the next tab,
  else the previous. (b) keep a dead pane open with "process exited" until a key press.
- **Q7. Program after `--` for new panes.** (a) **rec**: only the first pane runs it; new panes and
  tabs use the config/login shell (as kitty and WezTerm do). Implementation: take `overrides.program`
  out before `config::load`, spawn the first pane with it, keep `Overrides { program: None, .. }` for
  reloads. (b) every new pane runs it again (current `Config::resolve` behavior; makes the perf
  harness simpler because every pane runs the fixture).
- **Q8. Working directory of a new pane.** (a) **rec**: `shell.working_directory` (or inherit), as
  today. (b) the focused pane's current directory (needs the child PID from `retro-terminal`, an API change).
- **Q9. Config keys.** (a) **rec**: no new keys in this task; layout constants are code. (b) add
  `[layout]` keys (divider width, minimum pane size) to `Config`, `Layer`, presets, example file,
  docs, and tests.
- **Q10. Mouse.** (a) **rec**: click focuses the pane under the pointer and starts selection there;
  click on a tab selects it; wheel scrolls the pane under the pointer; no divider drag. (b) also
  drag dividers.
- **Q11. Config error banner.** (a) **rec**: last row of the focused pane (current code with an origin).
  (b) bottom row across the whole window.
- **Q12. Hidden tabs on window resize.** (a) **rec**: resize every pane in every tab at once (PTY
  sizes stay correct; no GPU work). (b) resize hidden tabs only on reveal.
- **Q13. Perf harness scope.** (a) **rec**: this task defines the scenarios in `docs/performance.md`
  and keeps the single-pane suite valid (D5); the harness implementation is a follow-up task run on
  the measurement host once Alacritty 0.17.0 is installed (it cannot be validated here).
  (b) implement the harness now, validated only by headless Python tests.
- **Q14. CRT capture probe with several panes.** (a) measure multi-session scenarios in plain mode
  first; CRT multi-pane stays unverified. (b) **rec** (when the harness is built): find the window's
  warped area, then map each pane's source rect through the window-level warp (`crt_point` with the
  window area and pane-relative fractions); the harness computes pane rects with the Q5 rule.
  (c) per-pane CRT (Q3 b), which makes the current probe work per pane.
- **Q15. Alacritty comparison topology.** (a) **rec**: one Alacritty process with N windows via
  `alacritty msg create-window -e <fixture>`; hidden tab = minimized window; per-window grid equal to
  the pane grid. (b) N separate Alacritty processes, metrics summed. (c) tmux inside one Alacritty
  (adds a process the candidate does not have).
- **Q16. Blocking close.** (a) **rec**: `Session::close` on the main thread, as the current exit path
  does; record any visible stall in the smoke check. (b) close on a background thread (needs `Session: Send`).

## Contradictions found while planning

- Ceiling: the old version of this plan and `docs/AI/README.md`/architecture say 1.20×; the user and
  `docs/decisions/05-renderer-baseline.md` set 1.40× for this phase (D3).
- `docs/AI/README.md` says 08 needs "07 completed"; 07 is implemented locally with the smoke check and
  performance unverified. The user accepted stacking on `07-text-configuration` (D1).
- Architecture: "Draw the tab bar and other controls after CRT effects." The recommended Q2 (b) draws
  the tab bar inside the CRT pass to avoid a renderer change and extra GPU work. Needs the user's choice.
- The old plan step 7 says "extend the benchmark", but `perf.rs` accepts exactly the 8 fixed scenarios,
  `run.py` accepts one fixture connection, and the CRT probe assumes one full-surface grid. This is a
  harness project of its own (Q13).
- `Config::resolve` merges the `--` program into `config.shell`, so without a change every new pane
  would re-run that program. `docs/configuration.md` says the `--` program "replaces shell.program"
  with no pane scope (Q7).
- `docs/configuration.md` and the reload message say "new shells"; with tabs this means new panes
  and tabs.
- The architecture tree lists `crates/app/tests/scheduler.rs` and `crates/renderer/tests/rendering.rs`;
  the code has scheduler unit tests and `crates/renderer/tests/gpu_render.rs`. Not a blocker.
- The task 07 handoff records deny/actionlint/zizmor as missing; the user says they are now installed (D6).

## Progress

- Steps 1–9 done. Deviations:
  - The descriptor test is in its own file, `crates/terminal/tests/descriptors.rs`, not in
    `session.rs`: the other session tests run in parallel in the same process and change the
    `/proc/self/fd` count (first attempt failed 12 -> 5). One warm-up cycle runs before the count.
  - `Pane.origin` is `Option<[u32; 2]>` (`None` = too small, not drawn) instead of `[u32; 2]`.
  - `scene::convert` is `#[cfg(test)]` (the window uses `scene::empty` + `scene::pane`);
    `scene::pointer_cell` takes the pane origin. `ui::tab_bar` takes the cell width; `ui::tab_at`
    shares the tab geometry with the click hit test.
  - A refused split (pane below the minimum) shows "pane too small to split" in the focused pane's
    banner (user decision after the first handoff; same banner and clearing as config errors). A failed new pane shows
    "cannot open a pane: ..." in the banner; a failed new tab returns to the previous tab.
  - `docs/architecture.md` unchanged: the feature is not merge-ready until the smoke check runs.

## Handoff

- Base: `843716f0d7bdcfbbfb4e42eedddbb06ab56f4988` on `07-text-configuration`.
- Worktree: `.worktrees/08-tabs-and-splits` (branch `08-tabs-and-splits`). Nothing staged or committed.
- Changed files: `crates/app/src/{layout.rs (new), tabs.rs (new), ui.rs (new), lib.rs, input.rs, scene.rs, app.rs}`,
  `crates/app/tests/layout.rs` (new), `crates/terminal/tests/descriptors.rs` (new),
  `docs/configuration.md`, `docs/development.md`, `docs/performance.md`, `docs/AI/README.md`, this file.
- Decisions: Q1–Q16 all **rec** (user). Refused split shows a banner (user, follow-up). No renderer change (Q2 b, Q3 a), so no renderer code changed.
- Commands run and results:
  - `cargo xtask check`: fmt, check, clippy, test, deny, actionlint, zizmor passed; ignored-tests not_run (the GPU test, run below).
  - `cargo build --locked --release -p retro-app --bin retro-term`: passed.
  - `cargo test --locked -p retro-renderer --test gpu_render -- --ignored`: 1 passed (AMD Radeon 890M, RADV).
  - `timeout 8 cargo xtask run`: window started, `run.log` shows the RADV adapter and "frame presented"; stopped by the timeout (exit 124).
  - `cargo xtask perf ... --preflight`: exit 3, `target/perf/preflight.json` unverified: "baseline executable unavailable: alacritty".
- Not run, and why: the interactive smoke check (tabs, splits, focus, resize, `stty size`, hidden-tab output, close, reload) needs a person on the Wayland session; the full perf suite (Alacritty 0.17.0 missing, D4). Performance: **unverified**, not a pass.
- Artifacts: `target/xtask/check.json`, `target/xtask/logs/run.log`, `target/perf/preflight.json`.
- Blockers and next step: a person runs the smoke check in "Verification"; install Alacritty 0.17.0 and run the single-pane suite; multi-session harness is a follow-up task (Q13 a).
