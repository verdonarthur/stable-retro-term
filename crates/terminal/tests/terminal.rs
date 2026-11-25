#![cfg(target_os = "linux")]

mod common;

use retro_terminal::{CellWidth, CursorShape, GridSize, Input, Position, Rgb, Underline};

#[test]
fn snapshots_preserve_cells_colors_and_cursor() {
    const OUTPUT: &str = "\x1b]4;1;#123456\x07\x1b[2;3H\x1b[1;4;31;48;2;1;2;3m界e\u{301}\x1b[5 q";
    let (mut session, wake) = common::spawn(common::options(r#"printf '%s' "$1""#, &[OUTPUT]));
    common::wait_for(&mut session, &wake, |_, update| update.exit_status);

    let view = session.snapshot();
    let cell = &view.cells[82];
    assert_eq!(cell.position, Position { row: 1, column: 2 });
    assert_eq!(cell.character, '界');
    assert_eq!(cell.width, CellWidth::Wide);
    assert_eq!(cell.foreground, Rgb(0x12, 0x34, 0x56));
    assert_eq!(cell.background, Rgb(1, 2, 3));
    assert!(cell.attributes.bold);
    assert_eq!(cell.attributes.underline, Some(Underline::Single));
    assert_eq!(view.cells[83].width, CellWidth::WideSpacer);
    assert_eq!(view.cells[84].character, 'e');
    assert_eq!(&*view.cells[84].combining, &['\u{301}']);
    assert_eq!(view.cursor.position, Some(Position { row: 1, column: 5 }));
    assert_eq!(view.cursor.shape, CursorShape::Beam);
    assert!(view.cursor.blinking);
}

#[test]
fn leaving_alternate_screen_restores_the_main_screen() {
    const ALTERNATE: &str = "main\x1b[?1049h\x1b[Hother\x1b[?25l";
    let (mut session, wake) = common::spawn(common::options(
        r#"
            stty -echo
            printf '%s' "$1"
            IFS= read -r -t 5 || exit 1
            printf '\033[?1049l'
        "#,
        &[ALTERNATE],
    ));
    common::wait_for(&mut session, &wake, |session, _| {
        let view = session.snapshot();
        (view.alternate_screen
            && view.cursor.shape == CursorShape::Hidden
            && view.cells[0].character == 'o')
            .then_some(())
    });
    session.input(Input::Text("\r")).unwrap();
    let status = common::wait_for(&mut session, &wake, |_, update| update.exit_status);
    assert!(status.success());
    let view = session.snapshot();
    assert!(!view.alternate_screen);
    let text: String = view.cells[..4].iter().map(|cell| cell.character).collect();
    assert_eq!(text, "main");
}

#[test]
fn scrollback_is_bounded_and_selection_uses_the_visible_rows() {
    let mut options = common::options("printf 'AAA\\nBBB\\nCCC\\nDDD\\nEEE'", &[]);
    options.size = GridSize {
        rows: 2,
        columns: 4,
        ..options.size
    };
    options.scrollback_lines = 2;
    let (mut session, wake) = common::spawn(options);
    common::wait_for(&mut session, &wake, |_, update| update.exit_status);

    let live = session.snapshot();
    assert_eq!(live.cells[0].character, 'D');
    assert_eq!(live.cells[4].character, 'E');
    session.scroll(i32::MAX);
    let history = session.snapshot();
    assert_eq!(history.display_offset, 2);
    assert_eq!(history.cells.len(), 8);
    assert_eq!(history.cells[0].character, 'B');
    assert_eq!(history.cells[4].character, 'C');
    assert_eq!(history.cursor.position, None);
    assert!(session.select(
        Position { row: 0, column: 2 },
        Position { row: 0, column: 1 }
    ));
    assert_eq!(session.selected_text().as_deref(), Some("BB"));
    assert!(session.snapshot().cells[1].selected);
    assert!(session.snapshot().cells[2].selected);
    session.clear_selection();
    assert_eq!(session.selected_text(), None);
    session.scroll(i32::MIN);
    assert_eq!(session.snapshot(), live);
}
