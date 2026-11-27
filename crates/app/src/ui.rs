//! Stateless scene helpers for window controls: the tab bar, the pane dividers, and the settings panel.

use retro_renderer::scene::{Cluster, Glyph, Rect, Rgb, Scene};
use unicode_width::UnicodeWidthChar;

use crate::layout;
use crate::settings::{Category, CategoryFooter};

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
    let (columns, rows) = full_panel_size(window, cell);
    if columns == 0 || rows == 0 {
        clear_full_panel(scene, window);
        return;
    }
    let wrapped = wrap_panel(lines, columns);
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
    let (columns, rows) = full_panel_size(window, cell);
    wrap_panel(lines, columns.max(1)).len().saturating_sub(rows)
}

fn full_panel_size(window: Rect, [width, height]: [u32; 2]) -> (usize, usize) {
    // Leave two cells for padding and one column for fractional font advances.
    (
        (window.width / width).saturating_sub(3) as usize,
        (window.height / height).saturating_sub(2) as usize,
    )
}

fn clear_full_panel(scene: &mut Scene, window: Rect) {
    scene.glyphs.clear();
    scene.fill(window, TAB);
}

pub(crate) fn settings_category(
    scene: &mut Scene,
    window: Rect,
    category: &Category,
    cell: [u32; 2],
) {
    let (columns, rows) = full_panel_size(window, cell);
    if columns == 0 || rows == 0 {
        clear_full_panel(scene, window);
        return;
    }
    let lines = category_lines(category, columns, rows);
    draw_panel(scene, window, &lines, cell);
}

fn category_lines(category: &Category, columns: usize, rows: usize) -> Vec<(String, bool)> {
    let selected = category
        .items
        .iter()
        .position(|(_, selected)| *selected)
        .expect("a settings category has a selected item");
    let items: Vec<_> = category
        .items
        .iter()
        .map(|(text, selected)| wrap_line(text, *selected, columns))
        .collect();
    let mut header: Vec<_> = category
        .header
        .iter()
        .flat_map(|text| wrap_line(text, false, columns))
        .collect();
    let essentials: Vec<_> = category
        .footer
        .status
        .as_deref()
        .into_iter()
        .chain([category.footer.help])
        .flat_map(|text| wrap_line(text, false, columns))
        .collect();
    let mut footer = vec![(String::new(), false)];
    footer.extend(
        category
            .footer
            .details
            .iter()
            .flat_map(|text| wrap_line(text, false, columns)),
    );
    footer.push((String::new(), false));
    footer.extend_from_slice(&essentials);

    if let Some(list) = fit_category(&items, selected, columns, rows, &header, &footer) {
        return list;
    }
    header.retain(|(text, _)| !text.is_empty());
    footer.retain(|(text, _)| !text.is_empty());
    fit_category(&items, selected, columns, rows, &header, &footer)
        .or_else(|| fit_category(&items, selected, columns, rows, &[], &essentials))
        .unwrap_or_else(|| compact_category(category, selected, columns, rows))
}

fn fit_category(
    items: &[Vec<(String, bool)>],
    selected: usize,
    columns: usize,
    rows: usize,
    header: &[(String, bool)],
    footer: &[(String, bool)],
) -> Option<Vec<(String, bool)>> {
    let available = rows.checked_sub(header.len() + footer.len())?;
    let total = items.len();
    if items.iter().map(Vec::len).sum::<usize>() <= available {
        return Some(
            header
                .iter()
                .chain(items.iter().flatten())
                .chain(footer)
                .cloned()
                .collect(),
        );
    }

    // Try earlier starts first so equal-sized ranges keep the earlier items visible.
    let mut best = 0..0;
    let mut top = Vec::new();
    for start in 0..=selected {
        let mut height = 0;
        for (index, item) in items.iter().enumerate().skip(start) {
            height += item.len();
            if height > available {
                break;
            }
            let end = index + 1;
            if end <= selected || end - start <= best.len() {
                continue;
            }
            let cue = wrap_line(&range_text(start, end, total), false, columns);
            if height + cue.len() + usize::from(end < total) <= available {
                best = start..end;
                top = cue;
            }
        }
    }
    if best.is_empty() {
        return None;
    }
    let bottom = (best.end < total).then(|| (" ↓".into(), false));
    Some(
        header
            .iter()
            .cloned()
            .chain(top)
            .chain(items[best].iter().flatten().cloned())
            .chain(bottom)
            .chain(footer.iter().cloned())
            .collect(),
    )
}

fn range_text(start: usize, end: usize, total: usize) -> String {
    let arrow = if start > 0 { "↑ " } else { "" };
    format!(" {arrow}{}–{end} of {total}", start + 1)
}

fn compact_category(
    category: &Category,
    selected: usize,
    columns: usize,
    rows: usize,
) -> Vec<(String, bool)> {
    let CategoryFooter { status, help, .. } = &category.footer;
    let mut remaining = rows - 1;
    let mut take_line = |text: &str| {
        if remaining == 0 {
            return None;
        }
        remaining -= 1;
        Some((truncate(text.trim_start(), columns), false))
    };
    let help = take_line(if columns_of(help) <= columns {
        help
    } else {
        "↑↓ ←→ Enter Esc"
    });
    let status = status.as_deref().and_then(&mut take_line);
    let total = category.items.len();
    let top = (total > 1)
        .then(|| range_text(selected, selected + 1, total))
        .as_deref()
        .and_then(&mut take_line);
    let bottom = (selected + 1 < total).then(|| take_line("↓")).flatten();
    let item = truncate(category.items[selected].0.trim_start(), columns);
    top.into_iter()
        .chain([(item, true)])
        .chain(bottom)
        .chain(status)
        .chain(help)
        .collect()
}

fn wrap_panel(lines: &[(String, bool)], columns: usize) -> Vec<(String, bool)> {
    lines
        .iter()
        .flat_map(|(text, selected)| wrap_line(text, *selected, columns))
        .collect()
}

fn wrap_line(mut rest: &str, selected: bool, columns: usize) -> Vec<(String, bool)> {
    let mut wrapped = Vec::new();
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
        wrapped.push((rest[..end].to_owned(), selected));
        rest = rest[end..].trim_start();
    }
    wrapped.push((rest.to_owned(), selected));
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
    use crate::config::Overrides;
    use crate::settings::{Key, Settings};

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

    fn example_category(count: usize, selected: usize) -> Category {
        Category {
            header: vec!["General".into()],
            items: (1..=count)
                .map(|index| (format!("Item {index}"), index == selected))
                .collect(),
            footer: CategoryFooter {
                details: vec![],
                status: None,
                help: "Up/Down select  Esc cancel",
            },
        }
    }

    #[test]
    fn settings_category_uses_all_available_rows() {
        for count in [1, 9] {
            let category = example_category(count, count);
            let lines = category_lines(&category, 60, count + 4);
            assert_eq!(lines[1..=count], category.items);
            assert_eq!(lines.last().unwrap().0, category.footer.help);
            assert_eq!(lines.len(), count + 4);
        }
    }

    #[test]
    fn settings_category_marks_overflow_at_each_edge() {
        for (selected, range, hidden_below) in [
            (1, "1–3 of 9", true),
            (5, "↑ 3–5 of 9", true),
            (9, "↑ 6–9 of 9", false),
        ] {
            let category = example_category(9, selected);
            let lines = category_lines(&category, 60, 9);
            assert_eq!(lines[1].0.trim(), range);
            let item_count = if hidden_below { 3 } else { 4 };
            assert_eq!(
                lines[2 + item_count].0.trim(),
                if hidden_below { "↓" } else { "" }
            );
            let highlighted: Vec<_> = lines.iter().filter(|(_, selected)| *selected).collect();
            assert_eq!(highlighted, [&category.items[selected - 1]]);
            assert_eq!(lines.last().unwrap().0, category.footer.help);
        }
    }

    #[test]
    fn settings_category_counts_wrapped_items_once_and_keeps_the_footer() {
        let mut category = example_category(9, 5);
        category.items[4].0 = "Selected item with a long value".into();
        category.footer.details = vec!["File /a/very/long/settings-path".into()];
        category.footer.status = Some("Status with a long explanation".into());
        let lines = category_lines(&category, 16, 17);
        assert_eq!(lines[1].0.trim(), "↑ 4–5 of 9");
        let highlighted: Vec<_> = lines
            .iter()
            .filter_map(|(line, selected)| selected.then_some(line.as_str()))
            .collect();
        assert_eq!(highlighted, ["Selected item", "with a long", "value"]);
        assert!(lines.iter().any(|(line, _)| line.contains("/a/very/long")));
        assert!(lines.iter().any(|(line, _)| line.contains("explanation")));
        assert!(lines.iter().any(|(line, _)| line.contains("Up/Down")));
        assert_eq!(lines.last().unwrap().0, "cancel");
        assert!(lines.len() <= 17);
    }

    #[test]
    fn settings_category_draws_in_a_window_with_at_most_one_row() {
        let mut category = example_category(9, 5);
        category.items[4].0 = "界界界".into();
        for rows in [0, 1] {
            let window = Rect {
                height: (rows + 2) * 20,
                ..bar(6 * 8)
            };
            let mut scene = Scene::new(Rgb(0, 0, 0), 14, 20);
            settings_category(&mut scene, window, &category, [8, 20]);
            assert_eq!(scene.fills[0].bounds, window);
            if rows == 0 {
                assert!(scene.glyphs.is_empty());
            } else {
                assert_eq!(scene.glyphs.len(), 1);
                assert_eq!(scene.glyphs[0].cluster.text, "界");
                assert!(scene.glyphs[0].cluster.bold);
            }
        }
    }

    #[test]
    fn settings_category_keeps_the_selection_and_help_after_resize() {
        let mut settings = Settings::open(Some("config.toml".into()), None, Overrides::default());
        for _ in 0..8 {
            settings.key(Key::Down);
        }
        let category = settings.category().unwrap();
        for (height, count) in [(16 * 20, 4), (22 * 20, 9)] {
            let window = Rect { height, ..bar(800) };
            let mut scene = Scene::new(Rgb(0, 0, 0), 14, 20);
            settings_category(&mut scene, window, &category, [8, 20]);
            let has_text = |text: &str| {
                scene
                    .glyphs
                    .iter()
                    .any(|glyph| glyph.cluster.text.contains(text))
            };
            assert_eq!(has_text("↑ 6–9 of 9"), count == 4);
            assert_eq!(has_text("Preset"), count == 9);
            assert!(has_text("Up/Down select"));
            let highlighted: Vec<_> = scene
                .glyphs
                .iter()
                .filter(|glyph| glyph.cluster.bold)
                .collect();
            assert_eq!(highlighted.len(), 1);
            assert!(highlighted[0].cluster.text.contains("Frame blue"));
        }
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
