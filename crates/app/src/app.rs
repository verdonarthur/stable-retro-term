use std::error::Error;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

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
use crate::platform;
use crate::scene;
use crate::scheduler::Scheduler;

const TITLE: &str = "Retro Terminal";
const INITIAL_SIZE: LogicalSize<u32> = LogicalSize::new(800, 500);
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
            .and_then(|mut state| state.session.close().err());
        self.error = error.or_else(|| close_error.map(Into::into));
        events.exit();
    }
}

struct State {
    renderer: SurfaceRenderer,
    window: Arc<Window>,
    session: Session,
    proxy: EventLoopProxy<Event>,
    scheduler: Scheduler,
    modifiers: ModifiersState,
    pointer: Option<PhysicalPosition<f64>>,
    selection_start: Option<Position>,
    composition: Composition,
    grid: GridSize,
    font_size: u16,
    occluded: bool,
    paste_pending: bool,
    scroll_fraction: f64,
    gpu_recovered: bool,
    development: bool,
    config: Config,
    source: Source,
    overrides: Overrides,
    banner: String,
    previous_ime_cursor: Option<(GridSize, Position, [u32; 2])>,
}

impl State {
    fn new(
        events: &ActiveEventLoop,
        proxy: EventLoopProxy<Event>,
        development: bool,
        source: Source,
        overrides: Overrides,
    ) -> AppResult<Self> {
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
        let wake = proxy.clone();
        let session = Session::spawn(
            SessionOptions {
                size: grid,
                program: program(&config),
                working_directory: Some(&config.shell.working_directory)
                    .filter(|directory| !directory.is_empty())
                    .map(PathBuf::from),
                scrollback_lines: config.terminal.scrollback_lines,
            },
            move |id| {
                let _ = wake.send_event(Event::Terminal(id));
            },
        )?;
        let mut state = Self {
            renderer,
            window,
            session,
            proxy,
            scheduler: Scheduler::default(),
            modifiers: ModifiersState::empty(),
            pointer: None,
            selection_start: None,
            composition: Composition::default(),
            grid,
            font_size,
            occluded: false,
            paste_pending: false,
            scroll_fraction: 0.0,
            gpu_recovered: false,
            development,
            config,
            source,
            overrides,
            banner: String::new(),
            previous_ime_cursor: None,
        };
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
        let cell_width = if font_size == self.font_size && line_height == self.grid.cell_height {
            self.grid.cell_width
        } else {
            self.renderer.cell_width(font_size, line_height)
        };
        let grid = scene::grid_size(size.into(), cell_width, line_height);
        self.scheduler
            .set_presentable(grid.is_some() && !self.occluded);
        let Some(grid) = grid else {
            self.renderer.resize([0, 0])?;
            return Ok(());
        };
        self.renderer.resize(size.into())?;
        if grid != self.grid {
            match self.session.resize(grid) {
                Ok(()) => {
                    self.grid = grid;
                    self.selection_start = None;
                }
                Err(SessionError::Closed) => return Ok(()),
                Err(error) => return Err(error.into()),
            }
        }
        self.font_size = font_size;
        Ok(())
    }

    fn show(&mut self, message: String) {
        eprintln!("retro-term: {message}");
        self.banner = message;
        self.scheduler.invalidate();
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
                "configuration reloaded; shell and scrollback settings apply to new shells".into(),
            );
        }
        Ok(())
    }

    fn send_input(&mut self, input: Input<'_>) -> AppResult<()> {
        match self.session.input(input) {
            Ok(()) => {
                if self.session.scroll_to_bottom() {
                    self.scheduler.invalidate();
                }
                Ok(())
            }
            Err(SessionError::Closed) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn action(&mut self, action: Action) -> AppResult<()> {
        match action {
            Action::Bytes(bytes) => self.send_input(Input::Bytes(&bytes))?,
            Action::Copy => {
                if let Some(text) = self.session.selected_text()
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
        }
        Ok(())
    }

    fn pointer_cell(&self, clamp: bool) -> Option<Position> {
        let point = self.pointer?;
        scene::pointer_cell(
            [point.x, point.y],
            self.renderer.size(),
            self.renderer.mode(),
            self.grid,
            clamp,
        )
    }

    fn drag(&mut self) {
        if let (Some(start), Some(end)) = (self.selection_start, self.pointer_cell(true)) {
            self.session.select(start, end);
            self.scheduler.invalidate();
        }
    }

    fn scroll(&mut self, delta: MouseScrollDelta) {
        let lines = match delta {
            MouseScrollDelta::LineDelta(_, y) => f64::from(y) * 3.0,
            MouseScrollDelta::PixelDelta(point) => point.y / f64::from(self.grid.cell_height),
        };
        self.scroll_fraction += lines;
        let whole_lines = self.scroll_fraction.trunc() as i32;
        self.scroll_fraction -= f64::from(whole_lines);
        if whole_lines != 0 {
            self.session.scroll(whole_lines);
            self.scheduler.invalidate();
        }
    }

    fn draw(&mut self, events: &ActiveEventLoop) -> AppResult<()> {
        if !self.scheduler.begin_frame() {
            return Ok(());
        }
        let now = Instant::now();
        let snapshot = self.session.snapshot();
        let cursor_position = snapshot.cursor.position;
        let viewport = self.renderer.size();
        let ime_cursor = cursor_position.map(|position| (snapshot.size, position, viewport));
        if let Some((grid, position, viewport)) = ime_cursor
            && ime_cursor != self.previous_ime_cursor
        {
            let mode = self.renderer.mode();
            let x = f64::from(position.column) * f64::from(grid.cell_width);
            let y = f64::from(position.row) * f64::from(grid.cell_height);
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
            && self.composition.preedit.is_empty();
        let visible = self.scheduler.cursor_visible(now, blinking);
        let scene = scene::convert(
            &snapshot,
            self.font_size,
            visible,
            &self.composition.preedit,
            &self.banner,
        );
        match self
            .renderer
            .draw(&scene, || self.window.pre_present_notify())?
        {
            Frame::Presented => {
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

    fn window_event(&mut self, events: &ActiveEventLoop, event: WindowEvent) -> AppResult<()> {
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
                if let Some(action) = input::key(
                    event.logical_key.as_ref(),
                    event.text.as_deref(),
                    event.physical_key,
                    self.modifiers,
                    self.session.keyboard_modes(),
                ) {
                    self.action(action)?;
                }
            }
            WindowEvent::Ime(event) => {
                if let Some(text) = self.composition.update(event) {
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
                self.selection_start = self.pointer_cell(false);
                self.session.clear_selection();
                self.scheduler.invalidate();
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: ElementState::Released,
                ..
            } => {
                self.selection_start = None;
            }
            WindowEvent::MouseWheel { delta, .. } => self.scroll(delta),
            WindowEvent::RedrawRequested => self.draw(events)?,
            _ => {}
        }
        Ok(())
    }

    fn poll_terminal(&mut self) -> AppResult<bool> {
        let update = self.session.poll()?;
        if let Some(title) = update.title {
            match title {
                TitleChange::Set(title) => self.window.set_title(&title),
                TitleChange::Reset => self.window.set_title(TITLE),
            }
        }
        if update.view_changed {
            self.scheduler.invalidate();
        }
        Ok(update.exit_status.is_some())
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
    if config.effects.crt {
        RenderMode::StaticCrt
    } else {
        RenderMode::Plain
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
            Event::Terminal(id) if state.session.id() == id => state.poll_terminal(),
            Event::Terminal(_) => return,
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
        } else if let Err(error) = state.window_event(events, event) {
            self.close(events, Some(error));
        }
    }

    fn about_to_wait(&mut self, events: &ActiveEventLoop) {
        let Some(state) = &mut self.state else { return };
        if state.scheduler.request_frame(Instant::now()) {
            state.window.request_redraw();
        }
        events.set_control_flow(
            state
                .scheduler
                .deadline()
                .map_or(ControlFlow::Wait, ControlFlow::WaitUntil),
        );
    }
}
