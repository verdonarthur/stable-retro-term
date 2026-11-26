//! The split tree of one tab: pure geometry and focus, no window or terminal types.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PaneId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Side by side, with a vertical divider.
    Columns,
    /// Stacked, with a horizontal divider.
    Rows,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    fn axis(self) -> Axis {
        match self {
            Self::Left | Self::Right => Axis::Columns,
            Self::Up | Self::Down => Axis::Rows,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    fn contains(self, [x, y]: [f64; 2]) -> bool {
        x >= f64::from(self.x)
            && y >= f64::from(self.y)
            && x < f64::from(self.x + self.width)
            && y < f64::from(self.y + self.height)
    }
}

/// Cell size and divider thickness in pixels, minimum pane size in cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub cell: [u32; 2],
    pub divider: u32,
    pub min_cells: [u32; 2],
}

impl Limits {
    fn cell(&self, axis: Axis) -> u32 {
        self.cell[index(axis)].max(1)
    }

    fn fits(&self, rect: Rect) -> bool {
        rect.width / self.cell[0].max(1) >= self.min_cells[0]
            && rect.height / self.cell[1].max(1) >= self.min_cells[1]
    }
}

const DEFAULT_RATIO: f32 = 0.5;

enum Node {
    Leaf(PaneId),
    Split {
        axis: Axis,
        ratio: f32,
        first: Box<Node>,
        second: Box<Node>,
    },
}

pub struct Layout {
    root: Option<Node>,
    focus: PaneId,
}

impl Layout {
    pub fn new(pane: PaneId) -> Self {
        Self {
            root: Some(Node::Leaf(pane)),
            focus: pane,
        }
    }

    pub fn focus(&self) -> PaneId {
        self.focus
    }

    /// The panes in tree order: first child before second child.
    pub fn panes(&self) -> Vec<PaneId> {
        let mut panes = Vec::new();
        if let Some(root) = &self.root {
            root.leaves(&mut panes);
        }
        panes
    }

    pub fn rects(&self, area: Rect, limits: Limits) -> Vec<(PaneId, Rect)> {
        let mut rects = Vec::new();
        if let Some(root) = &self.root {
            root.walk(area, limits, &mut |node, rect| {
                if let Node::Leaf(pane) = node {
                    rects.push((*pane, rect));
                }
            });
        }
        rects
    }

    pub fn dividers(&self, area: Rect, limits: Limits) -> Vec<Rect> {
        let mut dividers = Vec::new();
        if let Some(root) = &self.root {
            root.walk(area, limits, &mut |node, rect| {
                if let Node::Split { axis, ratio, .. } = node {
                    dividers.push(split_rect(rect, *axis, *ratio, limits).1);
                }
            });
        }
        dividers
    }

    /// Splits the focused pane in two halves and focuses `new`.
    /// Refuses when a half would be smaller than `limits.min_cells`.
    pub fn split(&mut self, new: PaneId, axis: Axis, area: Rect, limits: Limits) -> bool {
        let focus = self.focus;
        let Some(rect) = self.rect_of(focus, area, limits) else {
            return false;
        };
        let (first, _, second) = split_rect(rect, axis, DEFAULT_RATIO, limits);
        if !limits.fits(first) || !limits.fits(second) {
            return false;
        }
        let Some(leaf) = self.root.as_mut().and_then(|root| root.find(focus)) else {
            return false;
        };
        *leaf = Node::Split {
            axis,
            ratio: DEFAULT_RATIO,
            first: Box::new(Node::Leaf(focus)),
            second: Box::new(Node::Leaf(new)),
        };
        self.focus = new;
        true
    }

    /// Removes `pane` and puts its sibling in the parent's place.
    /// Returns the focus after the change, or `None` when the layout is empty.
    pub fn close(&mut self, pane: PaneId) -> Option<PaneId> {
        let root = self.root.take()?;
        let (root, sibling) = root.remove(pane);
        self.root = root;
        let root = self.root.as_ref()?;
        if self.focus == pane || !root.contains(self.focus) {
            self.focus = sibling
                .filter(|sibling| root.contains(*sibling))
                .unwrap_or_else(|| root.first());
        }
        Some(self.focus)
    }

    pub fn set_focus(&mut self, pane: PaneId) -> bool {
        let known = self.root.as_ref().is_some_and(|root| root.contains(pane));
        if known {
            self.focus = pane;
        }
        known
    }

    /// Focuses the nearest pane on the `direction` side that shares an edge span with the focus.
    pub fn focus_toward(&mut self, direction: Direction, area: Rect, limits: Limits) -> bool {
        let rects = self.rects(area, limits);
        let Some(&(_, current)) = rects.iter().find(|(pane, _)| *pane == self.focus) else {
            return false;
        };
        let target = rects
            .iter()
            .filter_map(|&(pane, rect)| {
                let (gap, overlap) = neighbour(current, rect, direction)?;
                Some((gap, std::cmp::Reverse(overlap), pane))
            })
            .min();
        match target {
            Some((_, _, pane)) => {
                self.focus = pane;
                true
            }
            None => false,
        }
    }

    /// Moves the nearest divider on the `direction` axis above the focus by one cell.
    pub fn resize(&mut self, direction: Direction, area: Rect, limits: Limits) -> bool {
        let focus = self.focus;
        self.root.as_mut().is_some_and(|root| {
            root.resize(focus, direction, area, limits)
                .is_some_and(|moved| moved)
        })
    }

    /// The pane under a point, or `None` on a divider or outside the area.
    pub fn pane_at(&self, point: [f64; 2], area: Rect, limits: Limits) -> Option<PaneId> {
        self.rects(area, limits)
            .into_iter()
            .find(|(_, rect)| rect.contains(point))
            .map(|(pane, _)| pane)
    }

    fn rect_of(&self, pane: PaneId, area: Rect, limits: Limits) -> Option<Rect> {
        self.rects(area, limits)
            .into_iter()
            .find(|(id, _)| *id == pane)
            .map(|(_, rect)| rect)
    }
}

impl Node {
    fn leaves(&self, panes: &mut Vec<PaneId>) {
        match self {
            Self::Leaf(pane) => panes.push(*pane),
            Self::Split { first, second, .. } => {
                first.leaves(panes);
                second.leaves(panes);
            }
        }
    }

    fn first(&self) -> PaneId {
        match self {
            Self::Leaf(pane) => *pane,
            Self::Split { first, .. } => first.first(),
        }
    }

    fn contains(&self, pane: PaneId) -> bool {
        match self {
            Self::Leaf(leaf) => *leaf == pane,
            Self::Split { first, second, .. } => first.contains(pane) || second.contains(pane),
        }
    }

    fn find(&mut self, pane: PaneId) -> Option<&mut Self> {
        match self {
            Self::Leaf(leaf) if *leaf == pane => Some(self),
            Self::Leaf(_) => None,
            Self::Split { first, second, .. } => first.find(pane).or_else(|| second.find(pane)),
        }
    }

    fn walk(&self, rect: Rect, limits: Limits, visit: &mut impl FnMut(&Self, Rect)) {
        visit(self, rect);
        if let Self::Split {
            axis,
            ratio,
            first,
            second,
        } = self
        {
            let (first_rect, _, second_rect) = split_rect(rect, *axis, *ratio, limits);
            first.walk(first_rect, limits, visit);
            second.walk(second_rect, limits, visit);
        }
    }

    /// Returns the node without `pane` and the first leaf of the subtree that took its parent's place.
    fn remove(self, pane: PaneId) -> (Option<Self>, Option<PaneId>) {
        match self {
            Self::Leaf(leaf) if leaf == pane => (None, None),
            leaf @ Self::Leaf(_) => (Some(leaf), None),
            Self::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                let (first, first_sibling) = first.remove(pane);
                let (second, second_sibling) = second.remove(pane);
                match (first, second) {
                    (Some(first), Some(second)) => (
                        Some(Self::Split {
                            axis,
                            ratio,
                            first: Box::new(first),
                            second: Box::new(second),
                        }),
                        first_sibling.or(second_sibling),
                    ),
                    (Some(only), None) | (None, Some(only)) => {
                        let sibling = only.first();
                        (Some(only), Some(sibling))
                    }
                    (None, None) => (None, None),
                }
            }
        }
    }

    /// `None` when no split on the `direction` axis is above `focus` here;
    /// otherwise whether the nearest one moved.
    fn resize(
        &mut self,
        focus: PaneId,
        direction: Direction,
        rect: Rect,
        limits: Limits,
    ) -> Option<bool> {
        let Self::Split {
            axis,
            ratio,
            first,
            second,
        } = self
        else {
            return None;
        };
        let (first_rect, _, second_rect) = split_rect(rect, *axis, *ratio, limits);
        let inner = if first.contains(focus) {
            first.resize(focus, direction, first_rect, limits)
        } else if second.contains(focus) {
            second.resize(focus, direction, second_rect, limits)
        } else {
            return None;
        };
        if inner.is_some() || *axis != direction.axis() {
            return inner;
        }
        let cell = limits.cell(*axis);
        let available = length(rect, *axis).saturating_sub(limits.divider);
        let current = length(first_rect, *axis);
        let lowest = first.minimum(*axis, limits);
        let highest = available.saturating_sub(second.minimum(*axis, limits));
        let wanted = match direction {
            Direction::Right | Direction::Down => current + cell,
            Direction::Left | Direction::Up => current.saturating_sub(cell),
        };
        if wanted < lowest || wanted > highest || available == 0 {
            return Some(false);
        }
        // Aim at the middle of the cell so that the floor in `split_rect` keeps `wanted`.
        *ratio = (wanted as f32 + cell as f32 / 2.0) / available as f32;
        Some(true)
    }

    /// The smallest length on `axis` that keeps every pane at `min_cells`.
    fn minimum(&self, axis: Axis, limits: Limits) -> u32 {
        match self {
            Self::Leaf(_) => limits.min_cells[index(axis)] * limits.cell(axis),
            Self::Split {
                axis: split,
                first,
                second,
                ..
            } => {
                let first = first.minimum(axis, limits);
                let second = second.minimum(axis, limits);
                if *split == axis {
                    first + limits.divider + second
                } else {
                    first.max(second)
                }
            }
        }
    }
}

fn index(axis: Axis) -> usize {
    match axis {
        Axis::Columns => 0,
        Axis::Rows => 1,
    }
}

fn length(rect: Rect, axis: Axis) -> u32 {
    match axis {
        Axis::Columns => rect.width,
        Axis::Rows => rect.height,
    }
}

/// The first child gets whole cells, the divider follows, the second child gets the rest.
fn split_rect(rect: Rect, axis: Axis, ratio: f32, limits: Limits) -> (Rect, Rect, Rect) {
    let cell = limits.cell(axis);
    let size = length(rect, axis);
    let divider = limits.divider.min(size);
    let available = size - divider;
    let first = ((available as f32 * ratio).floor() as u32 / cell * cell).min(available);
    let second = available - first;
    match axis {
        Axis::Columns => (
            Rect {
                width: first,
                ..rect
            },
            Rect {
                x: rect.x + first,
                width: divider,
                ..rect
            },
            Rect {
                x: rect.x + first + divider,
                width: second,
                ..rect
            },
        ),
        Axis::Rows => (
            Rect {
                height: first,
                ..rect
            },
            Rect {
                y: rect.y + first,
                height: divider,
                ..rect
            },
            Rect {
                y: rect.y + first + divider,
                height: second,
                ..rect
            },
        ),
    }
}

/// `(gap, overlap)` when `candidate` lies on the `direction` side of `current` and shares a span.
fn neighbour(current: Rect, candidate: Rect, direction: Direction) -> Option<(u32, u32)> {
    let span = |start: u32, size: u32, other: u32, other_size: u32| {
        (start + size)
            .min(other + other_size)
            .saturating_sub(start.max(other))
    };
    let (gap, overlap) = match direction {
        Direction::Left if candidate.x + candidate.width <= current.x => (
            current.x - (candidate.x + candidate.width),
            span(current.y, current.height, candidate.y, candidate.height),
        ),
        Direction::Right if candidate.x >= current.x + current.width => (
            candidate.x - (current.x + current.width),
            span(current.y, current.height, candidate.y, candidate.height),
        ),
        Direction::Up if candidate.y + candidate.height <= current.y => (
            current.y - (candidate.y + candidate.height),
            span(current.x, current.width, candidate.x, candidate.width),
        ),
        Direction::Down if candidate.y >= current.y + current.height => (
            candidate.y - (current.y + current.height),
            span(current.x, current.width, candidate.x, candidate.width),
        ),
        _ => return None,
    };
    (overlap > 0).then_some((gap, overlap))
}
