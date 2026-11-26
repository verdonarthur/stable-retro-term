//! The settings panel model: a few keys of the user file, edited in place with `toml_edit`.
//! It has no window or renderer types. The app draws `lines()` and writes `Outcome::Save`.

use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item, Value};

use crate::config::{Config, FONT_SIZE, Layer, MAX_SCROLLBACK_LINES, Overrides, PRESETS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Escape,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Nothing,
    Redraw,
    Close,
    /// Write this text to the file, then reload.
    Save(String),
}

/// The result of Left (-1) or Right (+1) on a row, computed from the file values.
enum Change {
    Set(Value),
    /// No change. `Some` explains which other row sets the bound.
    Stop(Option<String>),
}

/// One editable key. A new key is one new entry in `ROWS`.
struct Row {
    label: &'static str,
    key: &'static str,
    value: fn(&Config) -> String,
    change: fn(&Config, i64) -> Change,
    /// The command-line option that overrides this key, if it is active.
    overridden: fn(&Overrides) -> Option<&'static str>,
}

const ROWS: &[Row] = &[
    Row {
        label: "Preset",
        key: "preset",
        value: |config| config.preset.clone(),
        change: preset,
        overridden: |_| None,
    },
    Row {
        label: "CRT effects",
        key: "effects.crt",
        value: |config| on_off(config.effects.crt),
        change: |config, _| Change::Set((!config.effects.crt).into()),
        overridden: |overrides| overrides.plain.then_some("--plain"),
    },
    Row {
        label: "Font size",
        key: "font.size",
        value: |config| config.font.size.to_string(),
        change: font_size,
        overridden: |_| None,
    },
    Row {
        label: "Line height",
        key: "font.line_height",
        value: |config| config.font.line_height.to_string(),
        change: line_height,
        overridden: |_| None,
    },
    Row {
        label: "Scrollback (new shells)",
        key: "terminal.scrollback_lines",
        value: |config| config.terminal.scrollback_lines.to_string(),
        change: scrollback,
        overridden: |_| None,
    },
];

const LABEL_WIDTH: usize = 25;
const SCROLLBACK_STEP: i64 = 1000;

fn on_off(value: bool) -> String {
    if value { "on" } else { "off" }.into()
}

fn preset(config: &Config, direction: i64) -> Change {
    let count = PRESETS.len() as i64;
    let index = PRESETS
        .iter()
        .position(|(name, _)| *name == config.preset)
        .unwrap_or(0) as i64;
    let next = (index + direction).rem_euclid(count) as usize;
    Change::Set(PRESETS[next].0.into())
}

fn font_size(config: &Config, direction: i64) -> Change {
    let (size, line_height) = (config.font.size, config.font.line_height);
    let low = (*FONT_SIZE.start()).max(line_height.div_ceil(2));
    let high = (*FONT_SIZE.end()).min(line_height);
    let next = i64::from(size) + direction;
    if next > i64::from(high) {
        return Change::Stop((high < *FONT_SIZE.end()).then(|| {
            format!(
                "font.size cannot exceed font.line_height ({line_height}); raise line height first"
            )
        }));
    }
    if next < i64::from(low) {
        return Change::Stop((low > *FONT_SIZE.start()).then(|| {
            format!(
                "font.size cannot go below half of font.line_height ({line_height}); lower line height first"
            )
        }));
    }
    Change::Set(next.into())
}

fn line_height(config: &Config, direction: i64) -> Change {
    let (size, line_height) = (config.font.size, config.font.line_height);
    let next = i64::from(line_height) + direction;
    if next < i64::from(size) {
        return Change::Stop(Some(format!(
            "font.line_height cannot go below font.size ({size}); lower font size first"
        )));
    }
    if next > 2 * i64::from(size) {
        return Change::Stop(Some(format!(
            "font.line_height cannot exceed twice font.size ({}); raise font size first",
            2 * size
        )));
    }
    Change::Set(next.into())
}

fn scrollback(config: &Config, direction: i64) -> Change {
    let current = config.terminal.scrollback_lines as i64;
    let next = (current + direction * SCROLLBACK_STEP).clamp(0, MAX_SCROLLBACK_LINES as i64);
    if next == current {
        Change::Stop(None)
    } else {
        Change::Set(next.into())
    }
}

/// Sets the dotted `key` in place. An existing value keeps its spacing and trailing comment.
/// A missing table is appended as `[table]`; a missing key is appended to its table.
fn set(document: &mut DocumentMut, key: &str, value: Value) -> Result<(), String> {
    let (tables, name) = key.rsplit_once('.').map_or(("", key), |pair| pair);
    let mut table = document.as_table_mut() as &mut dyn toml_edit::TableLike;
    for part in tables.split('.').filter(|part| !part.is_empty()) {
        if !table.contains_key(part) {
            table.insert(part, toml_edit::table());
        }
        table = table
            .get_mut(part)
            .and_then(Item::as_table_like_mut)
            .ok_or_else(|| format!("`{part}` is not a table"))?;
    }
    match table.get_mut(name).and_then(Item::as_value_mut) {
        Some(current) => {
            let decor = current.decor().clone();
            *current = value;
            *current.decor_mut() = decor;
        }
        None => {
            table.insert(name, Item::Value(value));
        }
    }
    Ok(())
}

/// The resolved file values without `--plain`, so the steppers change what the file holds.
fn file_config(text: &str, overrides: &Overrides) -> Result<Config, String> {
    let file_overrides = Overrides {
        plain: false,
        ..overrides.clone()
    };
    Layer::parse(text)
        .and_then(|layer| Config::resolve(layer, &file_overrides))
        .map_err(|error| error.to_string())
}

fn resolve(text: &str, overrides: &Overrides) -> Result<Config, String> {
    Layer::parse(text)
        .and_then(|layer| Config::resolve(layer, overrides))
        .map_err(|error| error.to_string())
}

/// Every edit passed `Config::resolve`, so `preview` and `file` are always valid.
struct Editor {
    document: DocumentMut,
    file: Config,
}

pub struct Settings {
    path: Option<PathBuf>,
    original: Option<String>,
    overrides: Overrides,
    /// `None`: read-only, `status` says why.
    editor: Option<Editor>,
    preview: Config,
    selected: usize,
    status: String,
}

impl Settings {
    /// `original` is the file text, or `None` when the file does not exist.
    pub fn open(path: Option<PathBuf>, original: Option<String>, overrides: Overrides) -> Self {
        let text = original.as_deref().unwrap_or("");
        let editable = match &path {
            None => Err("no configuration path ($HOME is not set); read-only".to_string()),
            // `Config::resolve` first: its errors are one line with the position.
            Some(_) => resolve(text, &overrides)
                .and_then(|preview| {
                    let file = file_config(text, &overrides)?;
                    let document = text
                        .parse::<DocumentMut>()
                        .map_err(|error| error.to_string())?;
                    Ok((Editor { document, file }, preview))
                })
                .map_err(|error| format!("{error}; fix the file, then reload with Ctrl+Shift+R")),
        };
        let (editor, preview, status) = match editable {
            Ok((editor, preview)) => (Some(editor), preview, String::new()),
            Err(status) => (None, resolve("", &overrides).unwrap_or_default(), status),
        };
        Self {
            path,
            original,
            overrides,
            editor,
            preview,
            selected: 0,
            status,
        }
    }

    /// A read-only panel for a file that exists but cannot be read.
    pub fn unreadable(path: PathBuf, error: &std::io::Error, overrides: Overrides) -> Self {
        let mut settings = Self::open(None, None, overrides);
        settings.path = Some(path);
        settings.status = format!("cannot read the file: {error}; read-only");
        settings
    }

    pub fn key(&mut self, key: Key) -> Outcome {
        let Some(editor) = &mut self.editor else {
            return if key == Key::Escape {
                Outcome::Close
            } else {
                Outcome::Nothing
            };
        };
        let direction = match key {
            Key::Escape => return Outcome::Close,
            Key::Up if self.selected > 0 => {
                self.selected -= 1;
                return Outcome::Redraw;
            }
            Key::Down if self.selected + 1 < ROWS.len() => {
                self.selected += 1;
                return Outcome::Redraw;
            }
            Key::Up | Key::Down => return Outcome::Nothing,
            Key::Enter => {
                let text = editor.document.to_string();
                return if text == self.original.as_deref().unwrap_or("") {
                    Outcome::Close
                } else {
                    Outcome::Save(text)
                };
            }
            Key::Left => -1,
            Key::Right => 1,
        };
        let row = &ROWS[self.selected];
        let status = match (row.change)(&editor.file, direction) {
            Change::Stop(hint) => hint.unwrap_or_default(),
            Change::Set(value) => {
                let mut document = editor.document.clone();
                let text = set(&mut document, row.key, value).map(|()| document.to_string());
                match text.and_then(|text| {
                    Ok((
                        resolve(&text, &self.overrides)?,
                        file_config(&text, &self.overrides)?,
                    ))
                }) {
                    Ok((preview, file)) => {
                        *editor = Editor { document, file };
                        self.preview = preview;
                        self.status.clear();
                        return Outcome::Redraw;
                    }
                    Err(error) => error,
                }
            }
        };
        if status == self.status {
            Outcome::Nothing
        } else {
            self.status = status;
            Outcome::Redraw
        }
    }

    /// The text read at open: what `config::save` expects on disk.
    pub fn original(&self) -> Option<&str> {
        self.original.as_deref()
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn preview(&self) -> &Config {
        &self.preview
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn set_status(&mut self, status: String) {
        self.status = status;
    }

    /// The panel text, one entry per line, with the selected row marked.
    pub fn lines(&self) -> Vec<(String, bool)> {
        let line = |text: String| (text, false);
        let field = |label: &str, value: String| format!(" {label:<LABEL_WIDTH$} {value} ");
        let mut lines = vec![line(" Settings".into()), line(String::new())];
        if let Some(editor) = &self.editor {
            for (index, row) in ROWS.iter().enumerate() {
                let mut value = (row.value)(&self.preview);
                if let Some(option) = (row.overridden)(&self.overrides) {
                    value = format!("{value} ({option}; file: {})", (row.value)(&editor.file));
                }
                lines.push((field(row.label, value), index == self.selected));
            }
            lines.push(line(String::new()));
        }
        let family = Some(self.preview.font.family.clone())
            .filter(|family| !family.is_empty())
            .unwrap_or_else(|| "system monospace".into());
        lines.push(line(field("Font family", family)));
        let path = self
            .path
            .as_ref()
            .map_or_else(|| "none".into(), |path| path.display().to_string());
        lines.push(line(field("File", path)));
        lines.push(line(" Edit the file for shell settings.".into()));
        lines.push(line(String::new()));
        if !self.status.is_empty() {
            lines.push(line(format!(" {} ", self.status)));
        }
        lines.push(line(if self.editor.is_some() {
            " Up/Down select  Left/Right change  Enter save  Esc cancel".into()
        } else {
            " Esc close".into()
        }));
        lines
    }
}
