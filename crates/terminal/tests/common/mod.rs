use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use retro_terminal::{GridSize, Program, Session, SessionId, SessionOptions, SessionUpdate};

pub const TIMEOUT: Duration = Duration::from_secs(8);

pub fn options(script: &str, arguments: &[&str]) -> SessionOptions {
    let mut args = vec!["-c".into(), script.into(), "terminal-test".into()];
    args.extend(arguments.iter().map(|argument| (*argument).into()));
    SessionOptions {
        size: GridSize {
            rows: 24,
            columns: 80,
            cell_width: 8,
            cell_height: 16,
        },
        program: Some(Program {
            executable: "/bin/bash".into(),
            arguments: args,
        }),
        working_directory: None,
        scrollback_lines: 20,
    }
}

pub fn spawn(options: SessionOptions) -> (Session, Receiver<SessionId>) {
    let (wake, receiver) = mpsc::channel();
    let session = Session::spawn(options, move |id| {
        let _ = wake.send(id);
    })
    .expect("spawn test PTY");
    (session, receiver)
}

pub fn wait_for<T>(
    session: &mut Session,
    wake: &Receiver<SessionId>,
    mut inspect: impl FnMut(&mut Session, SessionUpdate) -> Option<T>,
) -> T {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let update = session.poll().expect("poll test PTY");
        let exit = update.exit_status;
        if let Some(result) = inspect(session, update) {
            return result;
        }
        assert!(
            exit.is_none(),
            "child exited before the expected update: {exit:?}"
        );
        let id = wake
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("timed out waiting for PTY");
        assert_eq!(id, session.id());
    }
}
