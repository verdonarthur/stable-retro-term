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
    NextCategory,
    BrowseProfiles,
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

const ANIMATION: Row = Row {
    label: "Legacy flicker",
    key: "effects.animation",
    value: |config| on_off(config.effects.animation),
    change: |config, _| Change::Set((!config.effects.animation).into()),
    overridden: |overrides| overrides.plain.then_some("--plain"),
};

const CONTROLS: &[(&str, &str, u8)] = &retro_renderer::CRT_CONTROLS;
const FIRST_CONTROL: usize = ROWS.len() + 1;
const ROW_COUNT: usize = FIRST_CONTROL + CONTROLS.len();

fn category(row: usize) -> &'static str {
    match row {
        0 | 21..=25 | 30..=32 => "General",
        2..=3 | 18..=20 | 26..=29 | 33 => "Terminal",
        4 => "Advanced",
        _ => "Effects",
    }
}

fn level(config: &Config, key: &str, default: u8) -> u8 {
    config.effects.levels.get(key).copied().unwrap_or(default)
}

fn row(index: usize) -> Option<&'static Row> {
    ROWS.get(index)
        .or_else(|| (index == ROWS.len()).then_some(&ANIMATION))
}

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

pub struct Category {
    pub header: Vec<String>,
    pub items: Vec<(String, bool)>,
    pub footer: CategoryFooter,
}

pub struct CategoryFooter {
    pub details: Vec<String>,
    pub status: Option<String>,
    pub help: &'static str,
}

pub struct Settings {
    path: Option<PathBuf>,
    original: Option<String>,
    overrides: Overrides,
    /// `None`: read-only, `status` says why.
    editor: Option<Editor>,
    preview: Config,
    selected: usize,
    profile_cursor: Option<usize>,
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
            profile_cursor: None,
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
        if let Some(index) = self.profile_cursor {
            self.profile_cursor = match key {
                Key::Up => Some((index + PRESETS.len() - 1) % PRESETS.len()),
                Key::Down => Some((index + 1) % PRESETS.len()),
                Key::Enter | Key::Escape | Key::BrowseProfiles => None,
                _ => return Outcome::Nothing,
            };
            if key == Key::Enter {
                self.apply("preset", Change::Set(PRESETS[index].0.into()));
            }
            return Outcome::Redraw;
        }
        if matches!(key, Key::Up | Key::Down) {
            let group = category(self.selected);
            let next = if key == Key::Up {
                (0..self.selected)
                    .rev()
                    .find(|&index| category(index) == group)
            } else {
                (self.selected + 1..ROW_COUNT).find(|&index| category(index) == group)
            };
            return if let Some(index) = next {
                self.selected = index;
                Outcome::Redraw
            } else {
                Outcome::Nothing
            };
        }
        let direction = match key {
            Key::Escape => return Outcome::Close,
            Key::Up | Key::Down => unreachable!("navigation handled above"),
            Key::NextCategory => {
                self.selected = match category(self.selected) {
                    "General" => 2,
                    "Terminal" => 1,
                    "Effects" => 4,
                    _ => 0,
                };
                return Outcome::Redraw;
            }
            Key::BrowseProfiles if self.selected == 0 => {
                self.profile_cursor = PRESETS
                    .iter()
                    .position(|(name, _)| *name == self.preview.preset);
                return Outcome::Redraw;
            }
            Key::BrowseProfiles => return Outcome::Nothing,
            Key::Enter => {
                let text = editor.document.to_string();
                return if text == self.original.as_deref().unwrap_or("") {
                    Outcome::Nothing
                } else {
                    Outcome::Save(text)
                };
            }
            Key::Left => -1,
            Key::Right => 1,
        };
        let (key, change) = if let Some(row) = row(self.selected) {
            (row.key.to_owned(), (row.change)(&editor.file, direction))
        } else {
            let (key, _, default) = CONTROLS[self.selected - FIRST_CONTROL];
            let current = i64::from(level(&editor.file, key, default));
            let next = (current + direction * 5).clamp(0, 100);
            let change = if next == current {
                Change::Stop(None)
            } else {
                Change::Set(next.into())
            };
            (format!("effects.levels.{key}"), change)
        };
        self.apply(&key, change)
    }

    fn apply(&mut self, key: &str, change: Change) -> Outcome {
        let Some(editor) = &mut self.editor else {
            return Outcome::Nothing;
        };
        let status = match change {
            Change::Stop(hint) => hint.unwrap_or_default(),
            Change::Set(value) => {
                let mut document = editor.document.clone();
                if key == "preset" {
                    document.remove("font");
                    document.remove("effects");
                }
                let text = set(&mut document, key, value).map(|()| document.to_string());
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

    pub fn mark_saved(&mut self, text: String) {
        self.original = Some(text);
        self.status = "Saved".into();
    }

    fn profile_label(&self) -> String {
        let Some(editor) = &self.editor else {
            return self.preview.preset.clone();
        };
        let base = format!("preset = {:?}", editor.file.preset);
        match file_config(&base, &self.overrides) {
            Ok(base) if base.font == editor.file.font && base.effects == editor.file.effects => {
                editor.file.preset.clone()
            }
            _ => "custom".into(),
        }
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

    /// The editable category, with all its items in keyboard navigation order.
    pub fn category(&self) -> Option<Category> {
        let editor = self.editor.as_ref()?;
        if self.profile_cursor.is_some() {
            return None;
        }
        let group = category(self.selected);
        let mut header = vec![
            " Settings".into(),
            String::new(),
            format!(" {group}  [Tab switches category]"),
        ];
        if self.selected == 0 {
            header.push(" Space opens the profile list.".into());
        }
        let items = (0..ROW_COUNT)
            .filter(|&index| category(index) == group)
            .map(|index| {
                let (label, value) = if let Some(row) = row(index) {
                    let mut value = if row.key == "preset" {
                        self.profile_label()
                    } else {
                        (row.value)(&self.preview)
                    };
                    if let Some(option) = (row.overridden)(&self.overrides) {
                        value = format!("{value} ({option}; file: {})", (row.value)(&editor.file));
                    }
                    (row.label, value)
                } else {
                    let (key, label, default) = CONTROLS[index - FIRST_CONTROL];
                    (label, format!("{}%", level(&self.preview, key, default)))
                };
                (field(label, &value), index == self.selected)
            })
            .collect();
        Some(Category {
            header,
            items,
            footer: CategoryFooter {
                details: self.file_details(),
                status: self.status_line(),
                help: " Up/Down select  Left/Right change  Enter save  Esc cancel",
            },
        })
    }

    /// The panel text, one entry per line, with the selected row marked.
    pub fn lines(&self) -> Vec<(String, bool)> {
        if let Some(index) = self.profile_cursor {
            const PAGE_SIZE: usize = 6;
            let start = index / PAGE_SIZE * PAGE_SIZE;
            let mut lines = vec![(format!(" Profiles  {}/{}", index + 1, PRESETS.len()), false)];
            lines.extend(
                PRESETS
                    .iter()
                    .enumerate()
                    .skip(start)
                    .take(PAGE_SIZE)
                    .map(|(row, (name, _))| (format!(" {name}"), row == index)),
            );
            lines.push((" Up/Down select  Enter choose  Esc back".into(), false));
            return lines;
        }
        let unselected = |text| (text, false);
        if let Some(category) = self.category() {
            return category
                .header
                .into_iter()
                .map(unselected)
                .chain(category.items)
                .chain(std::iter::once(unselected(String::new())))
                .chain(category.footer.details.into_iter().map(unselected))
                .chain(std::iter::once(unselected(String::new())))
                .chain(category.footer.status.into_iter().map(unselected))
                .chain(std::iter::once(unselected(category.footer.help.into())))
                .collect();
        }
        let mut lines = vec![unselected(" Settings".into()), unselected(String::new())];
        lines.extend(self.file_details().into_iter().map(unselected));
        lines.push(unselected(String::new()));
        lines.extend(self.status_line().into_iter().map(unselected));
        lines.push(unselected(" Esc close".into()));
        lines
    }

    fn file_details(&self) -> Vec<String> {
        let family = self.preview.font.family.as_str();
        let family = if family.is_empty() {
            "system monospace"
        } else {
            family
        };
        let path = self
            .path
            .as_ref()
            .map_or_else(|| "none".into(), |path| path.display().to_string());
        vec![
            field("Font family", family),
            field("File", &path),
            " Edit the file for shell settings.".into(),
        ]
    }

    fn status_line(&self) -> Option<String> {
        (!self.status.is_empty()).then(|| format!(" {} ", self.status))
    }
}

fn field(label: &str, value: &str) -> String {
    format!(" {label:<LABEL_WIDTH$} {value} ")
}
