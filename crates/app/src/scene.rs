use retro_renderer::scene::{Cluster, Glyph, Rect, Rgb, Scene};
use retro_terminal::{CellWidth, CursorShape, GridSize, Position, Snapshot, Underline};
use unicode_width::UnicodeWidthStr;

const SELECTION: Rgb = Rgb(55, 74, 101);
const BANNER: Rgb = Rgb(120, 24, 24);

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

pub(crate) fn font_metrics(scale: f64, size: u16, line_height: u16) -> (u16, u16) {
    let font = (f64::from(size) * scale)
        .round()
        .clamp(1.0, f64::from(u16::MAX)) as u16;
    let line = (f64::from(line_height) * scale)
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

pub(crate) fn pointer_cell(
    point: [f64; 2],
    viewport: [u32; 2],
    mode: retro_renderer::RenderMode,
    [left, top]: [u32; 2],
    grid: GridSize,
    clamp: bool,
) -> Option<Position> {
    let [x, y] = mode.source_point(point, viewport);
    cell_at(x - f64::from(left), y - f64::from(top), grid, clamp)
}

fn bounds(position: Position, grid: GridSize, [left, top]: [u32; 2]) -> Rect {
    let width = u32::from(grid.cell_width);
    let height = u32::from(grid.cell_height);
    Rect {
        x: left + u32::from(position.column) * width,
        y: top + u32::from(position.row) * height,
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

/// An empty window scene in the colors and metrics of `snapshot`.
pub(crate) fn empty(snapshot: &Snapshot, font_size: u16) -> Scene {
    Scene::new(
        rgb(snapshot.background),
        font_size,
        snapshot.size.cell_height,
    )
}

#[cfg(test)]
pub(crate) fn convert(
    snapshot: &Snapshot,
    font_size: u16,
    cursor_visible: bool,
    preedit: &str,
    banner: &str,
) -> Scene {
    let mut scene = empty(snapshot, font_size);
    pane(
        &mut scene,
        snapshot,
        [0, 0],
        true,
        cursor_visible,
        preedit,
        banner,
    );
    scene
}

/// Appends one pane at `origin`. An unfocused pane draws a steady hollow cursor and no preedit.
pub(crate) fn pane(
    scene: &mut Scene,
    snapshot: &Snapshot,
    origin: [u32; 2],
    focused: bool,
    cursor_visible: bool,
    preedit: &str,
    banner: &str,
) {
    let cursor = if focused {
        snapped_cursor(snapshot, cursor_visible)
    } else {
        snapped_cursor(snapshot, true).map(|(position, shape)| {
            let shape = if shape == CursorShape::Hidden {
                shape
            } else {
                CursorShape::HollowBlock
            };
            (position, shape)
        })
    };
    let preedit = if focused { preedit } else { "" };
    let pane_background = rgb(snapshot.background);
    if pane_background != scene.clear {
        let mut area = bounds(Position { column: 0, row: 0 }, snapshot.size, origin);
        area.width *= u32::from(snapshot.size.columns);
        area.height *= u32::from(snapshot.size.rows);
        scene.fill(area, pane_background);
    }

    for cell in &snapshot.cells {
        let area = bounds(cell.position, snapshot.size, origin);
        let background = if cell.selected {
            SELECTION
        } else if cell.attributes.inverse {
            rgb(cell.foreground)
        } else {
            rgb(cell.background)
        };
        if background != pane_background {
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
        let mut area = bounds(cell.position, snapshot.size, origin);
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
            cursor_fill(scene, area, rgb(snapshot.cursor.color), shape);
            if shape == CursorShape::Block {
                foreground = if cell.attributes.inverse {
                    rgb(cell.foreground)
                } else {
                    rgb(cell.background)
                };
                if foreground == rgb(snapshot.cursor.color) {
                    foreground = pane_background;
                }
            }
        }
        if cell.attributes.hidden {
            continue;
        }
        if let Some(underline) = cell.attributes.underline {
            decoration(
                scene,
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
        let mut area = bounds(position, snapshot.size, origin);
        let remaining = snapshot.size.columns - position.column;
        let columns = preedit.width().clamp(1, usize::from(remaining)) as u16;
        area.width *= u32::from(columns);
        overlay(scene, area, SELECTION, preedit);
    }
    if !banner.is_empty() && snapshot.size.rows > 0 {
        let mut area = bounds(
            Position {
                column: 0,
                row: snapshot.size.rows - 1,
            },
            snapshot.size,
            origin,
        );
        area.width *= u32::from(snapshot.size.columns);
        overlay(scene, area, BANNER, banner);
    }
}

/// Covers `area` with one fill and one glyph cluster that replaces the glyphs below it.
fn overlay(scene: &mut Scene, area: Rect, background: Rgb, text: &str) {
    scene.glyphs.retain(|glyph| {
        glyph.bounds.y != area.y
            || glyph.bounds.x + glyph.bounds.width <= area.x
            || glyph.bounds.x >= area.x + area.width
    });
    scene.fill(area, background);
    scene.glyphs.push(Glyph {
        bounds: area,
        color: Rgb(255, 255, 255),
        cluster: Cluster {
            text: text.into(),
            bold: false,
            italic: false,
        },
    });
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
        assert_eq!(font_metrics(1.5, 14, 19), (21, 29));
        assert_eq!(font_metrics(2.0, 20, 24), (40, 48));
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
    fn distorted_selection_maps_edges_and_preserves_padding() {
        use retro_renderer::RenderMode;

        let size = [85, 61];
        let grid = grid_size(size, 8, 20).unwrap();
        let crt = RenderMode::StaticCrt;
        let last_cell = Some(Position { column: 9, row: 2 });
        let last_cell_pixel = crt.display_point([79.0, 59.0], size);
        assert_eq!(
            pointer_cell(last_cell_pixel, size, crt, [0, 0], grid, false),
            last_cell
        );
        let padding = crt.display_point([82.0, 60.5], size);
        assert_eq!(pointer_cell(padding, size, crt, [0, 0], grid, false), None);
        assert_eq!(
            pointer_cell(padding, size, crt, [0, 0], grid, true),
            last_cell
        );
        assert_eq!(
            pointer_cell([0.0, 0.0], size, crt, [0, 0], grid, false),
            None
        );
        assert_eq!(
            pointer_cell([0.0, 0.0], size, RenderMode::Plain, [0, 0], grid, false),
            Some(Position { column: 0, row: 0 })
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
        let scene = convert(&snapshot, 14, true, "", "");
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

    #[test]
    fn banner_covers_the_last_row_within_the_grid() {
        for columns in [1, 10] {
            let grid = GridSize {
                columns,
                rows: 3,
                cell_width: 8,
                cell_height: 20,
            };
            let snapshot = Snapshot {
                size: grid,
                background: retro_terminal::Rgb(8, 8, 8),
                cells: Vec::new(),
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
            let scene = convert(&snapshot, 14, true, "", "config error: a long message");
            let expected = Rect {
                x: 0,
                y: 40,
                width: 8 * u32::from(columns),
                height: 20,
            };
            assert_eq!(scene.glyphs.len(), 1);
            assert_eq!(scene.glyphs[0].bounds, expected);
            assert_eq!(scene.fills.last().map(|fill| fill.bounds), Some(expected));
        }
    }

    fn sample(background: retro_terminal::Rgb, shape: CursorShape) -> Snapshot {
        let grid = GridSize {
            columns: 4,
            rows: 2,
            cell_width: 8,
            cell_height: 20,
        };
        let cell = |column, character, selected| Cell {
            position: Position { row: 0, column },
            character,
            combining: Box::default(),
            foreground: retro_terminal::Rgb(220, 220, 220),
            background,
            underline_color: None,
            width: CellWidth::Single,
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
            selected,
        };
        Snapshot {
            size: grid,
            background,
            cells: vec![cell(0, 'a', false), cell(1, 'b', true), cell(2, ' ', false)],
            cursor: Cursor {
                position: Some(Position { row: 0, column: 2 }),
                shape,
                blinking: true,
                color: retro_terminal::Rgb(255, 255, 255),
            },
            display_offset: 0,
            alternate_screen: false,
            bracketed_paste: false,
        }
    }

    #[test]
    fn single_pane_scene_equals_convert() {
        let snapshot = sample(retro_terminal::Rgb(8, 8, 8), CursorShape::Block);
        let base = convert(&snapshot, 14, true, "に", "config error");
        let mut window = empty(&snapshot, 14);
        pane(
            &mut window,
            &snapshot,
            [0, 0],
            true,
            true,
            "に",
            "config error",
        );
        assert_eq!(window.clear, base.clear);
        assert_eq!(
            (window.font_size, window.line_height),
            (base.font_size, base.line_height)
        );
        assert_eq!(format!("{:?}", window.fills), format!("{:?}", base.fills));
        assert_eq!(format!("{:?}", window.glyphs), format!("{:?}", base.glyphs));
        let whole_grid = Rect {
            x: 0,
            y: 0,
            width: 32,
            height: 40,
        };
        assert!(window.fills.iter().all(|fill| fill.bounds != whole_grid));
    }

    #[test]
    fn pane_origin_offsets_fills_and_glyphs() {
        let snapshot = sample(retro_terminal::Rgb(8, 8, 8), CursorShape::Block);
        let base = convert(&snapshot, 14, true, "", "");
        let mut moved = empty(&snapshot, 14);
        pane(&mut moved, &snapshot, [100, 40], true, true, "", "");
        let shift = |rect: Rect| Rect {
            x: rect.x + 100,
            y: rect.y + 40,
            ..rect
        };
        let fills: Vec<_> = base.fills.iter().map(|fill| shift(fill.bounds)).collect();
        let moved_fills: Vec<_> = moved.fills.iter().map(|fill| fill.bounds).collect();
        assert_eq!(moved_fills, fills);
        let glyphs: Vec<_> = base
            .glyphs
            .iter()
            .map(|glyph| shift(glyph.bounds))
            .collect();
        let moved_glyphs: Vec<_> = moved.glyphs.iter().map(|glyph| glyph.bounds).collect();
        assert_eq!(moved_glyphs, glyphs);

        let mut other = Scene::new(Rgb(0, 0, 0), 14, 20);
        pane(&mut other, &snapshot, [100, 40], true, true, "", "");
        assert_eq!(
            other.fills[0].bounds,
            Rect {
                x: 100,
                y: 40,
                width: 32,
                height: 40
            }
        );
        assert_eq!(other.fills[0].color, Rgb(8, 8, 8));
    }

    #[test]
    fn unfocused_pane_draws_a_hollow_cursor_and_no_preedit() {
        let snapshot = sample(retro_terminal::Rgb(8, 8, 8), CursorShape::Block);
        let mut scene = empty(&snapshot, 14);
        pane(&mut scene, &snapshot, [0, 0], false, false, "に", "");
        let cursor = Rect {
            x: 16,
            y: 0,
            width: 8,
            height: 20,
        };
        let edges = scene
            .fills
            .iter()
            .filter(|fill| fill.color == Rgb(255, 255, 255))
            .count();
        assert_eq!(edges, 4, "hollow block draws four edges, ignoring blink");
        assert!(scene.fills.iter().all(|fill| fill.bounds != cursor));
        assert!(scene.glyphs.iter().all(|glyph| glyph.cluster.text != "に"));

        let hidden = sample(retro_terminal::Rgb(8, 8, 8), CursorShape::Hidden);
        let mut scene = empty(&hidden, 14);
        pane(&mut scene, &hidden, [0, 0], false, true, "", "");
        assert!(
            scene
                .fills
                .iter()
                .all(|fill| fill.color != Rgb(255, 255, 255))
        );
    }
}
