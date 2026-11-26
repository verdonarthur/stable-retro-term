use std::path::PathBuf;

use retro_app::config::{Config, Layer, Overrides, SaveError, Source, load, save};
use retro_app::settings::{Key, Outcome, Settings};

const PRESET: usize = 0;
const CRT: usize = 1;
const SIZE: usize = 2;
const LINE_HEIGHT: usize = 3;
const SCROLLBACK: usize = 4;

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("retro-term-{}-{name}.toml", std::process::id()))
}

fn open(text: Option<&str>) -> Settings {
    Settings::open(
        Some(temp_path("unused")),
        text.map(Into::into),
        Overrides::default(),
    )
}

fn select(settings: &mut Settings, row: usize) {
    for _ in 0..row {
        settings.key(Key::Down);
    }
}

fn saved(settings: &mut Settings) -> String {
    match settings.key(Key::Enter) {
        Outcome::Save(text) => text,
        other => panic!("expected a save, got {other:?}"),
    }
}

fn change(text: &str, row: usize, key: Key) -> String {
    let mut settings = open(Some(text));
    select(&mut settings, row);
    assert_eq!(settings.key(key), Outcome::Redraw);
    saved(&mut settings)
}

#[test]
fn edit_keeps_comments_and_unrelated_keys() {
    let input = "# my retro-term file\n\npreset = \"static-crt\"\n\n# font settings\n[font]\nsize = 14   # pixels\nline_height = 19\n\n[shell]\nprogram = \"zsh\"\n";
    let expected = input.replace("size = 14 ", "size = 15 ");
    assert_eq!(change(input, SIZE, Key::Right), expected);
}

#[test]
fn edit_supports_inline_tables_and_dotted_keys() {
    assert_eq!(
        change("font = { size = 14 }\n", SIZE, Key::Right),
        "font = { size = 15 }\n"
    );
    assert_eq!(
        change("font.size = 14\n", SIZE, Key::Right),
        "font.size = 15\n"
    );
}

#[test]
fn edit_appends_missing_keys_and_tables() {
    assert_eq!(change("", CRT, Key::Right), "[effects]\ncrt = false\n");
    assert_eq!(
        change("[font]\nsize = 14\n", LINE_HEIGHT, Key::Right),
        "[font]\nsize = 14\nline_height = 20\n"
    );
    assert_eq!(
        change("[font]\nsize = 14\n", PRESET, Key::Right),
        "preset = \"plain\"\n[font]\nsize = 14\n"
    );
}

#[test]
fn enter_without_edits_closes_without_saving() {
    let mut settings = open(Some("[font]\nsize = 14\n"));
    settings.key(Key::Down);
    settings.key(Key::Up);
    assert_eq!(settings.key(Key::Enter), Outcome::Close);
    assert_eq!(open(None).key(Key::Enter), Outcome::Close);
}

#[test]
fn toggle_twice_saves_the_original_text() {
    let mut settings = open(Some("[effects]\ncrt = true # glow\n"));
    select(&mut settings, CRT);
    settings.key(Key::Right);
    settings.key(Key::Left);
    assert_eq!(settings.key(Key::Enter), Outcome::Close);
}

#[test]
fn steppers_stop_at_the_config_limits() {
    let mut settings = open(Some("[font]\nsize = 72\nline_height = 72\n"));
    select(&mut settings, SIZE);
    assert_eq!(settings.key(Key::Right), Outcome::Nothing);
    assert_eq!(settings.status(), "");

    let mut settings = open(Some("[terminal]\nscrollback_lines = 0\n"));
    select(&mut settings, SCROLLBACK);
    assert_eq!(settings.key(Key::Left), Outcome::Nothing);
    assert_eq!(
        change(
            "[terminal]\nscrollback_lines = 99500\n",
            SCROLLBACK,
            Key::Right
        ),
        "[terminal]\nscrollback_lines = 100000\n"
    );
    assert_eq!(
        change(
            "[terminal]\nscrollback_lines = 500\n",
            SCROLLBACK,
            Key::Left
        ),
        "[terminal]\nscrollback_lines = 0\n"
    );
}

#[test]
fn steppers_never_produce_an_invalid_file() {
    let valid = |settings: &mut Settings| {
        if let Outcome::Save(text) = settings.key(Key::Enter) {
            Config::resolve(Layer::parse(&text).unwrap(), &Overrides::default()).unwrap();
        }
    };
    let mut settings = open(None);
    select(&mut settings, SIZE);
    for _ in 0..5 {
        assert_eq!(settings.key(Key::Right), Outcome::Redraw);
        valid(&mut settings);
    }
    assert_eq!(settings.preview().font.size, 19);
    // The first stop shows the reason once; a repeated press changes nothing.
    assert_eq!(settings.key(Key::Right), Outcome::Redraw);
    assert!(settings.status().contains("font.line_height"));
    assert_eq!(settings.key(Key::Right), Outcome::Nothing);
    valid(&mut settings);

    let mut settings = open(Some("[font]\nsize = 14\nline_height = 14\n"));
    select(&mut settings, LINE_HEIGHT);
    assert_eq!(settings.key(Key::Left), Outcome::Redraw);
    assert!(settings.status().contains("font.size"));
    valid(&mut settings);

    let mut settings = open(Some("[font]\nsize = 10\n"));
    select(&mut settings, SIZE);
    assert_eq!(settings.key(Key::Left), Outcome::Redraw);
    assert!(settings.status().contains("font.line_height"));
    assert_eq!(settings.preview().font.size, 10);
    valid(&mut settings);
}

#[test]
fn invalid_file_opens_read_only() {
    for (text, message) in [
        ("[font\n", "line 1"),
        ("colour = 1\n", "colour"),
        ("[font]\nsize = 100\nline_height = 100\n", "font.size"),
    ] {
        let mut settings = open(Some(text));
        assert!(settings.status().contains(message), "{}", settings.status());
        assert!(settings.status().contains("Ctrl+Shift+R"));
        for key in [Key::Up, Key::Down, Key::Left, Key::Right, Key::Enter] {
            assert_eq!(settings.key(key), Outcome::Nothing);
        }
        assert_eq!(settings.key(Key::Escape), Outcome::Close);
    }
}

#[test]
fn missing_path_opens_read_only() {
    let mut settings = Settings::open(None, None, Overrides::default());
    assert_eq!(settings.key(Key::Enter), Outcome::Nothing);
    assert!(settings.status().contains("no configuration path"));
}

#[test]
fn preview_uses_the_real_precedence() {
    let mut settings = open(Some("[effects]\ncrt = true\n"));
    assert_eq!(settings.key(Key::Right), Outcome::Redraw);
    assert_eq!(settings.preview().preset, "plain");
    assert!(settings.preview().effects.crt);
}

#[test]
fn cli_override_is_shown_and_file_still_changes() {
    let overrides = Overrides {
        plain: true,
        ..Overrides::default()
    };
    let mut settings = Settings::open(Some(temp_path("unused")), None, overrides);
    select(&mut settings, CRT);
    assert_eq!(settings.key(Key::Right), Outcome::Redraw);
    assert!(!settings.preview().effects.crt);
    let row = &settings
        .lines()
        .into_iter()
        .find(|(_, selected)| *selected)
        .unwrap()
        .0;
    assert!(row.contains("off (--plain; file: off)"), "{row}");
    assert_eq!(saved(&mut settings), "[effects]\ncrt = false\n");
}

#[test]
fn saved_text_loads_to_the_preview() {
    let path = temp_path("settings-saved");
    std::fs::write(&path, "# keep\n[font]\nsize = 14\n").unwrap();
    let original = std::fs::read_to_string(&path).unwrap();
    let mut settings = Settings::open(Some(path.clone()), Some(original), Overrides::default());
    settings.key(Key::Right);
    select(&mut settings, SIZE - PRESET);
    settings.key(Key::Left);
    let text = saved(&mut settings);
    save(&path, settings.original(), &text).unwrap();
    let loaded = load(&Source::Explicit(path.clone()), &Overrides::default()).unwrap();
    assert_eq!(&loaded, settings.preview());
    assert_eq!(loaded.font.size, 13);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn saved_text_after_external_change_is_refused() {
    let path = temp_path("settings-external");
    std::fs::write(&path, "[font]\nsize = 14\n").unwrap();
    let mut settings = Settings::open(
        Some(path.clone()),
        Some("[font]\nsize = 14\n".into()),
        Overrides::default(),
    );
    select(&mut settings, SIZE);
    settings.key(Key::Right);
    let text = saved(&mut settings);
    std::fs::write(&path, "[font]\nsize = 16\n").unwrap();
    assert!(matches!(
        save(&path, settings.original(), &text),
        Err(SaveError::Changed)
    ));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "[font]\nsize = 16\n"
    );
    let loaded = load(&Source::Explicit(path.clone()), &Overrides::default()).unwrap();
    assert_eq!(loaded.font.size, 16);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn no_op_keys_do_not_redraw() {
    let mut settings = open(None);
    assert_eq!(settings.key(Key::Up), Outcome::Nothing);
    select(&mut settings, SCROLLBACK);
    assert_eq!(settings.key(Key::Down), Outcome::Nothing);
    let mut settings = open(Some("[terminal]\nscrollback_lines = 0\n"));
    select(&mut settings, SCROLLBACK);
    assert_eq!(settings.key(Key::Left), Outcome::Nothing);
    assert_eq!(settings.key(Key::Right), Outcome::Redraw);
    let mut read_only = open(Some("[font\n"));
    assert_eq!(read_only.key(Key::Down), Outcome::Nothing);
}
