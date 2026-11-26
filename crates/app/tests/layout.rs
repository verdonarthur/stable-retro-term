use retro_app::layout::{Axis, Direction, Layout, Limits, PaneId, Rect};
use retro_app::tabs::Tabs;

const LIMITS: Limits = Limits {
    cell: [8, 20],
    divider: 2,
    min_cells: [10, 3],
};
const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 802,
    height: 600,
};

fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// Panes 0 | 1 on top, 2 | 3 below; focus on 3.
fn two_by_two() -> Layout {
    let mut layout = Layout::new(PaneId(0));
    assert!(layout.split(PaneId(2), Axis::Rows, AREA, LIMITS));
    assert!(layout.set_focus(PaneId(0)));
    assert!(layout.split(PaneId(1), Axis::Columns, AREA, LIMITS));
    assert!(layout.set_focus(PaneId(2)));
    assert!(layout.split(PaneId(3), Axis::Columns, AREA, LIMITS));
    layout
}

fn covered_once(layout: &Layout, area: Rect) {
    let mut pieces: Vec<Rect> = layout
        .rects(area, LIMITS)
        .into_iter()
        .map(|(_, rect)| rect)
        .collect();
    pieces.extend(layout.dividers(area, LIMITS));
    let total: u64 = pieces
        .iter()
        .map(|rect| u64::from(rect.width) * u64::from(rect.height))
        .sum();
    assert_eq!(total, u64::from(area.width) * u64::from(area.height));
    for (index, a) in pieces.iter().enumerate() {
        assert!(a.x + a.width <= area.x + area.width && a.y + a.height <= area.y + area.height);
        for b in &pieces[index + 1..] {
            let overlaps = a.x < b.x + b.width
                && b.x < a.x + a.width
                && a.y < b.y + b.height
                && b.y < a.y + a.height;
            assert!(!overlaps, "{a:?} overlaps {b:?}");
        }
    }
}

#[test]
fn single_pane_fills_the_area() {
    let layout = Layout::new(PaneId(7));
    assert_eq!(layout.rects(AREA, LIMITS), vec![(PaneId(7), AREA)]);
    assert!(layout.dividers(AREA, LIMITS).is_empty());
    assert_eq!(layout.focus(), PaneId(7));
}

#[test]
fn column_split_snaps_first_pane_to_cells_and_leaves_a_divider() {
    let mut layout = Layout::new(PaneId(0));
    assert!(layout.split(PaneId(1), Axis::Columns, AREA, LIMITS));
    assert_eq!(layout.focus(), PaneId(1));
    assert_eq!(
        layout.rects(AREA, LIMITS),
        vec![
            (PaneId(0), rect(0, 0, 400, 600)),
            (PaneId(1), rect(402, 0, 400, 600)),
        ]
    );
    assert_eq!(layout.dividers(AREA, LIMITS), vec![rect(400, 0, 2, 600)]);

    let odd = rect(0, 0, 803, 600);
    let first = layout.rects(odd, LIMITS)[0].1;
    assert_eq!(first.width % 8, 0);
}

#[test]
fn nested_splits_tile_the_area_without_overlap() {
    let layout = two_by_two();
    assert_eq!(layout.panes().len(), 4);
    covered_once(&layout, AREA);
    covered_once(&layout, rect(5, 20, 731, 413));
}

#[test]
fn split_is_refused_below_the_minimum() {
    let narrow = rect(0, 0, 160, 600);
    let mut layout = Layout::new(PaneId(0));
    assert!(!layout.split(PaneId(1), Axis::Columns, narrow, LIMITS));
    assert_eq!(layout.panes(), vec![PaneId(0)]);
    assert_eq!(layout.focus(), PaneId(0));
    let short = rect(0, 0, 802, 100);
    assert!(!layout.split(PaneId(1), Axis::Rows, short, LIMITS));
    assert_eq!(layout.panes(), vec![PaneId(0)]);
}

#[test]
fn close_collapses_the_parent_and_focuses_the_sibling() {
    // 0 | (1 / 2): closing 0 leaves the subtree (1 / 2), focused at its first leaf.
    let mut layout = Layout::new(PaneId(0));
    assert!(layout.split(PaneId(1), Axis::Columns, AREA, LIMITS));
    assert!(layout.split(PaneId(2), Axis::Rows, AREA, LIMITS));
    assert!(layout.set_focus(PaneId(0)));
    assert_eq!(layout.close(PaneId(0)), Some(PaneId(1)));
    assert_eq!(layout.panes(), vec![PaneId(1), PaneId(2)]);
    assert_eq!(layout.dividers(AREA, LIMITS).len(), 1);
    covered_once(&layout, AREA);

    // Closing an unfocused pane keeps the focus.
    assert!(layout.set_focus(PaneId(1)));
    assert_eq!(layout.close(PaneId(2)), Some(PaneId(1)));
    assert_eq!(layout.rects(AREA, LIMITS), vec![(PaneId(1), AREA)]);
}

#[test]
fn closing_the_last_pane_empties_the_layout() {
    let mut layout = Layout::new(PaneId(0));
    assert_eq!(layout.close(PaneId(0)), None);
    assert!(layout.panes().is_empty());
    assert!(layout.rects(AREA, LIMITS).is_empty());
}

#[test]
fn focus_moves_to_the_adjacent_pane() {
    let mut layout = two_by_two();
    assert_eq!(layout.focus(), PaneId(3));
    assert!(!layout.focus_toward(Direction::Right, AREA, LIMITS));
    assert!(!layout.focus_toward(Direction::Down, AREA, LIMITS));
    assert!(layout.focus_toward(Direction::Up, AREA, LIMITS));
    assert_eq!(layout.focus(), PaneId(1));
    assert!(layout.focus_toward(Direction::Left, AREA, LIMITS));
    assert_eq!(layout.focus(), PaneId(0));
    assert!(!layout.focus_toward(Direction::Up, AREA, LIMITS));
    assert!(!layout.focus_toward(Direction::Left, AREA, LIMITS));
    assert!(layout.focus_toward(Direction::Down, AREA, LIMITS));
    assert_eq!(layout.focus(), PaneId(2));
    assert!(layout.focus_toward(Direction::Right, AREA, LIMITS));
    assert_eq!(layout.focus(), PaneId(3));
}

#[test]
fn resize_moves_the_nearest_divider_by_one_cell_and_clamps() {
    let mut layout = Layout::new(PaneId(0));
    assert!(layout.split(PaneId(1), Axis::Columns, AREA, LIMITS));
    assert!(layout.resize(Direction::Right, AREA, LIMITS));
    assert_eq!(layout.rects(AREA, LIMITS)[0].1.width, 408);
    assert!(layout.resize(Direction::Left, AREA, LIMITS));
    assert!(layout.resize(Direction::Left, AREA, LIMITS));
    assert_eq!(layout.rects(AREA, LIMITS)[0].1.width, 392);
    assert!(!layout.resize(Direction::Up, AREA, LIMITS), "no row split");

    while layout.resize(Direction::Left, AREA, LIMITS) {}
    assert_eq!(layout.rects(AREA, LIMITS)[0].1.width, 80);
    while layout.resize(Direction::Right, AREA, LIMITS) {}
    let rects = layout.rects(AREA, LIMITS);
    assert!(rects[1].1.width >= 80);
    assert!(rects[1].1.width < 88);
    covered_once(&layout, AREA);
}

#[test]
fn pane_at_maps_points_and_divider_is_none() {
    let mut layout = Layout::new(PaneId(0));
    assert!(layout.split(PaneId(1), Axis::Columns, AREA, LIMITS));
    assert_eq!(layout.pane_at([0.0, 0.0], AREA, LIMITS), Some(PaneId(0)));
    assert_eq!(
        layout.pane_at([399.9, 599.0], AREA, LIMITS),
        Some(PaneId(0))
    );
    assert_eq!(layout.pane_at([400.0, 10.0], AREA, LIMITS), None);
    assert_eq!(layout.pane_at([401.5, 10.0], AREA, LIMITS), None);
    assert_eq!(layout.pane_at([402.0, 10.0], AREA, LIMITS), Some(PaneId(1)));
    assert_eq!(layout.pane_at([802.0, 10.0], AREA, LIMITS), None);
    assert_eq!(layout.pane_at([-1.0, 10.0], AREA, LIMITS), None);
}

#[test]
fn window_resize_keeps_ratios() {
    let mut layout = Layout::new(PaneId(0));
    assert!(layout.split(PaneId(1), Axis::Columns, AREA, LIMITS));
    let large = rect(0, 0, 1602, 1200);
    assert_eq!(
        layout.rects(large, LIMITS),
        vec![
            (PaneId(0), rect(0, 0, 800, 1200)),
            (PaneId(1), rect(802, 0, 800, 1200)),
        ]
    );
}

#[test]
fn repeated_split_and_close_returns_to_one_pane() {
    let (mut tabs, first) = Tabs::new();
    let mut seen = vec![first];
    for cycle in 0..100 {
        let axis = if cycle % 2 == 0 {
            Axis::Columns
        } else {
            Axis::Rows
        };
        let pane = tabs.split(axis, AREA, LIMITS).expect("split");
        assert!(!seen.contains(&pane), "id {pane:?} reused");
        seen.push(pane);
        assert!(!tabs.close(pane));
    }
    assert_eq!(tabs.active().layout.panes(), vec![first]);
    assert_eq!(tabs.focused(), first);
}

#[test]
fn tabs_open_after_active_and_activate() {
    let (mut tabs, first) = Tabs::new();
    let second = tabs.open();
    assert_eq!(tabs.active_index(), 1);
    assert!(tabs.select(0));
    let third = tabs.open();
    assert_eq!(tabs.active_index(), 1);
    assert_eq!(tabs.focused(), third);
    let order: Vec<_> = tabs.iter().map(|tab| tab.layout.focus()).collect();
    assert_eq!(order, vec![first, third, second]);
    assert!(!tabs.select(3));
}

#[test]
fn next_and_previous_wrap() {
    let (mut tabs, _) = Tabs::new();
    tabs.open();
    tabs.open();
    assert_eq!(tabs.active_index(), 2);
    tabs.next();
    assert_eq!(tabs.active_index(), 0);
    tabs.previous();
    assert_eq!(tabs.active_index(), 2);
}

#[test]
fn closing_a_tab_activates_the_next_then_previous() {
    let (mut tabs, first) = Tabs::new();
    let second = tabs.open();
    let third = tabs.open();
    assert!(tabs.select(1));
    assert!(!tabs.close(second));
    assert_eq!(tabs.focused(), third);
    assert!(!tabs.close(third));
    assert_eq!(tabs.focused(), first);
    assert_eq!(tabs.len(), 1);

    let fourth = tabs.open();
    assert!(tabs.select(1));
    assert!(!tabs.close(first));
    assert_eq!(tabs.active_index(), 0);
    assert_eq!(tabs.focused(), fourth);
}

#[test]
fn closing_the_last_pane_of_the_last_tab_reports_empty() {
    let (mut tabs, first) = Tabs::new();
    let second = tabs.split(Axis::Columns, AREA, LIMITS).unwrap();
    assert!(!tabs.close(first));
    assert!(tabs.close(second));
    assert!(tabs.is_empty());
}

#[test]
fn only_active_tab_panes_are_visible() {
    let (mut tabs, first) = Tabs::new();
    let split = tabs.split(Axis::Rows, AREA, LIMITS).unwrap();
    let other = tabs.open();
    assert!(tabs.is_visible(other));
    assert!(!tabs.is_visible(first));
    assert!(!tabs.is_visible(split));
    tabs.previous();
    assert!(tabs.is_visible(first) && tabs.is_visible(split));
    assert!(!tabs.is_visible(other));
    assert_eq!(tabs.tab_of(other), Some(1));
}
