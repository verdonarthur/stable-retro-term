use std::error::Error;
use std::io;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use retro_renderer::{Frame, SurfaceRenderer};
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
    use winit::platform::wayland::EventLoopBuilderExtWayland;
    let events = EventLoop::<Event>::with_user_event()
        .with_wayland()
        .build()?;
    let mut app = App {
        proxy: events.create_proxy(),
        state: None,
        error: None,
        development,
        program,
    };
    events.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}

struct App {
    proxy: EventLoopProxy<Event>,
    state: Option<State>,
    error: Option<Box<dyn Error>>,
    development: bool,
    program: Option<Program>,
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
    previous_ime_cursor: Option<(GridSize, Position)>,
}

impl State {
    fn new(
        events: &ActiveEventLoop,
        proxy: EventLoopProxy<Event>,
        development: bool,
        program: Option<Program>,
    ) -> AppResult<Self> {
        let window = Arc::new(
            events.create_window(
                Window::default_attributes()
                    .with_title(TITLE)
                    .with_inner_size(INITIAL_SIZE)
                    .with_min_inner_size(LogicalSize::new(80, 48)),
            )?,
        );
        window.set_ime_allowed(true);
        let mut renderer = create_renderer(events, &window, development)?;
        let (font_size, line_height) = scene::font_metrics(window.scale_factor());
        let cell_width = renderer.cell_width(font_size, line_height);
        let fallback = INITIAL_SIZE.to_physical::<u32>(window.scale_factor());
        let grid = scene::grid_size(window.inner_size().into(), cell_width, line_height)
            .or_else(|| scene::grid_size(fallback.into(), cell_width, line_height))
            .ok_or_else(|| io::Error::other("cannot calculate the initial terminal grid"))?;
        let wake = proxy.clone();
        let session = Session::spawn(
            SessionOptions {
                size: grid,
                program,
                working_directory: None,
                scrollback_lines: 10_000,
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
            previous_ime_cursor: None,
        };
        state.resize(state.window.inner_size())?;
        Ok(state)
    }

    fn resize(&mut self, size: PhysicalSize<u32>) -> AppResult<()> {
        let (font_size, line_height) = scene::font_metrics(self.window.scale_factor());
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
        }
        Ok(())
    }

    fn pointer_cell(&self, clamp: bool) -> Option<Position> {
        let point = self.pointer?;
        scene::cell_at(point.x, point.y, self.grid, clamp)
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
        let ime_cursor = cursor_position.map(|position| (snapshot.size, position));
        if let Some((grid, position)) = ime_cursor
            && ime_cursor != self.previous_ime_cursor
        {
            self.window.set_ime_cursor_area(
                PhysicalPosition::new(
                    u32::from(position.column) * u32::from(grid.cell_width),
                    u32::from(position.row) * u32::from(grid.cell_height),
                ),
                PhysicalSize::new(u32::from(grid.cell_width), u32::from(grid.cell_height)),
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
                self.renderer = create_renderer(events, &self.window, self.development)?;
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
) -> AppResult<SurfaceRenderer> {
    let renderer = pollster::block_on(SurfaceRenderer::new(
        platform::graphics(events),
        window.clone(),
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

impl ApplicationHandler<Event> for App {
    fn resumed(&mut self, events: &ActiveEventLoop) {
        let result = match &mut self.state {
            Some(state) => state.resize(state.window.inner_size()),
            None => State::new(
                events,
                self.proxy.clone(),
                self.development,
                self.program.take(),
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
