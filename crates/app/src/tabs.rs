//! The tabs of one window. Each tab owns a split tree of panes.

use crate::layout::{Axis, Layout, Limits, PaneId, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TabId(pub u64);

pub struct Tab {
    pub id: TabId,
    pub layout: Layout,
}

/// Always holds at least one tab until `close` reports that the last one is gone.
pub struct Tabs {
    tabs: Vec<Tab>,
    active: usize,
    next_id: u64,
}

impl Tabs {
    pub fn new() -> (Self, PaneId) {
        let mut tabs = Self {
            tabs: Vec::new(),
            active: 0,
            next_id: 0,
        };
        let pane = tabs.open();
        (tabs, pane)
    }

    /// Opens a tab after the active one and activates it.
    pub fn open(&mut self) -> PaneId {
        let id = TabId(self.allocate());
        let pane = PaneId(self.allocate());
        let index = if self.tabs.is_empty() {
            0
        } else {
            self.active + 1
        };
        self.tabs.insert(
            index,
            Tab {
                id,
                layout: Layout::new(pane),
            },
        );
        self.active = index;
        pane
    }

    /// Splits the focused pane of the active tab. `None` when the split is refused.
    pub fn split(&mut self, axis: Axis, area: Rect, limits: Limits) -> Option<PaneId> {
        let pane = PaneId(self.next_id);
        let tab = self.tabs.get_mut(self.active)?;
        tab.layout.split(pane, axis, area, limits).then(|| {
            self.next_id += 1;
            pane
        })
    }

    /// Removes `pane`; an empty tab is removed and the next tab, else the previous one, becomes
    /// active. Returns true when no tab is left.
    pub fn close(&mut self, pane: PaneId) -> bool {
        if let Some(index) = self.tab_of(pane)
            && self.tabs[index].layout.close(pane).is_none()
        {
            self.tabs.remove(index);
            if index < self.active || self.active >= self.tabs.len() {
                self.active = self.active.saturating_sub(1);
            }
        }
        self.tabs.is_empty()
    }

    pub fn select(&mut self, index: usize) -> bool {
        let known = index < self.tabs.len();
        if known {
            self.active = index;
        }
        known
    }

    pub fn next(&mut self) {
        self.active = (self.active + 1) % self.tabs.len().max(1);
    }

    pub fn previous(&mut self) {
        let len = self.tabs.len().max(1);
        self.active = (self.active + len - 1) % len;
    }

    pub fn active(&self) -> &Tab {
        &self.tabs[self.active]
    }

    pub fn active_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn focused(&self) -> PaneId {
        self.active().layout.focus()
    }

    /// True when `pane` is in the active tab.
    pub fn is_visible(&self, pane: PaneId) -> bool {
        self.tab_of(pane) == Some(self.active)
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Tab> {
        self.tabs.iter()
    }

    pub fn tab_of(&self, pane: PaneId) -> Option<usize> {
        self.tabs
            .iter()
            .position(|tab| tab.layout.panes().contains(&pane))
    }

    fn allocate(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}
