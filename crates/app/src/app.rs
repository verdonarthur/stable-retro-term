use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use retro_renderer::{FontError, Frame, RenderMode, SurfaceRenderer};
use retro_terminal::{
    CursorShape, GridSize, Input, Position, Program, Session, SessionError, SessionId,
    SessionOptions, TitleChange,
};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowId};

use crate::config::{self, Changes, Config, ConfigError, Overrides, Source};
use crate::input::{self, Action, Composition};
use crate::layout::{Limits, PaneId, Rect};
use crate::platform;
use crate::scene;
use crate::scheduler::Scheduler;
use crate::settings::{Outcome, Settings};
use crate::tabs::Tabs;
use crate::ui;

const TITLE: &str = "Stable Retro Term";
const INITIAL_SIZE: LogicalSize<u32> = LogicalSize::new(800, 500);
const MIN_PANE_CELLS: [u32; 2] = [10, 3];
type AppResult<T> = Result<T, Box<dyn Error>>;

enum Event {
    Terminal(SessionId),
    Paste(Result<String, String>),
}

pub fn run(development: bool) -> AppResult<()> {
    run_with_program(development, None)
}

pub fn run_with_program(development: bool, program: Option<Program>) -> AppResult<()> {
    run_with_mode(development, false, program)
}

pub fn run_with_mode(development: bool, plain: bool, program: Option<Program>) -> AppResult<()> {
    let overrides = Overrides {
        plain,
        program: program.map(|program| (program.executable, program.arguments)),
    };
    run_with_config(
        development,
        Source::Default(platform::config_path()),
        overrides,
    )
}

/// An invalid file starts the defaults and shows the error in the window.
/// An explicit file that cannot be read is an error.
pub fn run_with_config(development: bool, source: Source, overrides: Overrides) -> AppResult<()> {
    use winit::platform::wayland::EventLoopBuilderExtWayland;
    let events = EventLoop::<Event>::with_user_event()
        .with_wayland()
        .build()?;
    let mut app = App {
        proxy: events.create_proxy(),
        state: None,
        error: None,
        development,
        source,
        overrides,
    };
    events.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}

struct App {
    proxy: EventLoopProxy<Event>,
    state: Option<State>,
    error: Option<Box<dyn Error>>,
    development: bool,
    source: Source,
    overrides: Overrides,
}

impl App {
    fn close(&mut self, events: &ActiveEventLoop, error: Option<Box<dyn Error>>) {
        let close_error = self
            .state
            .take()
            .and_then(|mut state| state.close_all().err());
        self.error = error.or_else(|| close_error.map(Into::into));
        events.exit();
    }
}

struct Pane {
    session: Session,
    grid: GridSize,
    /// `None` while the pane is too small to hold a grid; it is not drawn then.
    origin: Option<[u32; 2]>,
    title: Option<String>,
    scroll_fraction: f64,
}

struct State {
    renderer: SurfaceRenderer,
    window: Arc<Window>,
    tabs: Tabs,
    panes: HashMap<PaneId, Pane>,
    sessions: HashMap<SessionId, PaneId>,
    proxy: EventLoopProxy<Event>,
    scheduler: Scheduler,
    modifiers: ModifiersState,
    pointer: Option<PhysicalPosition<f64>>,
    selection_start: Option<(PaneId, Position)>,
    composition: Composition,
    /// Cell width and line height in physical pixels.
    cell: (u16, u16),
    font_size: u16,
    occluded: bool,
    paste_pending: bool,
    gpu_recovered: bool,
    development: bool,
    config: Config,
    source: Source,
    overrides: Overrides,
    banner: String,
    previous_ime_cursor: Option<(GridSize, Position, [u32; 2])>,
    /// Counts presented frames. Only the animated CRT mode reads it.
    animation_frame: u32,
    /// The open settings panel. It takes every key while open.
    settings: Option<Settings>,
    saved_settings_config: Option<Config>,
    settings_deadline: Option<Instant>,
    shortcuts_open: bool,
    menu: Option<(usize, Option<[f64; 2]>)>,
}

impl State {
    fn new(
        events: &ActiveEventLoop,
        proxy: EventLoopProxy<Event>,
        development: bool,
        source: Source,
        mut overrides: Overrides,
    ) -> AppResult<Self> {
        // Only the first pane runs the program after `--`; new panes use the configured shell.
        let first_program = overrides
            .program
            .take()
            .map(|(executable, arguments)| Program {
                executable,
                arguments,
            });
        let (mut config, mut startup_error) = match config::load(&source, &overrides) {
            Ok(config) => (config, None),
            Err(error @ ConfigError::Io(_)) if matches!(source, Source::Explicit(_)) => {
                return Err(config_message(&source, &error).into());
            }
            Err(error) => (fallback(&overrides)?, Some(error)),
        };
        let window = Arc::new(
            events.create_window(
                Window::default_attributes()
                    .with_title(TITLE)
                    .with_inner_size(INITIAL_SIZE)
                    .with_min_inner_size(LogicalSize::new(80, 48)),
            )?,
        );
        window.set_ime_allowed(true);
        let mut renderer = match create_renderer(events, &window, development, &config) {
            Err(error) if error.is::<FontError>() => {
                startup_error = Some(font_error(&config));
                config = fallback(&overrides)?;
                create_renderer(events, &window, development, &config)?
            }
            result => result?,
        };
        let (font_size, line_height) = scene::font_metrics(
            window.scale_factor(),
            config.font.size,
            config.font.line_height,
        );
        let cell_width = renderer.cell_width(font_size, line_height);
        let fallback = INITIAL_SIZE.to_physical::<u32>(window.scale_factor());
        let grid = scene::grid_size(window.inner_size().into(), cell_width, line_height)
            .or_else(|| scene::grid_size(fallback.into(), cell_width, line_height))
            .ok_or_else(|| io::Error::other("cannot calculate the initial terminal grid"))?;
        let first_program = first_program.or_else(|| program(&config));
        let session = spawn(&proxy, &config, grid, first_program)?;
        let (tabs, pane) = Tabs::new();
        let mut state = Self {
            renderer,
            window,
            tabs,
            panes: HashMap::new(),
            sessions: HashMap::new(),
            proxy,
            scheduler: Scheduler::default(),
            modifiers: ModifiersState::empty(),
            pointer: None,
            selection_start: None,
            composition: Composition::default(),
            cell: (cell_width, line_height),
            font_size,
            occluded: false,
            paste_pending: false,
            gpu_recovered: false,
            development,
            config,
            source,
            overrides,
            banner: String::new(),
            previous_ime_cursor: None,
            animation_frame: 0,
            settings: None,
            saved_settings_config: None,
            settings_deadline: None,
            shortcuts_open: false,
            menu: None,
        };
        state.insert(pane, session, grid);
        if let Some(error) = startup_error {
            state.show(config_message(&state.source, &error));
        }
        state.resize(state.window.inner_size())?;
        Ok(state)
    }

    fn resize(&mut self, size: PhysicalSize<u32>) -> AppResult<()> {
        let (font_size, line_height) = scene::font_metrics(
            self.window.scale_factor(),
            self.config.font.size,
            self.config.font.line_height,
        );
        let cell_width = if font_size == self.font_size && line_height == self.cell.1 {
            self.cell.0
        } else {
            self.renderer.cell_width(font_size, line_height)
        };
        let grid = scene::grid_size(size.into(), cell_width, line_height);
        self.scheduler
            .set_presentable(grid.is_some() && !self.occluded);
        if grid.is_none() {
            self.renderer.resize([0, 0])?;
            return Ok(());
        }
        self.renderer.resize(size.into())?;
        self.cell = (cell_width, line_height);
        self.font_size = font_size;
        self.layout_panes()
    }

    fn window_rect(&self) -> retro_renderer::scene::Rect {
        let [width, height] = self.renderer.size();
        retro_renderer::scene::Rect {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    fn open_settings(&mut self) {
        let path = self.source.path().map(Path::to_path_buf);
        let overrides = self.overrides.clone();
        let settings = match path.as_deref().map(fs::read_to_string) {
            Some(Err(error)) if error.kind() != io::ErrorKind::NotFound => {
                Settings::unreadable(path.unwrap_or_default(), &error, overrides)
            }
            Some(Ok(text)) => Settings::open(path, Some(text), overrides),
            _ => Settings::open(path, None, overrides),
        };
        self.window.set_ime_allowed(false);
        self.saved_settings_config = Some(self.config.clone());
        self.settings = Some(settings);
        self.scheduler.invalidate();
    }

    fn close_settings(&mut self) {
        if self.settings.take().is_some() {
            self.window.set_ime_allowed(true);
            self.scheduler.invalidate();
        }
    }

    /// Saves, then applies the file through `reload`, the same as Ctrl+Shift+R.
    fn settings_outcome(&mut self, outcome: Outcome) -> AppResult<()> {
        match outcome {
            Outcome::Nothing => {}
            Outcome::Redraw => self.scheduler.invalidate(),
            Outcome::Close => {
                self.settings_deadline = None;
                self.close_settings();
                if let Some(saved) = self.saved_settings_config.take()
                    && let Some(error) = self.apply_settings_config(saved)?
                {
                    self.show(error.to_string());
                }
            }
            Outcome::Save(text) => {
                self.settings_deadline = None;
                let Some(candidate) = self
                    .settings
                    .as_ref()
                    .map(|settings| settings.preview().clone())
                else {
                    return Ok(());
                };
                if let Some(error) = self.apply_settings_config(candidate)? {
                    if let Some(settings) = &mut self.settings {
                        settings.set_status(error.to_string());
                    }
                    self.scheduler.invalidate();
                    return Ok(());
                }
                let Some(settings) = &mut self.settings else {
                    return Ok(());
                };
                let Some(path) = settings.path() else {
                    return Ok(());
                };
                match config::save(path, settings.original(), &text) {
                    Ok(()) => {
                        settings.mark_saved(text);
                        self.saved_settings_config = Some(self.config.clone());
                        self.scheduler.invalidate();
                    }
                    Err(error) => {
                        eprintln!("retro-term: settings: {error}");
                        settings.set_status(error.to_string());
                        self.scheduler.invalidate();
                    }
                }
            }
        }
        Ok(())
    }

    /// Gives every pane in every tab, hidden ones too, the grid of its layout rect.
    fn layout_panes(&mut self) -> AppResult<()> {
        let (area, limits) = (self.area(), self.limits());
        let (cell_width, line_height) = self.cell;
        let mut changed = false;
        for tab in self.tabs.iter() {
            for (id, rect) in tab.layout.rects(area, limits) {
                let Some(pane) = self.panes.get_mut(&id) else {
                    continue;
                };
                let grid = scene::grid_size([rect.width, rect.height], cell_width, line_height);
                let origin = grid.map(|_| [rect.x, rect.y]);
                changed |= origin != pane.origin;
                pane.origin = origin;
                let Some(grid) = grid.filter(|grid| *grid != pane.grid) else {
                    continue;
                };
                match pane.session.resize(grid) {
                    Ok(()) => {
                        changed = true;
                        pane.grid = grid;
                        if self.selection_start.is_some_and(|(pane, _)| pane == id) {
                            self.selection_start = None;
                        }
                    }
                    Err(SessionError::Closed) => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        if changed {
            self.scheduler.invalidate();
        }
        Ok(())
    }

    /// The window without the tab bar. The bar shows only with two or more tabs.
    fn area(&self) -> Rect {
        let [width, height] = self.renderer.size();
        let bar = self.tab_bar_height();
        Rect {
            x: 0,
            y: bar,
            width,
            height: height.saturating_sub(bar),
        }
    }

    fn tab_bar_height(&self) -> u32 {
        if self.tabs.len() >= 2 {
            u32::from(self.cell.1)
        } else {
            0
        }
    }

    fn limits(&self) -> Limits {
        Limits {
            cell: [u32::from(self.cell.0), u32::from(self.cell.1)],
            divider: (2.0 * self.window.scale_factor()).round().max(1.0) as u32,
            min_cells: MIN_PANE_CELLS,
        }
    }

    fn insert(&mut self, id: PaneId, session: Session, grid: GridSize) {
        self.sessions.insert(session.id(), id);
        self.panes.insert(
            id,
            Pane {
                session,
                grid,
                origin: None,
                title: None,
                scroll_fraction: 0.0,
            },
        );
    }

    /// Starts the shell of a pane that the tabs already hold; on failure removes it again.
    fn open_pane(&mut self, id: PaneId) -> AppResult<()> {
        let (cell_width, line_height) = self.cell;
        let grid = self.tabs.tab_of(id).and_then(|index| {
            let (area, limits) = (self.area(), self.limits());
            let tab = self.tabs.iter().nth(index)?;
            let (_, rect) = tab
                .layout
                .rects(area, limits)
                .into_iter()
                .find(|(pane, _)| *pane == id)?;
            scene::grid_size([rect.width, rect.height], cell_width, line_height)
        });
        let session = match grid {
            Some(grid) => spawn(&self.proxy, &self.config, grid, program(&self.config))
                .map(|session| (session, grid))
                .map_err(|error| error.to_string()),
            None => Err("the window is too small".into()),
        };
        match session {
            Ok((session, grid)) => self.insert(id, session, grid),
            Err(message) => {
                self.tabs.close(id);
                self.show(format!("cannot open a pane: {message}"));
            }
        }
        self.layout_panes()?;
        self.activate();
        Ok(())
    }

    /// Closes the pane's shell and collapses its layout. Returns true when no tab is left.
    fn close_pane(&mut self, id: PaneId) -> AppResult<bool> {
        let visible = self.tabs.is_visible(id);
        let closed = match self.panes.remove(&id) {
            Some(mut pane) => {
                self.sessions.remove(&pane.session.id());
                pane.session.close()
            }
            None => Ok(()),
        };
        if self.tabs.close(id) {
            return closed.map(|()| true).map_err(Into::into);
        }
        self.layout_panes()?;
        if visible {
            self.activate();
        }
        closed.map(|()| false).map_err(Into::into)
    }

    fn close_all(&mut self) -> Result<(), SessionError> {
        self.sessions.clear();
        self.panes
            .drain()
            .map(|(_, mut pane)| pane.session.close())
            .fold(Ok(()), Result::and)
    }

    /// Resets per-view input state after the focused pane or active tab changed.
    fn activate(&mut self) {
        self.selection_start = None;
        self.composition.preedit.clear();
        self.previous_ime_cursor = None;
        let title = self
            .panes
            .get(&self.tabs.focused())
            .and_then(|pane| pane.title.as_deref())
            .unwrap_or(TITLE);
        self.window.set_title(title);
        self.scheduler.invalidate();
    }

    fn focused(&mut self) -> Option<&mut Pane> {
        self.panes.get_mut(&self.tabs.focused())
    }

    fn show(&mut self, message: String) {
        eprintln!("retro-term: {message}");
        self.banner = message;
        self.scheduler.invalidate();
    }

    fn apply_pending_settings(&mut self, now: Instant) -> AppResult<()> {
        if self
            .settings_deadline
            .is_some_and(|deadline| deadline <= now)
        {
            self.settings_deadline = None;
            if let Some(candidate) = self
                .settings
                .as_ref()
                .map(|settings| settings.preview().clone())
                && candidate != self.config
                && let Some(error) = self.apply_settings_config(candidate)?
                && let Some(settings) = &mut self.settings
            {
                settings.set_status(error.to_string());
            }
            self.scheduler.invalidate();
        }
        Ok(())
    }

    fn apply_settings_config(&mut self, candidate: Config) -> AppResult<Option<ConfigError>> {
        let changes = Changes::between(&self.config, &candidate);
        if changes.renderer
            && self
                .renderer
                .reconfigure(render_mode(&candidate), font_family(&candidate))
                .is_err()
        {
            return Ok(Some(font_error(&candidate)));
        }
        self.config = candidate;
        if changes.layout {
            self.font_size = 0;
            self.previous_ime_cursor = None;
            self.selection_start = None;
            self.resize(self.window.inner_size())?;
        }
        Ok(None)
    }

    /// Applies a valid file. An invalid file keeps the running state and shows the error.
    fn reload(&mut self) -> AppResult<()> {
        let candidate = config::load(&self.source, &self.overrides).and_then(|candidate| {
            if Changes::between(&self.config, &candidate).renderer {
                self.renderer
                    .reconfigure(render_mode(&candidate), font_family(&candidate))
                    .map_err(|_| font_error(&candidate))?;
            }
            Ok(candidate)
        });
        let (candidate, error) = config::reload(&self.config, candidate);
        if let Some(error) = error {
            self.show(config_message(&self.source, &error));
            return Ok(());
        }
        let changes = Changes::between(&self.config, &candidate);
        self.config = candidate;
        if changes.layout {
            self.font_size = 0;
            self.previous_ime_cursor = None;
            self.selection_start = None;
            self.resize(self.window.inner_size())?;
        }
        self.banner.clear();
        self.scheduler.invalidate();
        if changes.new_sessions {
            self.show(
                "configuration reloaded; shell and scrollback settings apply to new panes and tabs"
                    .into(),
            );
        }
        Ok(())
    }

    fn send_input(&mut self, input: Input<'_>) -> AppResult<()> {
        let Some(pane) = self.focused() else {
            return Ok(());
        };
        match pane.session.input(input) {
            Ok(()) => {
                if pane.session.scroll_to_bottom() {
                    self.scheduler.invalidate();
                }
                Ok(())
            }
            Err(SessionError::Closed) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    /// Returns true when the action closed the last pane.
    fn action(&mut self, action: Action) -> AppResult<bool> {
        let (area, limits) = (self.area(), self.limits());
        match action {
            Action::Bytes(bytes) => self.send_input(Input::Bytes(&bytes))?,
            Action::Copy => {
                if let Some(text) = self.focused().and_then(|pane| pane.session.selected_text())
                    && let Err(error) = platform::copy_text(text)
                {
                    eprintln!("clipboard copy: {error}");
                }
            }
            Action::Paste if !self.paste_pending => {
                let proxy = self.proxy.clone();
                thread::Builder::new()
                    .name("terminal paste".into())
                    .spawn(move || {
                        let text = platform::paste_text().map_err(|error| error.to_string());
                        let _ = proxy.send_event(Event::Paste(text));
                    })?;
                self.paste_pending = true;
            }
            Action::Paste => {}
            Action::ReloadConfig => self.reload()?,
            Action::Settings => self.open_settings(),
            Action::Panel => {
                self.shortcuts_open = false;
                if self.menu.take().is_none() {
                    self.open_menu(None);
                }
                self.scheduler.invalidate();
            }
            Action::Shortcuts => {
                self.shortcuts_open = true;
                self.menu = Some((0, None));
                self.scheduler.invalidate();
            }
            Action::CyclePane(forward) => {
                let layout = &mut self.tabs.active_mut().layout;
                let panes = layout.panes();
                if panes.len() > 1
                    && let Some(index) = panes.iter().position(|&pane| pane == layout.focus())
                {
                    let offset = if forward { 1 } else { panes.len() - 1 };
                    layout.set_focus(panes[(index + offset) % panes.len()]);
                    self.activate();
                }
            }
            Action::NewTab => {
                let previous = self.tabs.active_index();
                let pane = self.tabs.open();
                self.open_pane(pane)?;
                if !self.panes.contains_key(&pane) {
                    self.tabs.select(previous);
                    self.activate();
                }
            }
            Action::Split(axis) => match self.tabs.split(axis, area, limits) {
                Some(pane) => self.open_pane(pane)?,
                None => self.show("pane too small to split".into()),
            },
            Action::ClosePane => return self.close_pane(self.tabs.focused()),
            Action::NextTab => {
                self.tabs.next();
                self.activate();
            }
            Action::PreviousTab => {
                self.tabs.previous();
                self.activate();
            }
            Action::Focus(direction) => {
                if self
                    .tabs
                    .active_mut()
                    .layout
                    .focus_toward(direction, area, limits)
                {
                    self.activate();
                }
            }
            Action::Resize(direction) => {
                if self
                    .tabs
                    .active_mut()
                    .layout
                    .resize(direction, area, limits)
                {
                    self.layout_panes()?;
                }
            }
        }
        Ok(false)
    }

    /// The pointer in window pixels before the CRT warp.
    fn pointer_source(&self) -> Option<[f64; 2]> {
        let point = self.pointer?;
        Some(
            self.renderer
                .mode()
                .source_point([point.x, point.y], self.renderer.size()),
        )
    }

    fn pane_under_pointer(&self) -> Option<PaneId> {
        let point = self.pointer_source()?;
        self.tabs
            .active()
            .layout
            .pane_at(point, self.area(), self.limits())
    }

    fn pointer_cell(&self, id: PaneId, clamp: bool) -> Option<Position> {
        let point = self.pointer?;
        let pane = self.panes.get(&id)?;
        scene::pointer_cell(
            [point.x, point.y],
            self.renderer.size(),
            self.renderer.mode(),
            pane.origin?,
            pane.grid,
            clamp,
        )
    }

    fn menu_actions(context: bool) -> Vec<(&'static str, &'static str, Action)> {
        use crate::layout::Axis;
        let mut actions = vec![
            ("New tab", "Ctrl+Shift+T", Action::NewTab),
            (
                "Split side by side",
                "Ctrl+Shift+E",
                Action::Split(Axis::Columns),
            ),
            (
                "Split top and bottom",
                "Ctrl+Shift+O",
                Action::Split(Axis::Rows),
            ),
            ("Quick Settings", "Ctrl+Shift+S", Action::Settings),
            ("Keyboard shortcuts", "", Action::Shortcuts),
        ];
        if context {
            actions.splice(
                ..0,
                [
                    ("Copy", "Ctrl+Shift+C", Action::Copy),
                    ("Paste", "Ctrl+Shift+V", Action::Paste),
                ],
            );
        }
        actions
    }

    fn menu_lines(selected: usize, context: bool) -> Vec<(String, bool)> {
        Self::menu_actions(context)
            .into_iter()
            .enumerate()
            .map(|(index, (label, keys, _))| (format!(" {label:<21} {keys}"), index == selected))
            .collect()
    }

    fn shortcut_lines() -> Vec<(String, bool)> {
        Self::menu_actions(true)
            .into_iter()
            .map(|(label, keys, _)| (label, keys))
            .filter(|(_, keys)| !keys.is_empty())
            .chain([
                ("Action panel", "Ctrl+Shift+P"),
                ("Reload config", "Ctrl+Shift+R"),
                ("Close pane", "Ctrl+Shift+W"),
                ("Next / previous tab", "Ctrl+Tab / Ctrl+Shift+Tab"),
                ("Next / previous tab", "Ctrl+PageDown / Ctrl+PageUp"),
                ("Next / previous pane", "Alt+PageDown / Alt+PageUp"),
                ("Focus adjacent pane", "Alt+Arrow"),
                ("Resize pane", "Ctrl+Shift+Arrow"),
                ("Close shortcut list", "Esc"),
            ])
            .map(|(label, keys)| (format!(" {label:<21} {keys}"), false))
            .collect()
    }

    fn open_menu(&mut self, anchor: Option<[f64; 2]>) {
        self.selection_start = None;
        self.shortcuts_open = false;
        if self
            .menu_box(&Self::menu_lines(0, anchor.is_some()), anchor)
            .is_some()
        {
            self.menu = Some((0, anchor));
        }
        self.scheduler.invalidate();
    }

    fn menu_box(
        &self,
        lines: &[(String, bool)],
        anchor: Option<[f64; 2]>,
    ) -> Option<retro_renderer::scene::Rect> {
        let cell = [u32::from(self.cell.0), u32::from(self.cell.1)];
        let window = self.window_rect();
        let mut panel = ui::settings_box(window, lines, cell)?;
        if let Some([x, y]) = anchor {
            panel.x = (x as u32).min(window.width.saturating_sub(panel.width));
            panel.y = (y as u32).min(window.height.saturating_sub(panel.height));
        }
        Some(panel)
    }

    fn menu_click(&mut self) -> AppResult<bool> {
        let Some((selected, anchor)) = self.menu.take() else {
            return Ok(false);
        };
        self.scheduler.invalidate();
        if std::mem::take(&mut self.shortcuts_open) {
            return Ok(false);
        }
        let lines = Self::menu_lines(selected, anchor.is_some());
        if let Some(panel) = self.menu_box(&lines, anchor)
            && let Some([x, y]) = self.pointer_source()
            && x >= f64::from(panel.x)
            && x < f64::from(panel.x + panel.width)
            && y >= f64::from(panel.y + u32::from(self.cell.1))
            && let Some((_, _, action)) = Self::menu_actions(anchor.is_some())
                .into_iter()
                .nth(((y - f64::from(panel.y)) / f64::from(self.cell.1)) as usize - 1)
        {
            return self.action(action);
        }
        Ok(false)
    }

    fn press(&mut self) {
        if self.settings.is_some() {
            return;
        }
        self.selection_start = None;
        let Some([x, y]) = self.pointer_source() else {
            return;
        };
        if y < f64::from(self.tab_bar_height()) {
            let [width, _] = self.renderer.size();
            if let Some(index) = ui::tab_at(x, width, self.tabs.len(), u32::from(self.cell.0))
                && index != self.tabs.active_index()
            {
                self.tabs.select(index);
                self.activate();
            }
            return;
        }
        let Some(id) = self.pane_under_pointer() else {
            return;
        };
        if id != self.tabs.focused() {
            self.tabs.active_mut().layout.set_focus(id);
            self.activate();
        }
        self.selection_start = self.pointer_cell(id, false).map(|start| (id, start));
        if let Some(pane) = self.panes.get_mut(&id) {
            pane.session.clear_selection();
        }
        self.scheduler.invalidate();
    }

    fn drag(&mut self) {
        if self.settings.is_none()
            && let Some((id, start)) = self.selection_start
            && let Some(end) = self.pointer_cell(id, true)
            && let Some(pane) = self.panes.get_mut(&id)
        {
            pane.session.select(start, end);
            self.scheduler.invalidate();
        }
    }

    fn scroll(&mut self, delta: MouseScrollDelta) {
        let Some(pane) = self
            .pane_under_pointer()
            .and_then(|id| self.panes.get_mut(&id))
        else {
            return;
        };
        let lines = match delta {
            MouseScrollDelta::LineDelta(_, y) => f64::from(y) * 3.0,
            MouseScrollDelta::PixelDelta(point) => point.y / f64::from(pane.grid.cell_height),
        };
        pane.scroll_fraction += lines;
        let whole_lines = pane.scroll_fraction.trunc() as i32;
        pane.scroll_fraction -= f64::from(whole_lines);
        if whole_lines != 0 {
            pane.session.scroll(whole_lines);
            self.scheduler.invalidate();
        }
    }

    fn draw(&mut self, events: &ActiveEventLoop) -> AppResult<()> {
        if !self.scheduler.begin_frame() {
            return Ok(());
        }
        let now = Instant::now();
        let focused = self.tabs.focused();
        let Some(pane) = self.panes.get(&focused) else {
            return Ok(());
        };
        let snapshot = pane.session.snapshot();
        let [left, top] = pane.origin.unwrap_or_default();
        let cursor_position = snapshot.cursor.position;
        let viewport = self.renderer.size();
        let ime_cursor = cursor_position.map(|position| (snapshot.size, position, viewport));
        if let Some((grid, position, viewport)) = ime_cursor
            && ime_cursor != self.previous_ime_cursor
        {
            let mode = self.renderer.mode();
            let x = f64::from(left) + f64::from(position.column) * f64::from(grid.cell_width);
            let y = f64::from(top) + f64::from(position.row) * f64::from(grid.cell_height);
            let [left, top] = mode.display_point([x, y], viewport);
            let [right, bottom] = mode.display_point(
                [
                    x + f64::from(grid.cell_width),
                    y + f64::from(grid.cell_height),
                ],
                viewport,
            );
            self.window.set_ime_cursor_area(
                PhysicalPosition::new(left.floor() as u32, top.floor() as u32),
                PhysicalSize::new(
                    (right.ceil() - left.floor()) as u32,
                    (bottom.ceil() - top.floor()) as u32,
                ),
            );
        }
        self.previous_ime_cursor = ime_cursor;
        let blinking = snapshot.cursor.blinking
            && cursor_position.is_some()
            && snapshot.cursor.shape != CursorShape::Hidden
            && self.composition.preedit.is_empty()
            && self.settings.is_none();
        let visible = self.scheduler.cursor_visible(now, blinking);
        let scene = self.compose(focused, &snapshot, visible);
        self.renderer.set_animation_frame(self.animation_frame);
        match self
            .renderer
            .draw(&scene, || self.window.pre_present_notify())?
        {
            Frame::Presented => {
                let animated = matches!(self.renderer.mode(), RenderMode::CustomCrt(_, _))
                    && [
                        "burn_in",
                        "static_noise",
                        "jitter",
                        "glow_line",
                        "flicker",
                        "horizontal_sync",
                    ]
                    .into_iter()
                    .any(|key| {
                        self.config
                            .effects
                            .levels
                            .get(key)
                            .is_some_and(|&value| value > 0)
                    });
                self.scheduler.schedule_effect(now, animated);
                self.animation_frame = self.animation_frame.wrapping_add(1);
                self.gpu_recovered = false;
                if self.development {
                    eprintln!("frame presented");
                }
            }
            Frame::Retry => self.scheduler.retry(now),
            Frame::Occluded => self.scheduler.surface_occluded(),
            Frame::RecreateSurface => {
                self.renderer.recreate_surface(self.window.clone())?;
                self.scheduler.retry(now);
            }
            Frame::RecreateDevice if !self.gpu_recovered => {
                self.renderer =
                    create_renderer(events, &self.window, self.development, &self.config)?;
                self.gpu_recovered = true;
                self.resize(self.window.inner_size())?;
                self.scheduler.retry(now);
            }
            Frame::RecreateDevice => {
                return Err(io::Error::other("GPU device lost again during recovery").into());
            }
        }
        Ok(())
    }

    /// One scene for the active tab only: panes, dividers, tab bar. Hidden tabs cost nothing.
    fn compose(
        &self,
        focused: PaneId,
        snapshot: &retro_terminal::Snapshot,
        cursor_visible: bool,
    ) -> retro_renderer::scene::Scene {
        let mut scene = scene::empty(snapshot, self.font_size);
        let layout = &self.tabs.active().layout;
        for id in layout.panes() {
            let Some(pane) = self.panes.get(&id) else {
                continue;
            };
            let Some(origin) = pane.origin else { continue };
            if id == focused {
                scene::pane(
                    &mut scene,
                    snapshot,
                    origin,
                    true,
                    cursor_visible,
                    &self.composition.preedit,
                    if self.settings.is_some() {
                        ""
                    } else {
                        &self.banner
                    },
                );
            } else {
                scene::pane(
                    &mut scene,
                    &pane.session.snapshot(),
                    origin,
                    false,
                    true,
                    "",
                    "",
                );
            }
        }
        ui::dividers(&mut scene, &layout.dividers(self.area(), self.limits()));
        let bar = self.tab_bar_height();
        if bar > 0 {
            let labels: Vec<_> = self
                .tabs
                .iter()
                .enumerate()
                .map(|(index, tab)| {
                    let title = self
                        .panes
                        .get(&tab.layout.focus())
                        .and_then(|pane| pane.title.as_deref())
                        .unwrap_or(TITLE);
                    (
                        format!("{}: {title}", index + 1),
                        index == self.tabs.active_index(),
                    )
                })
                .collect();
            let [width, _] = self.renderer.size();
            ui::tab_bar(
                &mut scene,
                retro_renderer::scene::Rect {
                    x: 0,
                    y: 0,
                    width,
                    height: bar,
                },
                &labels,
                u32::from(self.cell.0),
            );
        }
        if let Some(settings) = &self.settings {
            let cell = [u32::from(self.cell.0), u32::from(self.cell.1)];
            ui::full_panel(&mut scene, self.window_rect(), &settings.lines(), cell, 0);
        }
        if let Some((selected, anchor)) = self.menu {
            let cell = [u32::from(self.cell.0), u32::from(self.cell.1)];
            if self.shortcuts_open {
                ui::full_panel(
                    &mut scene,
                    self.window_rect(),
                    &Self::shortcut_lines(),
                    cell,
                    selected,
                );
            } else {
                let lines = Self::menu_lines(selected, anchor.is_some());
                if let Some(panel) = self.menu_box(&lines, anchor) {
                    ui::settings_panel(&mut scene, panel, &lines, cell);
                }
            }
        }
        scene
    }

    fn window_event(&mut self, events: &ActiveEventLoop, event: WindowEvent) -> AppResult<bool> {
        match event {
            WindowEvent::Resized(size) => self.resize(size)?,
            WindowEvent::ScaleFactorChanged { .. } => self.resize(self.window.inner_size())?,
            WindowEvent::Occluded(occluded) => {
                self.occluded = occluded;
                self.resize(self.window.inner_size())?;
            }
            WindowEvent::Focused(false) => {
                self.modifiers = ModifiersState::empty();
                self.selection_start = None;
                self.composition.preedit.clear();
                self.scheduler.invalidate();
            }
            WindowEvent::Focused(true) => self.resize(self.window.inner_size())?,
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } if event.state == ElementState::Pressed && self.composition.preedit.is_empty() => {
                if !self.banner.is_empty() {
                    self.banner.clear();
                    self.scheduler.invalidate();
                }
                if let Some((selected, anchor)) = self.menu {
                    if matches!(event.logical_key.as_ref(), winit::keyboard::Key::Character(key) if key.eq_ignore_ascii_case("p"))
                        && self.modifiers.control_key()
                        && self.modifiers.shift_key()
                        && !self.modifiers.alt_key()
                    {
                        return self.action(Action::Panel);
                    }
                    if self.shortcuts_open {
                        use winit::keyboard::{Key, NamedKey};
                        let limit = ui::full_panel_scroll_limit(
                            self.window_rect(),
                            &Self::shortcut_lines(),
                            [u32::from(self.cell.0), u32::from(self.cell.1)],
                        );
                        let selected = selected.min(limit);
                        let offset = match event.logical_key.as_ref() {
                            Key::Named(NamedKey::Escape) => return self.action(Action::Panel),
                            Key::Named(NamedKey::ArrowUp) => selected.saturating_sub(1),
                            Key::Named(NamedKey::ArrowDown) => (selected + 1).min(limit),
                            _ => return Ok(false),
                        };
                        self.menu = Some((offset, anchor));
                        self.scheduler.invalidate();
                        return Ok(false);
                    }
                    let actions = Self::menu_actions(anchor.is_some());
                    let key = input::settings_key(event.logical_key.as_ref(), self.modifiers);
                    self.menu = match key {
                        Some(crate::settings::Key::Up) => {
                            Some(((selected + actions.len() - 1) % actions.len(), anchor))
                        }
                        Some(crate::settings::Key::Down) => {
                            Some(((selected + 1) % actions.len(), anchor))
                        }
                        Some(crate::settings::Key::Escape | crate::settings::Key::Enter) => None,
                        _ => self.menu,
                    };
                    self.scheduler.invalidate();
                    if key == Some(crate::settings::Key::Enter)
                        && let Some((_, _, action)) = actions.into_iter().nth(selected)
                    {
                        return self.action(action);
                    }
                    if matches!(event.logical_key.as_ref(), winit::keyboard::Key::Character(key) if key.eq_ignore_ascii_case("s"))
                        && key == Some(crate::settings::Key::Escape)
                    {
                        self.open_settings();
                    }
                    return Ok(false);
                }
                if let Some(settings) = &mut self.settings {
                    if let Some(key) =
                        input::settings_key(event.logical_key.as_ref(), self.modifiers)
                    {
                        let previous = settings.preview().clone();
                        let outcome = settings.key(key);
                        if settings.preview() != &previous {
                            self.settings_deadline = (settings.preview() != &self.config)
                                .then(|| Instant::now() + Duration::from_millis(150));
                        }
                        self.settings_outcome(outcome)?;
                    }
                    return Ok(false);
                }
                let modes = self
                    .focused()
                    .map(|pane| pane.session.keyboard_modes())
                    .unwrap_or_default();
                if let Some(action) = input::key(
                    event.logical_key.as_ref(),
                    event.text.as_deref(),
                    event.physical_key,
                    self.modifiers,
                    modes,
                ) {
                    return self.action(action);
                }
            }
            WindowEvent::Ime(event) => {
                let committed = self.composition.update(event);
                if let Some(text) =
                    committed.filter(|_| self.settings.is_none() && self.menu.is_none())
                {
                    self.send_input(Input::Text(&text))?;
                }
                self.scheduler.invalidate();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer = Some(position);
                self.drag();
            }
            WindowEvent::CursorLeft { .. } => self.pointer = None,
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                ..
            } => {
                if self.menu.is_some() {
                    return self.menu_click();
                }
                self.press();
            }
            WindowEvent::MouseInput {
                button: MouseButton::Right,
                state: ElementState::Pressed,
                ..
            } if self.settings.is_none() => {
                if let Some(id) = self.pane_under_pointer()
                    && id != self.tabs.focused()
                {
                    self.tabs.active_mut().layout.set_focus(id);
                    self.activate();
                }
                if let Some(anchor) = self.pointer_source() {
                    self.open_menu(Some(anchor));
                }
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: ElementState::Released,
                ..
            } => {
                self.selection_start = None;
            }
            WindowEvent::MouseWheel { delta, .. } if self.menu.is_none() => self.scroll(delta),
            WindowEvent::RedrawRequested => self.draw(events)?,
            _ => {}
        }
        Ok(false)
    }

    /// Polls every session, hidden ones too. Returns true when the last pane closed.
    fn poll_pane(&mut self, id: PaneId) -> AppResult<bool> {
        let focused = self.tabs.focused() == id;
        let visible = self.tabs.is_visible(id);
        let Some(pane) = self.panes.get_mut(&id) else {
            return Ok(false);
        };
        let update = pane.session.poll()?;
        if let Some(title) = update.title {
            pane.title = match title {
                TitleChange::Set(title) => Some(title),
                TitleChange::Reset => None,
            };
            if focused {
                self.window
                    .set_title(pane.title.as_deref().unwrap_or(TITLE));
            }
            if self.tabs.len() >= 2 {
                self.scheduler.invalidate();
            }
        }
        if update.view_changed && visible {
            self.scheduler.invalidate();
        }
        if update.exit_status.is_some() {
            return self.close_pane(id);
        }
        Ok(false)
    }
}

fn create_renderer(
    events: &ActiveEventLoop,
    window: &Arc<Window>,
    development: bool,
    config: &Config,
) -> AppResult<SurfaceRenderer> {
    let renderer = pollster::block_on(SurfaceRenderer::with_family(
        platform::graphics(events),
        window.clone(),
        render_mode(config),
        font_family(config),
    ))?;
    if development {
        let adapter = renderer.adapter_info();
        eprintln!(
            "Wayland GPU adapter: {} ({:?}, {:?}); {} {}",
            adapter.name, adapter.backend, adapter.device_type, adapter.driver, adapter.driver_info
        );
    }
    Ok(renderer)
}

fn render_mode(config: &Config) -> RenderMode {
    if config.effects.crt && !config.effects.levels.is_empty() {
        return RenderMode::CustomCrt(
            retro_renderer::CrtLevels(retro_renderer::CRT_CONTROLS.map(|(key, _, default)| {
                config.effects.levels.get(key).copied().unwrap_or(default)
            })),
            config.effects.animation,
        );
    }
    match (config.effects.crt, config.effects.animation) {
        (true, true) => RenderMode::AnimatedCrt,
        (true, false) => RenderMode::StaticCrt,
        (false, _) => RenderMode::Plain,
    }
}

fn font_family(config: &Config) -> Option<&str> {
    Some(config.font.family.as_str()).filter(|family| !family.is_empty())
}

fn font_error(config: &Config) -> ConfigError {
    ConfigError::Font {
        family: config.font.family.clone(),
    }
}

fn spawn(
    proxy: &EventLoopProxy<Event>,
    config: &Config,
    grid: GridSize,
    program: Option<Program>,
) -> AppResult<Session> {
    let wake = proxy.clone();
    Ok(Session::spawn(
        SessionOptions {
            size: grid,
            program,
            working_directory: Some(&config.shell.working_directory)
                .filter(|directory| !directory.is_empty())
                .map(PathBuf::from),
            scrollback_lines: config.terminal.scrollback_lines,
        },
        move |id| {
            let _ = wake.send_event(Event::Terminal(id));
        },
    )?)
}

fn program(config: &Config) -> Option<Program> {
    Some(&config.shell)
        .filter(|shell| !shell.program.is_empty())
        .map(|shell| Program {
            executable: shell.program.clone(),
            arguments: shell.args.clone(),
        })
}

/// The defaults with the command-line overrides, used when the file is invalid at startup.
fn fallback(overrides: &Overrides) -> AppResult<Config> {
    Ok(Config::resolve(config::Layer::default(), overrides)?)
}

fn config_message(source: &Source, error: &ConfigError) -> String {
    match source.path() {
        Some(path) => format!("config {}: {error}", path.display()),
        None => format!("config: {error}"),
    }
}

impl ApplicationHandler<Event> for App {
    fn resumed(&mut self, events: &ActiveEventLoop) {
        let result = match &mut self.state {
            Some(state) => state.resize(state.window.inner_size()),
            None => State::new(
                events,
                self.proxy.clone(),
                self.development,
                self.source.clone(),
                self.overrides.clone(),
            )
            .map(|state| self.state = Some(state)),
        };
        if let Err(error) = result {
            self.close(events, Some(error));
        }
    }

    fn suspended(&mut self, _: &ActiveEventLoop) {
        if let Some(state) = &mut self.state {
            state.scheduler.set_presentable(false);
        }
    }

    fn user_event(&mut self, events: &ActiveEventLoop, event: Event) {
        let Some(state) = &mut self.state else { return };
        let result = match event {
            Event::Terminal(id) => match state.sessions.get(&id) {
                Some(&pane) => state.poll_pane(pane),
                None => return,
            },
            Event::Paste(text) => {
                state.paste_pending = false;
                match text {
                    Ok(text) => state.send_input(Input::Paste(&text)).map(|()| false),
                    Err(error) => {
                        eprintln!("clipboard paste: {error}");
                        Ok(false)
                    }
                }
            }
        };
        match result {
            Ok(true) => self.close(events, None),
            Ok(false) => {}
            Err(error) => self.close(events, Some(error)),
        }
    }

    fn window_event(&mut self, events: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(state) = &mut self.state else { return };
        if state.window.id() != id {
            return;
        }
        if matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed) {
            self.close(events, None);
            return;
        }
        match state.window_event(events, event) {
            Ok(false) => {}
            Ok(true) => self.close(events, None),
            Err(error) => self.close(events, Some(error)),
        }
    }

    fn about_to_wait(&mut self, events: &ActiveEventLoop) {
        let Some(state) = &mut self.state else { return };
        let now = Instant::now();
        if let Err(error) = state.apply_pending_settings(now) {
            self.close(events, Some(error));
            return;
        }
        if state.scheduler.request_frame(now) {
            state.window.request_redraw();
        }
        events.set_control_flow(
            state
                .scheduler
                .deadline()
                .into_iter()
                .chain(state.settings_deadline)
                .min()
                .map_or(ControlFlow::Wait, ControlFlow::WaitUntil),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_mode_follows_crt_and_animation() {
        let mode = |crt, animation| {
            let mut config = Config::default();
            config.effects.crt = crt;
            config.effects.animation = animation;
            render_mode(&config)
        };
        assert_eq!(mode(false, false), RenderMode::Plain);
        assert_eq!(mode(false, true), RenderMode::Plain);
        assert_eq!(mode(true, false), RenderMode::StaticCrt);
        assert_eq!(mode(true, true), RenderMode::AnimatedCrt);
    }
}
