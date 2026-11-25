use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fmt;
use std::io;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSendError, EventLoopSender, Msg, State};
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::index::Side;
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::tty;

use crate::engine::{self, TermSize};
use crate::input::{self, Input};
use crate::snapshot::{GridSize, Position, Snapshot};

const MAX_PENDING_REPLIES: usize = 128;
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Unique for the lifetime of the application process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SessionId(u64);

#[derive(Clone, Debug)]
pub struct Program {
    pub executable: String,
    pub arguments: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct SessionOptions {
    pub size: GridSize,
    /// Use the user's shell when no program is supplied.
    pub program: Option<Program>,
    pub working_directory: Option<PathBuf>,
    pub scrollback_lines: usize,
}

#[derive(Debug)]
pub enum SessionError {
    InvalidSize,
    InvalidScrollback,
    Startup(io::Error),
    WorkerIo(io::Error),
    WorkerStopped,
    WorkerPanicked,
    Closed,
    ReplyOverflow,
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSize => f.write_str("invalid terminal dimensions"),
            Self::InvalidScrollback => f.write_str("scrollback limit exceeds terminal coordinates"),
            Self::Startup(error) => write!(f, "cannot start terminal session: {error}"),
            Self::WorkerIo(error) => write!(f, "cannot wake terminal worker: {error}"),
            Self::WorkerStopped => f.write_str("terminal worker stopped unexpectedly"),
            Self::WorkerPanicked => f.write_str("terminal worker panicked"),
            Self::Closed => f.write_str("terminal session is closed"),
            Self::ReplyOverflow => f.write_str("too many terminal protocol replies are pending"),
        }
    }
}

impl Error for SessionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Startup(error) | Self::WorkerIo(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TitleChange {
    Set(String),
    Reset,
}

/// Changes since the previous poll. Repeated redraws, bells, and title changes coalesce.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SessionUpdate {
    pub view_changed: bool,
    pub bell: bool,
    pub title: Option<TitleChange>,
    /// Present once, after the worker has drained the child's final output.
    pub exit_status: Option<ExitStatus>,
}

type Worker = EventLoop<tty::Pty, Listener>;

/// Owns a Linux PTY worker. Snapshots remain available after close or child exit.
///
/// Close and drop can block while Alacritty waits for a child that has not exited.
/// Upstream cleanup sends hangup to a numeric PID, which can have been reused if
/// the child has already been reaped. Worker I/O failures that upstream only logs
/// can be detected on a later poll once the worker finishes.
pub struct Session {
    id: SessionId,
    size: GridSize,
    terminal: Arc<FairMutex<Term<Listener>>>,
    listener: Listener,
    sender: EventLoopSender,
    worker: Option<JoinHandle<(Worker, State)>>,
}

impl Session {
    /// Start a PTY session. The callback may run on the worker or caller thread.
    ///
    /// The callback should only schedule a poll, return promptly, and never call
    /// back into the session. Keep polling hidden sessions as well as visible ones.
    pub fn spawn(
        options: SessionOptions,
        wake: impl Fn(SessionId) + Send + Sync + 'static,
    ) -> Result<Self, SessionError> {
        if !options.size.is_valid() {
            return Err(SessionError::InvalidSize);
        }
        if options.scrollback_lines > i32::MAX as usize - usize::from(u16::MAX) {
            return Err(SessionError::InvalidScrollback);
        }

        let id = SessionId(NEXT_ID.fetch_add(1, Ordering::Relaxed));
        let listener = Listener {
            shared: Arc::new(Notifications {
                id,
                wake: Box::new(wake),
                pending: Mutex::default(),
            }),
        };
        let terminal = Arc::new(FairMutex::new(engine::terminal(
            options.size,
            options.scrollback_lines,
            listener.clone(),
        )));
        let tty_options = tty::Options {
            shell: options
                .program
                .map(|program| tty::Shell::new(program.executable, program.arguments)),
            working_directory: options.working_directory,
            drain_on_exit: true,
            env: HashMap::from([
                ("TERM".into(), "xterm-256color".into()),
                ("COLORTERM".into(), "truecolor".into()),
            ]),
        };
        let pty =
            tty::new(&tty_options, options.size.window_size(), 0).map_err(SessionError::Startup)?;
        let event_loop = EventLoop::new(
            Arc::clone(&terminal),
            listener.clone(),
            pty,
            tty_options.drain_on_exit,
            false,
        )
        .map_err(SessionError::Startup)?;
        let sender = event_loop.channel();
        let worker = event_loop.spawn();
        Ok(Self {
            id,
            size: options.size,
            terminal,
            listener,
            sender,
            worker: Some(worker),
        })
    }

    pub fn id(&self) -> SessionId {
        self.id
    }

    /// Copy the visible grid and release the terminal lock before returning.
    pub fn snapshot(&self) -> Snapshot {
        engine::snapshot(&self.terminal.lock(), self.size)
    }

    /// Drain queued replies and updates, and rearm the session's wakeup.
    ///
    /// A reply overflow ends the session's reply stream and is reported on polls
    /// until the owner closes the session.
    pub fn poll(&mut self) -> Result<SessionUpdate, SessionError> {
        let worker_finished = self.worker.as_ref().is_some_and(JoinHandle::is_finished);
        let (mut update, replies, overflow, exited) = {
            let mut pending = self
                .listener
                .shared
                .pending
                .lock()
                .expect("notification lock");
            pending.notified = false;
            let mut update = std::mem::take(&mut pending.update);
            if pending.exited {
                update.exit_status = pending.exit_status.take();
            }
            (
                update,
                std::mem::take(&mut pending.replies),
                pending.overflow,
                pending.exited,
            )
        };
        if exited {
            self.join_worker()?;
            update.view_changed |= update.exit_status.is_some();
        } else if worker_finished {
            self.join_worker()?;
            return Err(SessionError::WorkerStopped);
        }
        if overflow && self.worker.is_some() {
            return Err(SessionError::ReplyOverflow);
        }
        if self.worker.is_some() && !replies.is_empty() {
            let colors = *self.terminal.lock().colors();
            for reply in replies {
                let text = match reply {
                    Event::PtyWrite(text) => text,
                    Event::ColorRequest(index, format) => {
                        format(engine::palette_color(&colors, index))
                    }
                    Event::TextAreaSizeRequest(format) => format(self.size.window_size()),
                    _ => unreachable!("only PTY replies are queued"),
                };
                self.send(Msg::Input(text.into_bytes().into()))?;
            }
        }
        Ok(update)
    }

    pub fn input(&mut self, input: Input<'_>) -> Result<(), SessionError> {
        let bracketed = self
            .terminal
            .lock()
            .mode()
            .contains(TermMode::BRACKETED_PASTE);
        self.send(Msg::Input(input::encode(input, bracketed).into()))
    }

    /// Queue a PTY resize and update the terminal grid using the same dimensions.
    pub fn resize(&mut self, size: GridSize) -> Result<(), SessionError> {
        if !size.is_valid() {
            return Err(SessionError::InvalidSize);
        }
        {
            let mut terminal = self.terminal.lock();
            self.send(Msg::Resize(size.window_size()))?;
            terminal.resize(TermSize(size));
        }
        self.size = size;
        self.listener
            .update(|pending| pending.update.view_changed = true);
        Ok(())
    }

    /// Scroll the viewport by lines. Positive values move back into history.
    pub fn scroll(&mut self, lines: i32) {
        let mut terminal = self.terminal.lock();
        let offset = terminal.grid().display_offset() as i32;
        terminal.scroll_display(Scroll::Delta(lines.min(i32::MAX - offset)));
    }

    /// Select an inclusive range of visible cells. Returns false for an invalid endpoint.
    pub fn select(&mut self, start: Position, end: Position) -> bool {
        let in_view =
            |point: Position| point.row < self.size.rows && point.column < self.size.columns;
        if !in_view(start) || !in_view(end) {
            return false;
        }
        {
            let mut terminal = self.terminal.lock();
            let offset = terminal.grid().display_offset();
            let mut selection = Selection::new(
                SelectionType::Simple,
                engine::terminal_point(start, offset),
                Side::Left,
            );
            selection.update(engine::terminal_point(end, offset), Side::Right);
            selection.include_all();
            terminal.selection = Some(selection);
        }
        self.listener
            .update(|pending| pending.update.view_changed = true);
        true
    }

    pub fn clear_selection(&mut self) {
        if self.terminal.lock().selection.take().is_some() {
            self.listener
                .update(|pending| pending.update.view_changed = true);
        }
    }

    pub fn selected_text(&self) -> Option<String> {
        self.terminal.lock().selection_to_string()
    }

    /// Stop the worker and let Alacritty hang up and reap its child.
    ///
    /// This may block. Call it outside any UI or terminal locks.
    pub fn close(&mut self) -> Result<(), SessionError> {
        let Some(worker) = &self.worker else {
            return Ok(());
        };
        let shutdown = if worker.is_finished() {
            Ok(())
        } else {
            self.sender.send(Msg::Shutdown)
        };
        self.join_worker()?;
        match shutdown {
            Ok(()) | Err(EventLoopSendError::Send(_)) => Ok(()),
            Err(EventLoopSendError::Io(error)) => Err(SessionError::WorkerIo(error)),
        }
    }

    fn send(&self, message: Msg) -> Result<(), SessionError> {
        if self.worker.as_ref().is_none_or(JoinHandle::is_finished) {
            return Err(SessionError::Closed);
        }
        if matches!(&message, Msg::Input(bytes) if bytes.is_empty()) {
            return Ok(());
        }
        self.sender.send(message).map_err(|error| match error {
            EventLoopSendError::Send(_) => SessionError::WorkerStopped,
            EventLoopSendError::Io(error) => SessionError::WorkerIo(error),
        })
    }

    fn join_worker(&mut self) -> Result<(), SessionError> {
        if let Some(worker) = self.worker.take() {
            let (event_loop, _) = worker.join().map_err(|_| SessionError::WorkerPanicked)?;
            drop(event_loop);
        }
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[derive(Default)]
struct Pending {
    update: SessionUpdate,
    notified: bool,
    exit_status: Option<ExitStatus>,
    exited: bool,
    replies: VecDeque<Event>,
    overflow: bool,
}

struct Notifications {
    id: SessionId,
    wake: Box<dyn Fn(SessionId) + Send + Sync>,
    pending: Mutex<Pending>,
}

#[derive(Clone)]
struct Listener {
    shared: Arc<Notifications>,
}

impl Listener {
    fn update(&self, update: impl FnOnce(&mut Pending)) {
        let wake = {
            let mut pending = self.shared.pending.lock().expect("notification lock");
            update(&mut pending);
            !std::mem::replace(&mut pending.notified, true)
        };
        if wake {
            (self.shared.wake)(self.shared.id);
        }
    }
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        match event {
            Event::ChildExit(status) => {
                self.shared
                    .pending
                    .lock()
                    .expect("notification lock")
                    .exit_status = Some(status);
            }
            Event::Exit => self.update(|pending| pending.exited = true),
            Event::Wakeup | Event::CursorBlinkingChange | Event::MouseCursorDirty => {
                self.update(|pending| pending.update.view_changed = true);
            }
            Event::Title(title) => {
                self.update(|pending| pending.update.title = Some(TitleChange::Set(title)));
            }
            Event::ResetTitle => {
                self.update(|pending| pending.update.title = Some(TitleChange::Reset));
            }
            Event::Bell => self.update(|pending| pending.update.bell = true),
            Event::PtyWrite(_) | Event::ColorRequest(..) | Event::TextAreaSizeRequest(_) => {
                self.update(|pending| {
                    if pending.replies.len() == MAX_PENDING_REPLIES {
                        pending.overflow = true;
                        pending.replies.clear();
                    }
                    if !pending.overflow {
                        pending.replies.push_back(event);
                    }
                });
            }
            Event::ClipboardLoad(..) | Event::ClipboardStore(..) => {}
        }
    }
}
