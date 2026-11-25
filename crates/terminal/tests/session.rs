#![cfg(target_os = "linux")]

mod common;

use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use retro_terminal::{GridSize, Input, Session, SessionError, TitleChange};

fn has_text(session: &Session, text: &str) -> bool {
    let cells: String = session
        .snapshot()
        .cells
        .iter()
        .map(|cell| cell.character)
        .collect();
    cells.contains(text)
}

fn wait_for_text_without_polling(session: &Session, text: &str) {
    let deadline = Instant::now() + common::TIMEOUT;
    while !has_text(session, text) {
        assert!(Instant::now() < deadline, "PTY did not print {text}");
        thread::yield_now();
    }
}

#[test]
fn startup_failure_and_invalid_dimensions_are_reported() {
    let mut options = common::options("", &[]);
    options.program.as_mut().unwrap().executable = "/missing/retro-term-test-shell".into();
    assert!(matches!(
        Session::spawn(options, |_| {}),
        Err(SessionError::Startup(_))
    ));

    let mut options = common::options("", &[]);
    options.size.columns = 1;
    assert!(matches!(
        Session::spawn(options, |_| {}),
        Err(SessionError::InvalidSize)
    ));
}

#[test]
fn input_resize_exit_status_and_final_output_round_trip() {
    let (mut session, wake) = common::spawn(common::options(
        r#"
            stty -echo
            printf 'READY\n'
            IFS= read -r -t 5 line || exit 1
            printf 'INPUT:%s\n' "$line"
            printf 'SIZE:'
            stty size
            printf 'FINAL'
            exit 19
        "#,
        &[],
    ));

    common::wait_for(&mut session, &wake, |session, _| {
        has_text(session, "READY").then_some(())
    });
    let original = session.snapshot().size;
    let invalid = GridSize {
        cell_width: u16::MAX,
        ..original
    };
    assert!(matches!(
        session.resize(invalid),
        Err(SessionError::InvalidSize)
    ));
    assert_eq!(session.snapshot().size, original);
    let size = GridSize {
        rows: 12,
        columns: 32,
        ..original
    };
    session.resize(size).unwrap();
    session.input(Input::Bytes(b"")).unwrap();
    session.input(Input::Text("hello")).unwrap();
    session.input(Input::Bytes(b"\r")).unwrap();

    let exit = common::wait_for(&mut session, &wake, |_, update| {
        if update.exit_status.is_some() {
            assert!(update.view_changed);
        }
        update.exit_status
    });
    assert_eq!(exit.code(), Some(19));
    assert_eq!(session.snapshot().size, size);
    assert!(has_text(&session, "INPUT:hello"));
    assert!(has_text(&session, "SIZE:12 32"));
    assert!(has_text(&session, "FINAL"));
    assert!(matches!(
        session.input(Input::Text("ignored")),
        Err(SessionError::Closed)
    ));
    session.close().unwrap();
}

#[test]
fn cursor_size_and_color_queries_receive_exact_replies() {
    let (mut session, wake) = common::spawn(common::options(
        r#"
            export LC_ALL=C
            stty raw -echo
            printf '\033[2;4H\033[6n\033[14t\033]10;#112233\007\033]10;?\007'
            expected=$'\033[2;4R\033[4;384;640t\033]10;rgb:1111/2222/3333\007'
            IFS= read -r -N "${#expected}" -t 5 actual || exit 1
            [[ $actual == "$expected" ]] || exit 2
            printf 'REPLIES OK'
        "#,
        &[],
    ));
    let status = common::wait_for(&mut session, &wake, |_, update| update.exit_status);
    assert_eq!(status.code(), Some(0));
    assert!(has_text(&session, "REPLIES OK"));
}

#[test]
fn paste_mode_changes_the_bytes_written_to_the_pty() {
    let (mut session, wake) = common::spawn(common::options(
        r#"
            export LC_ALL=C
            stty raw -echo
            printf '\033[?2004hBRACKETED'
            expected=$'\033[200~a\nb\033[201~'
            IFS= read -r -N "${#expected}" -t 5 actual || exit 1
            [[ $actual == "$expected" ]] || exit 2
            printf '\033[?2004lPLAIN'
            actual=$(timeout --foreground 5 dd bs=3 count=1 iflag=fullblock status=none) || exit 3
            [[ $actual == $'c\rd' ]] || exit 4
        "#,
        &[],
    ));
    common::wait_for(&mut session, &wake, |session, _| {
        (session.snapshot().bracketed_paste && has_text(session, "BRACKETED")).then_some(())
    });
    session.input(Input::Paste("a\nb")).unwrap();
    common::wait_for(&mut session, &wake, |session, _| {
        (!session.snapshot().bracketed_paste && has_text(session, "PLAIN")).then_some(())
    });
    session.input(Input::Paste("c\nd")).unwrap();
    let status = common::wait_for(&mut session, &wake, |_, update| update.exit_status);
    assert_eq!(status.code(), Some(0));
}

#[test]
fn wakeups_rearm_and_sessions_have_distinct_ids() {
    let script = r#"
        stty -echo
        printf '\033]2;first\007\007READY'
        IFS= read -r -t 5 || exit 1
        printf '\033]2;second\007\007AGAIN'
        IFS= read -r -t 5
    "#;
    let (mut session, wake) = common::spawn(common::options(script, &[]));
    let (mut other, _) = common::spawn(common::options("exit", &[]));
    assert_ne!(session.id(), other.id());
    other.close().unwrap();

    assert_eq!(wake.recv_timeout(common::TIMEOUT).unwrap(), session.id());
    wait_for_text_without_polling(&session, "READY");
    assert!(wake.try_recv().is_err());
    let update = session.poll().unwrap();
    assert_eq!(update.title, Some(TitleChange::Set("first".into())));
    assert!(update.bell);

    session.input(Input::Text("\r")).unwrap();
    common::wait_for(&mut session, &wake, |_, update| {
        (update.title == Some(TitleChange::Set("second".into()))).then_some(())
    });
    session.close().unwrap();
}

#[test]
fn protocol_reply_overflow_is_reported() {
    let (mut session, wake) = common::spawn(common::options(
        r#"
            stty raw -echo
            for (( i = 0; i < 129; i++ )); do printf '\033[5n'; done
            printf 'READY'
            IFS= read -r -t 5
        "#,
        &[],
    ));
    assert_eq!(wake.recv_timeout(common::TIMEOUT).unwrap(), session.id());
    wait_for_text_without_polling(&session, "READY");
    assert!(matches!(session.poll(), Err(SessionError::ReplyOverflow)));
    session.close().unwrap();
}

#[test]
fn close_and_drop_reap_a_child_that_responds_to_hangup() {
    for explicit in [true, false] {
        let (mut session, wake) = common::spawn(common::options(
            r#"
                trap 'exit 0' HUP
                printf '%s READY' "$$"
                IFS= read -r -t 5
            "#,
            &[],
        ));
        let pid = common::wait_for(&mut session, &wake, |session, _| {
            let text: String = session
                .snapshot()
                .cells
                .iter()
                .map(|cell| cell.character)
                .collect();
            let (pid, _) = text.split_once(" READY")?;
            Some(pid.parse::<u32>().unwrap())
        });

        let (finished, completion) = mpsc::channel();
        let shutdown = thread::spawn(move || {
            if explicit {
                session.close().unwrap();
                session.close().unwrap();
            }
            drop(session);
            finished.send(()).unwrap();
        });
        completion
            .recv_timeout(common::TIMEOUT)
            .expect("session cleanup did not finish");
        shutdown.join().unwrap();
        assert!(!Path::new("/proc").join(pid.to_string()).exists());
    }
}
