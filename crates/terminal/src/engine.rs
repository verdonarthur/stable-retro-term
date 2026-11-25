use alacritty_terminal::event::{EventListener, WindowSize};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::term::{self, Term, TermMode};
use alacritty_terminal::vte::ansi::{self, Color, NamedColor};

use crate::snapshot::{
    Cell, CellAttributes, CellWidth, Cursor, CursorShape, GridSize, Position, Rgb, Snapshot,
    Underline,
};

impl GridSize {
    pub(crate) fn is_valid(self) -> bool {
        usize::from(self.columns) >= term::MIN_COLUMNS
            && usize::from(self.rows) >= term::MIN_SCREEN_LINES
            && self.cell_width != 0
            && self.cell_height != 0
            && self.columns.checked_mul(self.cell_width).is_some()
            && self.rows.checked_mul(self.cell_height).is_some()
    }

    pub(crate) fn window_size(self) -> WindowSize {
        WindowSize {
            num_lines: self.rows,
            num_cols: self.columns,
            cell_width: self.cell_width,
            cell_height: self.cell_height,
        }
    }
}

pub(crate) struct TermSize(pub GridSize);

impl Dimensions for TermSize {
    fn total_lines(&self) -> usize {
        self.screen_lines()
    }

    fn screen_lines(&self) -> usize {
        usize::from(self.0.rows)
    }

    fn columns(&self) -> usize {
        usize::from(self.0.columns)
    }
}

pub(crate) fn terminal<T: EventListener>(size: GridSize, history: usize, listener: T) -> Term<T> {
    let config = term::Config {
        scrolling_history: history,
        osc52: term::Osc52::Disabled,
        ..term::Config::default()
    };
    Term::new(config, &TermSize(size), listener)
}

pub(crate) fn terminal_point(position: Position, offset: usize) -> Point {
    Point::new(
        Line(i32::from(position.row) - offset as i32),
        Column(usize::from(position.column)),
    )
}

pub(crate) fn snapshot<T: EventListener>(terminal: &Term<T>, size: GridSize) -> Snapshot {
    let view = terminal.renderable_content();
    let position = |point: Point| {
        let row = u16::try_from(point.line.0 + view.display_offset as i32).ok()?;
        (row < size.rows).then_some(Position {
            row,
            column: point.column.0 as u16,
        })
    };
    let mut cells = Vec::with_capacity(usize::from(size.rows) * usize::from(size.columns));
    cells.extend(view.display_iter.map(|indexed| {
        let cell = indexed.cell;
        let flags = cell.flags;
        let selected = view.selection.is_some_and(|selection| {
            selection.contains_cell(&indexed, view.cursor.point, view.cursor.shape)
        });
        Cell {
            position: position(indexed.point).expect("display iterator stays in the viewport"),
            character: cell.c,
            combining: cell.zerowidth().unwrap_or_default().into(),
            foreground: rgb(view.colors, cell.fg),
            background: rgb(view.colors, cell.bg),
            underline_color: cell.underline_color().map(|color| rgb(view.colors, color)),
            width: match flags
                & (Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                Flags::WIDE_CHAR => CellWidth::Wide,
                Flags::WIDE_CHAR_SPACER => CellWidth::WideSpacer,
                Flags::LEADING_WIDE_CHAR_SPACER => CellWidth::LeadingWideSpacer,
                _ => CellWidth::Single,
            },
            attributes: CellAttributes {
                bold: flags.contains(Flags::BOLD),
                dim: flags.contains(Flags::DIM),
                italic: flags.contains(Flags::ITALIC),
                inverse: flags.contains(Flags::INVERSE),
                hidden: flags.contains(Flags::HIDDEN),
                strikeout: flags.contains(Flags::STRIKEOUT),
                wrapped: flags.contains(Flags::WRAPLINE),
                underline: match flags & Flags::ALL_UNDERLINES {
                    Flags::UNDERLINE => Some(Underline::Single),
                    Flags::DOUBLE_UNDERLINE => Some(Underline::Double),
                    Flags::UNDERCURL => Some(Underline::Curly),
                    Flags::DOTTED_UNDERLINE => Some(Underline::Dotted),
                    Flags::DASHED_UNDERLINE => Some(Underline::Dashed),
                    _ => None,
                },
            },
            selected,
        }
    }));
    Snapshot {
        size,
        cells,
        cursor: Cursor {
            position: position(view.cursor.point),
            shape: match view.cursor.shape {
                ansi::CursorShape::Block => CursorShape::Block,
                ansi::CursorShape::HollowBlock => CursorShape::HollowBlock,
                ansi::CursorShape::Underline => CursorShape::Underline,
                ansi::CursorShape::Beam => CursorShape::Beam,
                ansi::CursorShape::Hidden => CursorShape::Hidden,
            },
            blinking: terminal.cursor_style().blinking,
            color: rgb(view.colors, Color::Named(NamedColor::Cursor)),
        },
        display_offset: view.display_offset,
        alternate_screen: view.mode.contains(TermMode::ALT_SCREEN),
        bracketed_paste: view.mode.contains(TermMode::BRACKETED_PASTE),
    }
}

fn rgb(colors: &Colors, color: Color) -> Rgb {
    let rgb = match color {
        Color::Spec(rgb) => rgb,
        Color::Named(color) => palette_color(colors, color as usize),
        Color::Indexed(index) => palette_color(colors, usize::from(index)),
    };
    Rgb(rgb.r, rgb.g, rgb.b)
}

pub(crate) fn palette_color(colors: &Colors, index: usize) -> ansi::Rgb {
    const ANSI: [(u8, u8, u8); 16] = [
        (0x18, 0x18, 0x18),
        (0xac, 0x42, 0x42),
        (0x90, 0xa9, 0x59),
        (0xf4, 0xbf, 0x75),
        (0x6a, 0x9f, 0xb5),
        (0xaa, 0x75, 0x9f),
        (0x75, 0xb5, 0xaa),
        (0xd8, 0xd8, 0xd8),
        (0x6b, 0x6b, 0x6b),
        (0xc5, 0x55, 0x55),
        (0xaa, 0xc4, 0x74),
        (0xfe, 0xca, 0x88),
        (0x82, 0xb8, 0xc8),
        (0xc2, 0x8c, 0xb8),
        (0x93, 0xd3, 0xc3),
        (0xf8, 0xf8, 0xf8),
    ];
    const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];
    if let Some(color) = colors[index] {
        return color;
    }
    let (r, g, b) = match index {
        0..=15 => ANSI[index],
        16..=231 => {
            let index = index - 16;
            (CUBE[index / 36], CUBE[index / 6 % 6], CUBE[index % 6])
        }
        232..=255 => {
            let gray = 8 + (index - 232) as u8 * 10;
            (gray, gray, gray)
        }
        256 | 258 | 267 => (0xd8, 0xd8, 0xd8),
        259..=266 => {
            let (r, g, b) = ANSI[index - 259];
            let dim = |value| (u16::from(value) * 2 / 3) as u8;
            (dim(r), dim(g), dim(b))
        }
        _ => (0x18, 0x18, 0x18),
    };
    ansi::Rgb { r, g, b }
}
