//! Stateless scene helpers for window controls: the tab bar and the pane dividers.

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
        let columns: usize = text.chars().filter_map(UnicodeWidthChar::width).sum();
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
}
