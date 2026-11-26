//! Stateless scene helpers for window controls: the tab bar, the pane dividers, and the settings panel.

use retro_renderer::scene::{Cluster, Glyph, Rect, Rgb, Scene};
use unicode_width::UnicodeWidthChar;

use crate::layout;

const DIVIDER: Rgb = Rgb(90, 90, 90);
const TAB: Rgb = Rgb(40, 40, 40);
const TAB_TEXT: Rgb = Rgb(200, 200, 200);

/// Equal-width tabs in whole cells; the active tab is drawn inverted.
pub(crate) fn tab_bar(scene: &mut Scene, area: Rect, labels: &[(String, bool)], cell_width: u32) {
    let Some(width) = tab_width(area.width, labels.len(), cell_width) else {
        return;
    };
    for (index, (label, active)) in labels.iter().enumerate() {
        let tab = Rect {
            x: area.x + index as u32 * width,
            width,
            ..area
        };
        let (background, color) = if *active {
            (TAB_TEXT, TAB)
        } else {
            (TAB, TAB_TEXT)
        };
        scene.fill(tab, background);
        let text = truncate(label, (width / cell_width) as usize);
        let columns = columns_of(&text);
        if columns == 0 {
            continue;
        }
        scene.glyphs.push(Glyph {
            bounds: Rect {
                width: columns as u32 * cell_width,
                ..tab
            },
            color,
            cluster: Cluster {
                text,
                bold: *active,
                italic: false,
            },
        });
    }
}

/// The tab under `x` in a bar of `width` pixels.
pub(crate) fn tab_at(x: f64, width: u32, count: usize, cell_width: u32) -> Option<usize> {
    let tab = tab_width(width, count, cell_width)?;
    let index = (x / f64::from(tab)).floor();
    (index >= 0.0 && index < count as f64).then_some(index as usize)
}

pub(crate) fn dividers(scene: &mut Scene, dividers: &[layout::Rect]) {
    for divider in dividers {
        scene.fill(
            Rect {
                x: divider.x,
                y: divider.y,
                width: divider.width,
                height: divider.height,
            },
            DIVIDER,
        );
    }
}

/// Narrower panels truncate too much to be useful.
const MIN_PANEL_COLUMNS: u32 = 40;

/// The centered panel in whole cells with one cell of padding, or `None` when it does not fit.
/// Lines wider than the window are truncated when drawn.
pub(crate) fn settings_box(window: Rect, lines: &[(String, bool)], cell: [u32; 2]) -> Option<Rect> {
    let [cell_width, line_height] = cell.map(|value| value.max(1));
    let (columns, rows) = (window.width / cell_width, window.height / line_height);
    let longest = lines.iter().map(|(text, _)| columns_of(text)).max()?;
    let inner = u32::try_from(longest)
        .ok()?
        .max(MIN_PANEL_COLUMNS)
        .min(columns.checked_sub(2)?);
    let height = u32::try_from(lines.len()).ok()? + 2;
    if inner < MIN_PANEL_COLUMNS || height > rows {
        return None;
    }
    let width = inner + 2;
    Some(Rect {
        x: window.x + (columns - width) / 2 * cell_width,
        y: window.y + (rows - height) / 2 * line_height,
        width: width * cell_width,
        height: height * line_height,
    })
}

/// Draws the panel on top of everything pushed before; glyphs under it are removed.
pub(crate) fn settings_panel(
    scene: &mut Scene,
    window: Rect,
    lines: &[(String, bool)],
    cell: [u32; 2],
) {
    let Some(panel) = settings_box(window, lines, cell) else {
        return;
    };
    draw_panel(scene, panel, lines, cell);
}

/// Uses the whole window and wraps text before choosing the visible rows.
pub(crate) fn full_panel(
    scene: &mut Scene,
    window: Rect,
    lines: &[(String, bool)],
    cell: [u32; 2],
    scroll: usize,
) {
    let [width, height] = cell;
    let columns = (window.width / width).saturating_sub(2) as usize;
    let rows = (window.height / height).saturating_sub(2) as usize;
    if columns < 2 || rows == 0 {
        scene.glyphs.clear();
        scene.fill(window, TAB);
        return;
    }
    let wrapped = wrap_panel(lines, columns - 1);
    let last_start = wrapped.len().saturating_sub(rows);
    let start = wrapped
        .iter()
        .position(|(_, selected)| *selected)
        .map_or(scroll, |index| index.saturating_add(1).saturating_sub(rows))
        .min(last_start);
    let end = (start + rows).min(wrapped.len());
    draw_panel(scene, window, &wrapped[start..end], cell);
}

pub(crate) fn full_panel_scroll_limit(
    window: Rect,
    lines: &[(String, bool)],
    cell: [u32; 2],
) -> usize {
    let [width, height] = cell;
    let columns = (window.width / width).saturating_sub(3).max(1) as usize;
    let rows = (window.height / height).saturating_sub(2) as usize;
    wrap_panel(lines, columns).len().saturating_sub(rows)
}

fn wrap_panel(lines: &[(String, bool)], columns: usize) -> Vec<(String, bool)> {
    // Leave one column of slack for fractional font advances.
    let mut wrapped = Vec::new();
    for (line, selected) in lines {
        let mut rest = line.as_str();
        while columns_of(rest) > columns {
            let prefix = truncate(rest, columns);
            let end = prefix
                .trim_end()
                .rfind(' ')
                .filter(|&index| index > 1)
                .unwrap_or(prefix.len());
            if end == 0 {
                break;
            }
            wrapped.push((rest[..end].to_owned(), *selected));
            rest = rest[end..].trim_start();
        }
        wrapped.push((rest.to_owned(), *selected));
    }
    wrapped
}

fn draw_panel(scene: &mut Scene, panel: Rect, lines: &[(String, bool)], cell: [u32; 2]) {
    let [cell_width, line_height] = cell;
    scene.glyphs.retain(|glyph| {
        let bounds = glyph.bounds;
        bounds.x + bounds.width <= panel.x
            || bounds.x >= panel.x + panel.width
            || bounds.y + bounds.height <= panel.y
            || bounds.y >= panel.y + panel.height
    });
    scene.fill(panel, TAB);
    let inner = panel.width / cell_width - 2;
    for (index, (line, selected)) in lines.iter().enumerate() {
        let row = Rect {
            x: panel.x + cell_width,
            y: panel.y + (index as u32 + 1) * line_height,
            width: inner * cell_width,
            height: line_height,
        };
        let color = if *selected {
            scene.fill(row, TAB_TEXT);
            TAB
        } else {
            TAB_TEXT
        };
        let text = truncate(line, inner as usize);
        if text.is_empty() {
            continue;
        }
        // A shaped line can advance farther than its rounded cell count.
        scene.glyphs.push(Glyph {
            bounds: row,
            color,
            cluster: Cluster {
                text,
                bold: *selected,
                italic: false,
            },
        });
    }
}

fn columns_of(text: &str) -> usize {
    text.chars().filter_map(UnicodeWidthChar::width).sum()
}

fn tab_width(width: u32, count: usize, cell_width: u32) -> Option<u32> {
    let columns = width / cell_width.max(1) / u32::try_from(count).ok().filter(|&n| n > 0)?;
    (columns > 0).then_some(columns * cell_width)
}

fn truncate(label: &str, columns: usize) -> String {
    let mut used = 0;
    label
        .chars()
        .take_while(|character| {
            used += character.width().unwrap_or(0);
            used <= columns
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(width: u32) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width,
            height: 20,
        }
    }

    #[test]
    fn full_panel_uses_window_and_keeps_wrapped_text() {
        let window = Rect {
            height: 100,
            ..bar(160)
        };
        let mut scene = Scene::new(Rgb(0, 0, 0), 14, 20);
        let lines = vec![("Next tab Ctrl+PageDown / Ctrl+PageUp".into(), false)];
        full_panel(&mut scene, window, &lines, [8, 20], 0);
        assert_eq!(scene.fills[0].bounds, window);
        assert!(
            scene
                .glyphs
                .iter()
                .any(|glyph| glyph.cluster.text.contains("Ctrl+PageUp"))
        );
    }

    #[test]
    fn tab_bar_marks_the_active_tab_and_truncates_labels() {
        let labels = vec![
            ("1: shell".to_string(), false),
            ("2: a very long title".to_string(), true),
        ];
        let mut scene = Scene::new(Rgb(0, 0, 0), 14, 20);
        tab_bar(&mut scene, bar(160), &labels, 8);
        assert_eq!(scene.fills.len(), 2);
        assert_eq!(scene.fills[0].color, TAB);
        assert_eq!(scene.fills[1].color, TAB_TEXT);
        assert_eq!(scene.fills[1].bounds.x, 80);
        assert_eq!(scene.glyphs[0].cluster.text, "1: shell");
        assert_eq!(scene.glyphs[1].cluster.text, "2: a very ");
        assert_eq!(scene.glyphs[1].color, TAB);
        assert_eq!(scene.glyphs[1].bounds.width, 80);
        assert_eq!(tab_at(79.0, 160, 2, 8), Some(0));
        assert_eq!(tab_at(80.0, 160, 2, 8), Some(1));
        assert_eq!(tab_at(165.0, 160, 2, 8), None);

        for width in [0, 8, 12] {
            let mut scene = Scene::new(Rgb(0, 0, 0), 14, 20);
            tab_bar(&mut scene, bar(width), &labels, 8);
            assert!(scene.glyphs.iter().all(|glyph| glyph.bounds.width <= 8));
        }
        let mut scene = Scene::new(Rgb(0, 0, 0), 14, 20);
        tab_bar(&mut scene, bar(16), &[("界".into(), true)], 8);
        assert_eq!(scene.glyphs[0].bounds.width, 16);
        let mut scene = Scene::new(Rgb(0, 0, 0), 14, 20);
        tab_bar(&mut scene, bar(8), &[("界".into(), true)], 8);
        assert!(scene.glyphs.is_empty());
    }

    #[test]
    fn dividers_are_filled() {
        let mut scene = Scene::new(Rgb(0, 0, 0), 14, 20);
        let divider = layout::Rect {
            x: 400,
            y: 20,
            width: 2,
            height: 580,
        };
        dividers(&mut scene, &[divider]);
        assert_eq!(scene.fills.len(), 1);
        assert_eq!(scene.fills[0].color, DIVIDER);
        assert_eq!(
            scene.fills[0].bounds,
            Rect {
                x: 400,
                y: 20,
                width: 2,
                height: 580
            }
        );
    }

    #[test]
    fn settings_panel_covers_glyphs_and_fits_cells() {
        let cell = [8, 20];
        let window = Rect {
            x: 0,
            y: 0,
            width: 804,
            height: 405,
        };
        let lines = vec![
            ("x".repeat(48), false),
            ("selected".to_string(), true),
            (String::new(), false),
        ];
        let panel = settings_box(window, &lines, cell).unwrap();
        assert_eq!(
            panel,
            Rect {
                x: 25 * 8,
                y: 7 * 20,
                width: 50 * 8,
                height: 5 * 20,
            }
        );

        let mut scene = Scene::new(Rgb(0, 0, 0), 14, 20);
        let glyph = |x, y| Glyph {
            bounds: Rect {
                x,
                y,
                width: 8,
                height: 20,
            },
            color: TAB_TEXT,
            cluster: Cluster {
                text: "a".into(),
                bold: false,
                italic: false,
            },
        };
        scene.glyphs = vec![
            glyph(208, 140),
            glyph(400, 220),
            glyph(0, 220),
            glyph(600, 140),
        ];
        settings_panel(&mut scene, window, &lines, cell);
        assert_eq!(scene.fills.len(), 2);
        assert_eq!(scene.fills[0].bounds, panel);
        assert_eq!(scene.fills[1].color, TAB_TEXT);
        assert_eq!(scene.fills[1].bounds.y, 9 * 20);
        let texts: Vec<_> = scene
            .glyphs
            .iter()
            .map(|glyph| (glyph.bounds.x, glyph.cluster.text.as_str()))
            .collect();
        assert_eq!(
            texts,
            [
                (0, "a"),
                (600, "a"),
                (26 * 8, "x".repeat(48).as_str()),
                (26 * 8, "selected")
            ]
        );
        assert_eq!(scene.glyphs[3].color, TAB);
        assert_eq!(scene.glyphs[3].bounds, scene.fills[1].bounds);

        let wide = vec![("y".repeat(200), false)];
        let panel = settings_box(window, &wide, cell).unwrap();
        assert_eq!(panel.width, 100 * 8);
        for small in [
            Rect {
                height: 80,
                ..window
            },
            Rect {
                width: 300,
                ..window
            },
        ] {
            assert_eq!(settings_box(small, &lines, cell), None);
            let mut scene = Scene::new(Rgb(0, 0, 0), 14, 20);
            settings_panel(&mut scene, small, &lines, cell);
            assert!(scene.fills.is_empty() && scene.glyphs.is_empty());
        }
    }
}
