/// Grid dimensions in cells and cell dimensions in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridSize {
    pub rows: u16,
    pub columns: u16,
    pub cell_width: u16,
    pub cell_height: u16,
}

/// Zero-based position in the visible viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub row: u16,
    pub column: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellWidth {
    Single,
    Wide,
    WideSpacer,
    LeadingWideSpacer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Underline {
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellAttributes {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub inverse: bool,
    pub hidden: bool,
    pub strikeout: bool,
    pub wrapped: bool,
    pub underline: Option<Underline>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub position: Position,
    pub character: char,
    pub combining: Box<[char]>,
    pub foreground: Rgb,
    pub background: Rgb,
    pub underline_color: Option<Rgb>,
    pub width: CellWidth,
    pub attributes: CellAttributes,
    pub selected: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorShape {
    Block,
    HollowBlock,
    Underline,
    Beam,
    Hidden,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    /// Absent when the viewport is scrolled past the cursor.
    pub position: Option<Position>,
    pub shape: CursorShape,
    pub blinking: bool,
    pub color: Rgb,
}

/// A point-in-time copy of the visible cells in row order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub size: GridSize,
    pub cells: Vec<Cell>,
    pub cursor: Cursor,
    /// Number of lines between the live screen and the current viewport.
    pub display_offset: usize,
    pub alternate_screen: bool,
    pub bracketed_paste: bool,
}
