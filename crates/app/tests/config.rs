use std::ffi::OsString;
use std::path::PathBuf;

use retro_app::config::{
    Changes, Config, ConfigError, Layer, Overrides, PRESETS, SaveError, Source, load, reload, save,
    user_config_path,
};

fn resolve(text: &str) -> Result<Config, ConfigError> {
    Config::resolve(Layer::parse(text)?, &Overrides::default())
}

fn error(text: &str) -> String {
    resolve(text).unwrap_err().to_string()
}

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("retro-term-{}-{name}.toml", std::process::id()))
}

#[test]
fn defaults_match_the_base_behavior() {
    let config = Config::default();
    assert!(config.effects.crt);
    assert!(!config.effects.animation);
    assert_eq!(config.preset, "static-crt");
    assert_eq!(config.font.family, "");
    assert_eq!((config.font.size, config.font.line_height), (14, 19));
    assert_eq!(config.terminal.scrollback_lines, 10_000);
    assert_eq!(config.shell.program, "");
    assert!(config.shell.args.is_empty());
    assert_eq!(config.shell.working_directory, "");
}

#[test]
fn empty_file_and_missing_default_file_give_defaults() {
    assert_eq!(resolve("").unwrap(), Config::default());
    let overrides = Overrides::default();
    let missing = temp_path("missing-default");
    assert_eq!(
        load(&Source::Default(Some(missing)), &overrides).unwrap(),
        Config::default()
    );
    assert_eq!(
        load(&Source::Default(None), &overrides).unwrap(),
        Config::default()
    );
}

#[test]
fn missing_explicit_file_is_an_error() {
    let missing = temp_path("missing-explicit");
    assert!(matches!(
        load(&Source::Explicit(missing), &Overrides::default()),
        Err(ConfigError::Io(_))
    ));
}

#[test]
fn explicit_file_is_loaded() {
    let path = temp_path("explicit");
    std::fs::write(&path, "[font]\nsize = 20\nline_height = 24\n").unwrap();
    let config = load(&Source::Explicit(path.clone()), &Overrides::default());
    std::fs::remove_file(&path).unwrap();
    assert_eq!(config.unwrap().font.size, 20);
}

#[test]
fn precedence_is_defaults_then_preset_then_user_then_cli() {
    let plain = resolve("preset = \"plain\"").unwrap();
    assert!(!plain.effects.crt);
    assert_eq!(plain.preset, "plain");
    let user = "preset = \"plain\"\n[effects]\ncrt = true\n[font]\nsize = 16\n";
    let config = resolve(user).unwrap();
    assert!(config.effects.crt);
    assert_eq!(config.font.size, 16);
    let overrides = Overrides {
        plain: true,
        program: Some(("vi".into(), vec!["-u".into()])),
    };
    let config = Config::resolve(
        Layer::parse(&format!("{user}[shell]\nprogram = \"zsh\"\n")).unwrap(),
        &overrides,
    )
    .unwrap();
    assert!(!config.effects.crt);
    assert_eq!(config.shell.program, "vi");
    assert_eq!(config.shell.args, ["-u"]);
}

#[test]
fn unknown_key_reports_key_and_line() {
    let top = error("presett = \"plain\"");
    assert!(top.contains("presett") && top.contains("line 1"), "{top}");
    let nested = error("[effects]\ncrt = true\n[font]\nsizee = 3\n");
    assert!(
        nested.contains("sizee") && nested.contains("line 4"),
        "{nested}"
    );
}

#[test]
fn wrong_type_reports_key() {
    let message = error("[font]\nsize = \"big\"\n");
    assert!(
        message.contains("size") && message.contains("line 2"),
        "{message}"
    );
}

#[test]
fn out_of_range_values_name_the_range() {
    for (text, key, range) in [
        ("[font]\nsize = 0", "font.size", "6..=72"),
        ("[font]\nsize = 200", "font.size", "6..=72"),
        ("[font]\nline_height = 10", "font.line_height", "14..=28"),
        (
            "[terminal]\nscrollback_lines = 100001",
            "terminal.scrollback_lines",
            "0..=100000",
        ),
        ("[shell]\nargs = [\"-l\"]", "shell.args", "shell.program"),
    ] {
        let message = error(text);
        assert!(
            message.contains(key) && message.contains(range),
            "{message}"
        );
    }
}

#[test]
fn unknown_preset_lists_known_names() {
    let message = error("preset = \"amber\"");
    assert!(
        message.contains("amber") && message.contains("static-crt, plain"),
        "{message}"
    );
}

#[test]
fn example_file_equals_defaults() {
    let example = include_str!("../../../assets/config.example.toml");
    let config = resolve(example).unwrap();
    assert!(!config.effects.animation);
    assert_eq!(config, Config::default());
}

#[test]
fn animated_preset_enables_animation() {
    let preset = "preset = \"animated-crt\"\n";
    let config = resolve(preset).unwrap();
    assert!(config.effects.crt && config.effects.animation);
    let config = resolve(&format!("{preset}[effects]\nanimation = false\n")).unwrap();
    assert!(config.effects.crt && !config.effects.animation);
    let plain = Overrides {
        plain: true,
        ..Overrides::default()
    };
    let config = Config::resolve(Layer::parse(preset).unwrap(), &plain).unwrap();
    assert!(!config.effects.crt);
}

#[test]
fn every_builtin_preset_resolves() {
    for (name, text) in PRESETS {
        assert!(Layer::parse(text).unwrap().preset.is_none(), "{name}");
        assert_eq!(
            resolve(&format!("preset = \"{name}\"")).unwrap().preset,
            name
        );
    }
}

#[test]
fn config_path_follows_xdg() {
    let path = |xdg: Option<&str>, home: Option<&str>| {
        user_config_path(xdg.map(OsString::from), home.map(OsString::from))
    };
    let expected = |base: &str| Some(PathBuf::from(base).join("retro-term/config.toml"));
    assert_eq!(path(Some("/xdg"), Some("/home/a")), expected("/xdg"));
    assert_eq!(
        path(Some("relative"), Some("/home/a")),
        expected("/home/a/.config")
    );
    assert_eq!(path(Some(""), Some("/home/a")), expected("/home/a/.config"));
    assert_eq!(path(None, Some("/home/a")), expected("/home/a/.config"));
    assert_eq!(path(None, None), None);
}

#[test]
fn changes_classify_live_and_new_session_settings() {
    let old = Config::default();
    let changed = |change: fn(&mut Config)| {
        let mut new = old.clone();
        change(&mut new);
        Changes::between(&old, &new)
    };
    assert_eq!(Changes::between(&old, &old), Changes::default());
    let crt = changed(|c| c.effects.crt = false);
    assert!(crt.renderer && crt.layout && !crt.new_sessions);
    let animation = changed(|c| c.effects.animation = true);
    assert!(animation.renderer && animation.layout && !animation.new_sessions);
    let family = changed(|c| c.font.family = "Mono".into());
    assert!(family.renderer && family.layout);
    let size = changed(|c| c.font.size = 16);
    assert!(!size.renderer && size.layout && !size.new_sessions);
    let scrollback = changed(|c| c.terminal.scrollback_lines = 5);
    assert!(!scrollback.renderer && !scrollback.layout && scrollback.new_sessions);
    let shell = changed(|c| c.shell.program = "zsh".into());
    assert!(!shell.layout && shell.new_sessions);
}

#[test]
fn failed_reload_keeps_the_previous_config() {
    let current = resolve("[font]\nsize = 16").unwrap();
    let (config, failure) = reload(&current, resolve("[font]\nsize = 0"));
    assert_eq!(config, current);
    assert!(failure.is_some());
    let (config, failure) = reload(&current, resolve(""));
    assert_eq!(config, Config::default());
    assert!(failure.is_none());
}

#[test]
fn config_round_trips_through_toml() {
    let mut config = resolve("preset = \"plain\"").unwrap();
    config.shell.program = "zsh".into();
    config.shell.args = vec!["-l".into()];
    let text = toml::to_string(&config).unwrap();
    assert_eq!(toml::from_str::<Config>(&text).unwrap(), config);
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("retro-term-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn names(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

#[test]
fn save_creates_a_missing_file_and_its_directory() {
    let dir = temp_dir("save-create");
    let path = dir.join("nested/retro-term/config.toml");
    save(&path, None, "preset = \"plain\"\n").unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "preset = \"plain\"\n"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn save_replaces_the_file_and_leaves_no_temp_file() {
    let dir = temp_dir("save-replace");
    let path = dir.join("config.toml");
    std::fs::write(&path, "a = 1\n").unwrap();
    save(&path, Some("a = 1\n"), "a = 2\n").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "a = 2\n");
    assert_eq!(names(&dir), ["config.toml"]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn save_keeps_file_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp_dir("save-mode");
    let path = dir.join("config.toml");
    std::fs::write(&path, "").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    save(&path, Some(""), "preset = \"plain\"\n").unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn save_writes_through_a_symlink() {
    let dir = temp_dir("save-symlink");
    std::fs::create_dir(dir.join("real")).unwrap();
    std::fs::write(dir.join("real/config.toml"), "old\n").unwrap();
    let link = dir.join("config.toml");
    std::os::unix::fs::symlink(dir.join("real/config.toml"), &link).unwrap();
    save(&link, Some("old\n"), "new\n").unwrap();
    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(
        std::fs::read_to_string(dir.join("real/config.toml")).unwrap(),
        "new\n"
    );
    assert_eq!(names(&dir.join("real")), ["config.toml"]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn save_refuses_an_externally_changed_file() {
    let dir = temp_dir("save-changed");
    let path = dir.join("config.toml");
    std::fs::write(&path, "b\n").unwrap();
    assert!(matches!(
        save(&path, Some("a\n"), "c\n"),
        Err(SaveError::Changed)
    ));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "b\n");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn save_refuses_a_file_created_since_open() {
    let dir = temp_dir("save-created");
    let path = dir.join("config.toml");
    std::fs::write(&path, "b\n").unwrap();
    assert!(matches!(save(&path, None, "c\n"), Err(SaveError::Changed)));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "b\n");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn save_refuses_a_file_deleted_since_open() {
    let dir = temp_dir("save-deleted");
    let path = dir.join("config.toml");
    assert!(matches!(
        save(&path, Some("a\n"), "c\n"),
        Err(SaveError::Changed)
    ));
    assert!(!path.exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn save_failure_keeps_the_old_file() {
    let dir = temp_dir("save-failure");
    let target = dir.join("config.toml");
    std::fs::create_dir(&target).unwrap();
    assert!(matches!(save(&target, None, "x\n"), Err(SaveError::Io(_))));
    assert!(target.is_dir());
    assert_eq!(names(&dir), ["config.toml"]);

    let file = dir.join("file");
    std::fs::write(&file, "keep\n").unwrap();
    assert!(matches!(
        save(&file.join("config.toml"), None, "x\n"),
        Err(SaveError::Io(_))
    ));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "keep\n");

    #[cfg(unix)]
    {
        let link = dir.join("dangling.toml");
        std::os::unix::fs::symlink(dir.join("missing.toml"), &link).unwrap();
        assert!(matches!(save(&link, None, "x\n"), Err(SaveError::Io(_))));
        assert!(!dir.join("missing.toml").exists());
    }
    std::fs::remove_dir_all(dir).unwrap();
}
