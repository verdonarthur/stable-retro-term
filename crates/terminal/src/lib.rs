//! Linux PTY sessions and owned snapshots of the visible terminal grid.
//!
//! The session owner should call [`Session::poll`] after each wakeup, including for
//! hidden sessions. Polling forwards protocol replies and reports view changes.
//! Call [`Session::snapshot`] when a view needs drawing.

#![cfg(target_os = "linux")]

mod engine;
mod input;
mod session;
mod snapshot;

pub use input::Input;
pub use session::{
    Program, Session, SessionError, SessionId, SessionOptions, SessionUpdate, TitleChange,
};
pub use snapshot::{
    Cell, CellAttributes, CellWidth, Cursor, CursorShape, GridSize, Position, Rgb, Snapshot,
    Underline,
};
