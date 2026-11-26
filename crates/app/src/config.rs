//! Typed text configuration: built-in defaults, presets, the user file, and CLI overrides.

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const DEFAULT_PRESET: &str = "static-crt";
pub const PRESETS: [(&str, &str); 2] = [
    (
        "static-crt",
        include_str!("../../../assets/presets/static-crt.toml"),
    ),
    ("plain", include_str!("../../../assets/presets/plain.toml")),
];

const FONT_SIZE: std::ops::RangeInclusive<u16> = 6..=72;
const MAX_SCROLLBACK_LINES: usize = 100_000;

/// The resolved and validated settings. The only settings model of the application.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub preset: String,
    pub effects: Effects,
    pub font: Font,
    pub terminal: Terminal,
    pub shell: Shell,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Effects {
    pub crt: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Font {
    /// Empty selects the system monospace family.
    pub family: String,
    /// Pixels at scale 1.
    pub size: u16,
    /// Pixels at scale 1.
    pub line_height: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Terminal {
    pub scrollback_lines: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shell {
    /// Empty selects the user's login shell.
    pub program: String,
    pub args: Vec<String>,
    /// Empty inherits the working directory.
    pub working_directory: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            preset: DEFAULT_PRESET.into(),
            effects: Effects { crt: true },
            font: Font {
                family: String::new(),
                size: 14,
                line_height: 19,
            },
            terminal: Terminal {
                scrollback_lines: 10_000,
            },
            shell: Shell::default(),
        }
    }
}

/// One partial configuration layer: a preset file or the user file.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    pub preset: Option<String>,
    #[serde(default)]
    pub effects: EffectsLayer,
    #[serde(default)]
    pub font: FontLayer,
    #[serde(default)]
    pub terminal: TerminalLayer,
    #[serde(default)]
    pub shell: ShellLayer,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectsLayer {
    pub crt: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontLayer {
    pub family: Option<String>,
    pub size: Option<u16>,
    pub line_height: Option<u16>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalLayer {
    pub scrollback_lines: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShellLayer {
    pub program: Option<String>,
    pub args: Option<Vec<String>>,
    pub working_directory: Option<String>,
}

impl Layer {
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        toml::from_str(text).map_err(|error| ConfigError::parse(text, &error))
    }

    fn apply(self, config: &mut Config) {
        fn set<T>(target: &mut T, value: Option<T>) {
            if let Some(value) = value {
                *target = value;
            }
        }
        set(&mut config.effects.crt, self.effects.crt);
        set(&mut config.font.family, self.font.family);
        set(&mut config.font.size, self.font.size);
        set(&mut config.font.line_height, self.font.line_height);
        set(
            &mut config.terminal.scrollback_lines,
            self.terminal.scrollback_lines,
        );
        set(&mut config.shell.program, self.shell.program);
        set(&mut config.shell.args, self.shell.args);
        set(
            &mut config.shell.working_directory,
            self.shell.working_directory,
        );
    }
}

/// The command-line layer. It is kept and applied again on every reload.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Overrides {
    pub plain: bool,
    pub program: Option<(String, Vec<String>)>,
}

impl Config {
    /// Defaults, then the selected preset, then the user layer, then the CLI overrides.
    pub fn resolve(user: Layer, overrides: &Overrides) -> Result<Self, ConfigError> {
        let name = user.preset.clone().unwrap_or_else(|| DEFAULT_PRESET.into());
        let preset = preset(&name)?;
        let mut config = Self {
            preset: name,
            ..Self::default()
        };
        preset.apply(&mut config);
        user.apply(&mut config);
        if overrides.plain {
            config.effects.crt = false;
        }
        if let Some((program, args)) = &overrides.program {
            config.shell.program.clone_from(program);
            config.shell.args.clone_from(args);
        }
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        let range = |key, message: String| Err(ConfigError::Range { key, message });
        let font = &self.font;
        if !FONT_SIZE.contains(&font.size) {
            return range(
                "font.size",
                format!(
                    "must be in {}..={}, got {}",
                    FONT_SIZE.start(),
                    FONT_SIZE.end(),
                    font.size
                ),
            );
        }
        let lines = font.size..=font.size * 2;
        if !lines.contains(&font.line_height) {
            return range(
                "font.line_height",
                format!(
                    "must be in {}..={} (font.size to twice font.size), got {}",
                    lines.start(),
                    lines.end(),
                    font.line_height
                ),
            );
        }
        if self.terminal.scrollback_lines > MAX_SCROLLBACK_LINES {
            return range(
                "terminal.scrollback_lines",
                format!(
                    "must be in 0..={MAX_SCROLLBACK_LINES}, got {}",
                    self.terminal.scrollback_lines
                ),
            );
        }
        if self.shell.program.is_empty() && !self.shell.args.is_empty() {
            return range("shell.args", "requires shell.program".into());
        }
        Ok(())
    }
}

fn preset(name: &str) -> Result<Layer, ConfigError> {
    let (_, text) = PRESETS
        .iter()
        .find(|(known, _)| *known == name)
        .ok_or_else(|| ConfigError::UnknownPreset {
            name: name.into(),
            known: PRESETS.iter().map(|(known, _)| *known).collect(),
        })?;
    Layer::parse(text)
}

/// What a reload from `old` to `new` must do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// Rebuild the renderer: effects or font family changed.
    pub renderer: bool,
    /// Recompute cell metrics and resize the grid and PTY.
    pub layout: bool,
    /// Applies only when a new session starts.
    pub new_sessions: bool,
}

impl Changes {
    pub fn between(old: &Config, new: &Config) -> Self {
        let renderer = old.effects != new.effects || old.font.family != new.font.family;
        Self {
            renderer,
            layout: renderer
                || old.font.size != new.font.size
                || old.font.line_height != new.font.line_height,
            new_sessions: old.terminal != new.terminal || old.shell != new.shell,
        }
    }
}

/// Keeps the current configuration when the candidate is invalid.
pub fn reload(
    current: &Config,
    candidate: Result<Config, ConfigError>,
) -> (Config, Option<ConfigError>) {
    match candidate {
        Ok(config) => (config, None),
        Err(error) => (current.clone(), Some(error)),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// The user file. A missing file gives the defaults.
    Default(Option<PathBuf>),
    /// A file named with `--config`. A missing file is an error.
    Explicit(PathBuf),
}

impl Source {
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Default(path) => path.as_deref(),
            Self::Explicit(path) => Some(path),
        }
    }
}

pub fn load(source: &Source, overrides: &Overrides) -> Result<Config, ConfigError> {
    let text = match source {
        Source::Default(None) => String::new(),
        Source::Default(Some(path)) => match fs::read_to_string(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            result => result.map_err(ConfigError::Io)?,
        },
        Source::Explicit(path) => fs::read_to_string(path).map_err(ConfigError::Io)?,
    };
    Config::resolve(Layer::parse(&text)?, overrides)
}

/// `$XDG_CONFIG_HOME/retro-term/config.toml` when that value is absolute,
/// else `$HOME/.config/retro-term/config.toml`.
pub fn user_config_path(
    xdg_config_home: Option<OsString>,
    home: Option<OsString>,
) -> Option<PathBuf> {
    let absolute = |value: Option<OsString>| value.map(PathBuf::from).filter(|p| p.is_absolute());
    absolute(xdg_config_home)
        .or_else(|| absolute(home).map(|home| home.join(".config")))
        .map(|base| base.join("retro-term").join("config.toml"))
}

#[derive(Debug)]
pub enum ConfigError {
    Io(io::Error),
    Parse {
        line: usize,
        column: usize,
        source_line: String,
        message: String,
    },
    Range {
        key: &'static str,
        message: String,
    },
    UnknownPreset {
        name: String,
        known: Vec<&'static str>,
    },
    Font {
        family: String,
    },
}

impl ConfigError {
    fn parse(text: &str, error: &toml::de::Error) -> Self {
        let offset = error.span().map_or(0, |span| span.start).min(text.len());
        let before = &text[..offset];
        let line = before.matches('\n').count() + 1;
        let line_start = before.rfind('\n').map_or(0, |index| index + 1);
        let source_line = text[line_start..].lines().next().unwrap_or("").trim();
        Self::Parse {
            line,
            column: before[line_start..].chars().count() + 1,
            source_line: source_line.into(),
            message: error.message().trim().into(),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Parse {
                line,
                column,
                source_line,
                message,
            } => write!(f, "line {line}, column {column} `{source_line}`: {message}"),
            Self::Range { key, message } => write!(f, "{key} {message}"),
            Self::UnknownPreset { name, known } => {
                write!(f, "unknown preset `{name}`; known: {}", known.join(", "))
            }
            Self::Font { family } => write!(f, "font.family `{family}` is not installed"),
        }
    }
}

impl std::error::Error for ConfigError {}
