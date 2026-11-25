use retro_renderer::scene::{Cluster, Glyph, Rect, Rgb, Scene};
use retro_terminal::{CellWidth, CursorShape, GridSize, Position, Snapshot, Underline};
use unicode_width::UnicodeWidthStr;

const SELECTION: Rgb = Rgb(55, 74, 101);

pub(crate) fn grid_size(
    [width, height]: [u32; 2],
    cell_width: u16,
    cell_height: u16,
) -> Option<GridSize> {
    let size = GridSize {
        columns: u16::try_from(width.checked_div(u32::from(cell_width))?).ok()?,
        rows: u16::try_from(height.checked_div(u32::from(cell_height))?).ok()?,
        cell_width,
        cell_height,
    };
    size.is_valid().then_some(size)
}

pub(crate) fn font_metrics(scale: f64) -> (u16, u16) {
    let font = (14.0 * scale).round().clamp(1.0, f64::from(u16::MAX)) as u16;
    let line = (19.0 * scale)
        .round()
        .clamp(f64::from(font), f64::from(u16::MAX)) as u16;
    (font, line)
}

pub(crate) fn cell_at(x: f64, y: f64, grid: GridSize, clamp: bool) -> Option<Position> {
    let column = (x / f64::from(grid.cell_width)).floor();
    let row = (y / f64::from(grid.cell_height)).floor();
    let max_column = f64::from(grid.columns.checked_sub(1)?);
    let max_row = f64::from(grid.rows.checked_sub(1)?);
    if !clamp && !(0.0..=max_column).contains(&column) {
        return None;
    }
    if !clamp && !(0.0..=max_row).contains(&row) {
        return None;
    }
    Some(Position {
        column: column.clamp(0.0, max_column) as u16,
        row: row.clamp(0.0, max_row) as u16,
    })
}

fn bounds(position: Position, grid: GridSize) -> Rect {
    let width = u32::from(grid.cell_width);
    let height = u32::from(grid.cell_height);
    Rect {
        x: u32::from(position.column) * width,
        y: u32::from(position.row) * height,
        width,
        height,
    }
}

fn rgb(retro_terminal::Rgb(r, g, b): retro_terminal::Rgb) -> Rgb {
    Rgb(r, g, b)
}

fn snapped_cursor(snapshot: &Snapshot, visible: bool) -> Option<(Position, CursorShape)> {
    let mut position = snapshot.cursor.position.filter(|_| visible)?;
    if position.column > 0
        && snapshot
            .cells
            .iter()
            .any(|cell| cell.position == position && cell.width == CellWidth::WideSpacer)
    {
        position.column -= 1;
    }
    Some((position, snapshot.cursor.shape))
}

pub(crate) fn convert(
    snapshot: &Snapshot,
    font_size: u16,
    cursor_visible: bool,
    preedit: &str,
) -> Scene {
    let mut scene = Scene::new(
        rgb(snapshot.background),
        font_size,
        snapshot.size.cell_height,
    );
    let cursor = snapped_cursor(snapshot, cursor_visible);

    for cell in &snapshot.cells {
        let area = bounds(cell.position, snapshot.size);
        let background = if cell.selected {
            SELECTION
        } else if cell.attributes.inverse {
            rgb(cell.foreground)
        } else {
            rgb(cell.background)
        };
        if background != scene.clear {
            scene.fill(area, background);
        }
    }

    for cell in &snapshot.cells {
        if matches!(
            cell.width,
            CellWidth::WideSpacer | CellWidth::LeadingWideSpacer
        ) {
            continue;
        }
        let mut area = bounds(cell.position, snapshot.size);
        if cell.width == CellWidth::Wide && cell.position.column + 1 < snapshot.size.columns {
            area.width *= 2;
        }
        let mut foreground = if cell.attributes.inverse {
            rgb(cell.background)
        } else {
            rgb(cell.foreground)
        };
        if cell.attributes.dim {
            foreground = Rgb(foreground.0 / 2, foreground.1 / 2, foreground.2 / 2);
        }
        if let Some((position, shape)) = cursor
            && position == cell.position
        {
            cursor_fill(&mut scene, area, rgb(snapshot.cursor.color), shape);
            if shape == CursorShape::Block {
                foreground = if cell.attributes.inverse {
                    rgb(cell.foreground)
                } else {
                    rgb(cell.background)
                };
                if foreground == rgb(snapshot.cursor.color) {
                    foreground = scene.clear;
                }
            }
        }
        if cell.attributes.hidden {
            continue;
        }
        if let Some(underline) = cell.attributes.underline {
            decoration(
                &mut scene,
                area,
                cell.underline_color.map_or(foreground, rgb),
                underline,
            );
        }
        if cell.attributes.strikeout {
            scene.fill(
                Rect {
                    y: area.y + area.height / 2,
                    height: 1,
                    ..area
                },
                foreground,
            );
        }
        if cell.combining.is_empty()
            && (cell.character == ' ' || scene.box_drawing(cell.character, area, foreground))
        {
            continue;
        }
        let mut text = String::from(cell.character);
        text.extend(cell.combining.iter());
        scene.glyphs.push(Glyph {
            bounds: area,
            color: foreground,
            cluster: Cluster {
                text,
                bold: cell.attributes.bold,
                italic: cell.attributes.italic,
            },
        });
    }

    if let Some(position) = snapshot.cursor.position
        && !preedit.is_empty()
        && position.column < snapshot.size.columns
    {
        let mut area = bounds(position, snapshot.size);
        let remaining = snapshot.size.columns - position.column;
        let columns = preedit.width().clamp(1, usize::from(remaining)) as u16;
        area.width *= u32::from(columns);
        scene.glyphs.retain(|glyph| {
            glyph.bounds.y != area.y
                || glyph.bounds.x + glyph.bounds.width <= area.x
                || glyph.bounds.x >= area.x + area.width
        });
        scene.fill(area, SELECTION);
        scene.glyphs.push(Glyph {
            bounds: area,
            color: Rgb(255, 255, 255),
            cluster: Cluster {
                text: preedit.into(),
                bold: false,
                italic: false,
            },
        });
    }
    scene
}

fn cursor_fill(scene: &mut Scene, area: Rect, color: Rgb, shape: CursorShape) {
    let bottom = area.y + area.height - 1;
    let right = area.x + area.width - 1;
    match shape {
        CursorShape::Block => scene.fill(area, color),
        CursorShape::Beam => scene.fill(Rect { width: 1, ..area }, color),
        CursorShape::Underline => scene.fill(
            Rect {
                y: bottom,
                height: 1,
                ..area
            },
            color,
        ),
        CursorShape::HollowBlock => {
            scene.fill(Rect { height: 1, ..area }, color);
            scene.fill(
                Rect {
                    y: bottom,
                    height: 1,
                    ..area
                },
                color,
            );
            scene.fill(Rect { width: 1, ..area }, color);
            scene.fill(
                Rect {
                    x: right,
                    width: 1,
                    ..area
                },
                color,
            );
        }
        CursorShape::Hidden => {}
    }
}

fn decoration(scene: &mut Scene, area: Rect, color: Rgb, underline: Underline) {
    let y = area.y + area.height - 1;
    match underline {
        Underline::Single | Underline::Double => {
            scene.fill(
                Rect {
                    y,
                    height: 1,
                    ..area
                },
                color,
            );
            if underline == Underline::Double && area.height >= 3 {
                scene.fill(
                    Rect {
                        y: y - 2,
                        height: 1,
                        ..area
                    },
                    color,
                );
            }
        }
        Underline::Dotted | Underline::Dashed | Underline::Curly => {
            let mark = if underline == Underline::Dashed { 3 } else { 1 };
            for x in area.x..area.x + area.width {
                let phase = x % (mark + 1);
                if underline == Underline::Curly || phase < mark {
                    let rise =
                        u32::from(underline == Underline::Curly && x % 4 < 2 && area.height > 1);
                    scene.fill(
                        Rect {
                            x,
                            y: y - rise,
                            width: 1,
                            height: 1,
                        },
                        color,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use retro_terminal::{Cell, CellAttributes, Cursor};

    use super::*;

    #[test]
    fn physical_grid_and_selection_use_the_same_cells() {
        assert_eq!(font_metrics(1.5), (21, 29));
        assert!(grid_size([11, 20], 8, 20).is_none());
        assert!(grid_size([80_000, 20], 8, 20).is_none());
        let grid = grid_size([85, 61], 8, 20).unwrap();
        assert_eq!((grid.columns, grid.rows), (10, 3));
        assert_eq!(
            cell_at(17.0, 40.0, grid, false),
            Some(Position { column: 2, row: 2 })
        );
        assert_eq!(cell_at(81.0, -1.0, grid, false), None);
        assert_eq!(
            cell_at(81.0, -1.0, grid, true),
            Some(Position { column: 9, row: 0 })
        );
    }

    #[test]
    fn wide_wrap_spacer_and_combining_mark_keep_fixed_positions() {
        let grid = grid_size([32, 40], 8, 20).unwrap();
        let cell = |row, column, width, character, combining: &[char]| Cell {
            position: Position { row, column },
            character,
            combining: combining.into(),
            foreground: retro_terminal::Rgb(220, 220, 220),
            background: retro_terminal::Rgb(8, 8, 8),
            underline_color: None,
            width,
            attributes: CellAttributes {
                bold: false,
                dim: false,
                italic: false,
                inverse: false,
                hidden: false,
                strikeout: false,
                wrapped: false,
                underline: None,
            },
            selected: false,
        };
        let snapshot = Snapshot {
            size: grid,
            background: retro_terminal::Rgb(8, 8, 8),
            cells: vec![
                cell(0, 3, CellWidth::LeadingWideSpacer, ' ', &[]),
                cell(1, 0, CellWidth::Wide, '界', &[]),
                cell(1, 1, CellWidth::WideSpacer, ' ', &[]),
                cell(1, 2, CellWidth::Single, 'a', &['\u{301}']),
            ],
            cursor: Cursor {
                position: None,
                shape: CursorShape::Hidden,
                blinking: false,
                color: retro_terminal::Rgb(255, 255, 255),
            },
            display_offset: 0,
            alternate_screen: false,
            bracketed_paste: false,
        };
        let scene = convert(&snapshot, 14, true, "");
        assert_eq!(scene.glyphs.len(), 2);
        assert_eq!(
            scene.glyphs[0].bounds,
            Rect {
                x: 0,
                y: 20,
                width: 16,
                height: 20
            }
        );
        assert_eq!(scene.glyphs[1].bounds.x, 16);
        assert_eq!(scene.glyphs[1].cluster.text, "a\u{301}");
    }
}
