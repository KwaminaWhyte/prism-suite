//! Navigator panel + multi-document tabs — pure data model.
//!
//! Two related view-management concerns:
//!   * **Document tabs**: an ordered list of open documents with an active index,
//!     each carrying display metadata (title, doc size, thumbnail bounds).
//!   * **Navigator**: a proxy viewport rect over the document used by the small
//!     bird's-eye panel — pan re-centers it, zoom rescales it, both clamped to
//!     the document bounds.
//!
//! No GPU, no I/O: thumbnails are represented as bounds (the renderer fills the
//! pixels). Geometry helpers are deterministic and unit tested.

use super::{App, Action};

/// One open document tab.
#[derive(Clone, Debug, PartialEq)]
pub struct DocTab {
    pub id: u64,
    pub title: String,
    /// Document pixel size `[w, h]`.
    pub doc_size: [u32; 2],
    /// Whether the document has unsaved changes (shows a dot in the tab).
    pub dirty: bool,
    /// On-disk path, if the document has been saved.
    pub path: Option<String>,
}

impl DocTab {
    /// Thumbnail bounds `[x, y, w, h]` fitting the doc's aspect ratio into a
    /// `max_w × max_h` box, top-left anchored. Used by the tab strip preview.
    pub fn thumbnail_bounds(&self, max_w: f32, max_h: f32) -> [f32; 4] {
        let (dw, dh) = (self.doc_size[0].max(1) as f32, self.doc_size[1].max(1) as f32);
        let scale = (max_w / dw).min(max_h / dh);
        let w = dw * scale;
        let h = dh * scale;
        // Center within the box.
        let x = (max_w - w) * 0.5;
        let y = (max_h - h) * 0.5;
        [x, y, w, h]
    }
}

/// The tab-bar model: open documents + active index.
#[derive(Clone, Debug, Default)]
pub struct DocTabs {
    pub tabs: Vec<DocTab>,
    /// Index of the active tab (only meaningful when `tabs` is non-empty).
    pub active: usize,
    pub next_id: u64,
}

impl DocTabs {
    /// Open a new tab and make it active. Returns its id.
    pub fn open(&mut self, title: impl Into<String>, doc_size: [u32; 2]) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.push(DocTab {
            id,
            title: title.into(),
            doc_size,
            dirty: false,
            path: None,
        });
        self.active = self.tabs.len() - 1;
        id
    }

    /// Close a tab by id. Adjusts `active` so it still points at a valid tab.
    pub fn close(&mut self, id: u64) {
        let Some(idx) = self.tabs.iter().position(|t| t.id == id) else {
            return;
        };
        self.tabs.remove(idx);
        if self.tabs.is_empty() {
            self.active = 0;
        } else if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        } else if idx < self.active {
            // A tab before the active one was removed — shift active left.
            self.active -= 1;
        }
        // If idx == active and active is now still in range, it points at the
        // tab that shifted into the slot (the next document), which is correct.
    }

    /// Switch the active tab to the one with `id`. Returns true on success.
    pub fn activate(&mut self, id: u64) -> bool {
        if let Some(idx) = self.tabs.iter().position(|t| t.id == id) {
            self.active = idx;
            true
        } else {
            false
        }
    }

    /// Switch by ordinal index (clamped). Returns true if in range.
    pub fn activate_index(&mut self, idx: usize) -> bool {
        if idx < self.tabs.len() {
            self.active = idx;
            true
        } else {
            false
        }
    }

    /// The active tab, if any.
    pub fn active_tab(&self) -> Option<&DocTab> {
        self.tabs.get(self.active)
    }

    /// Mutable access to the active tab.
    pub fn active_tab_mut(&mut self) -> Option<&mut DocTab> {
        self.tabs.get_mut(self.active)
    }

    /// Reorder a tab from `from` to `to` (both clamped); active follows the moved
    /// tab if it was the active one.
    pub fn reorder(&mut self, from: usize, to: usize) {
        if from >= self.tabs.len() || to >= self.tabs.len() || from == to {
            return;
        }
        let was_active_id = self.active_tab().map(|t| t.id);
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        if let Some(id) = was_active_id {
            self.active = self.tabs.iter().position(|t| t.id == id).unwrap_or(self.active);
        }
    }
}

/// The navigator proxy viewport: a rect over the document in doc-space px,
/// representing the area currently visible in the main canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NavigatorView {
    /// Document size `[w, h]` the viewport is relative to.
    pub doc_size: [f32; 2],
    /// Visible rect `[x, y, w, h]` in doc px.
    pub view_rect: [f32; 4],
    /// Zoom factor (1.0 = 100%). Larger = more zoomed in = smaller view_rect.
    pub zoom: f32,
}

impl NavigatorView {
    /// A navigator fitting the whole document at 100% zoom.
    pub fn fit(doc_w: f32, doc_h: f32) -> Self {
        Self {
            doc_size: [doc_w.max(1.0), doc_h.max(1.0)],
            view_rect: [0.0, 0.0, doc_w.max(1.0), doc_h.max(1.0)],
            zoom: 1.0,
        }
    }

    /// Set the zoom factor, recomputing the viewport rect centered on its current
    /// center. Zoom is clamped to a sane range. At zoom 1, the viewport equals
    /// the whole document; at zoom 2 it covers half each dimension, etc.
    pub fn set_zoom(&mut self, zoom: f32) {
        let z = zoom.clamp(0.01, 64.0);
        let (cx, cy) = self.center();
        self.zoom = z;
        let vw = (self.doc_size[0] / z).min(self.doc_size[0]);
        let vh = (self.doc_size[1] / z).min(self.doc_size[1]);
        self.view_rect[2] = vw;
        self.view_rect[3] = vh;
        self.set_center(cx, cy);
    }

    /// Current center of the viewport rect in doc px.
    pub fn center(&self) -> (f32, f32) {
        (
            self.view_rect[0] + self.view_rect[2] * 0.5,
            self.view_rect[1] + self.view_rect[3] * 0.5,
        )
    }

    /// Re-center the viewport on `(cx, cy)`, clamped so it stays inside the doc.
    pub fn set_center(&mut self, cx: f32, cy: f32) {
        let vw = self.view_rect[2];
        let vh = self.view_rect[3];
        let max_x = (self.doc_size[0] - vw).max(0.0);
        let max_y = (self.doc_size[1] - vh).max(0.0);
        self.view_rect[0] = (cx - vw * 0.5).clamp(0.0, max_x);
        self.view_rect[1] = (cy - vh * 0.5).clamp(0.0, max_y);
    }

    /// Pan the viewport by `(dx, dy)` doc px (clamped to the document).
    pub fn pan(&mut self, dx: f32, dy: f32) {
        let (cx, cy) = self.center();
        self.set_center(cx + dx, cy + dy);
    }
}

impl App {
    /// Dispatch for the navigator/tabs domain actions.
    pub(super) fn apply_navigator(&mut self, action: Action) {
        match action {
            Action::OpenDocTab { title, width, height } => {
                self.doc_tabs.open(title, [width, height]);
            }
            Action::CloseDocTab(id) => {
                self.doc_tabs.close(id);
            }
            Action::ActivateDocTab(id) => {
                self.doc_tabs.activate(id);
            }
            Action::ActivateDocTabIndex(idx) => {
                self.doc_tabs.activate_index(idx);
            }
            Action::ReorderDocTab { from, to } => {
                self.doc_tabs.reorder(from, to);
            }
            Action::SetDocTabDirty { id, dirty } => {
                if let Some(t) = self.doc_tabs.tabs.iter_mut().find(|t| t.id == id) {
                    t.dirty = dirty;
                }
            }
            Action::NavigatorZoom(z) => {
                self.navigator.set_zoom(z);
            }
            Action::NavigatorPan { dx, dy } => {
                self.navigator.pan(dx, dy);
            }
            Action::NavigatorCenter { x, y } => {
                self.navigator.set_center(x, y);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_makes_active_and_returns_id() {
        let mut t = DocTabs::default();
        let a = t.open("Untitled-1", [800, 600]);
        let b = t.open("Untitled-2", [1024, 768]);
        assert_eq!(t.tabs.len(), 2);
        assert_eq!(t.active, 1);
        assert_eq!(t.active_tab().unwrap().id, b);
        assert_ne!(a, b);
    }

    #[test]
    fn activate_switches_tab() {
        let mut t = DocTabs::default();
        let a = t.open("A", [10, 10]);
        let _b = t.open("B", [10, 10]);
        assert_eq!(t.active, 1);
        assert!(t.activate(a));
        assert_eq!(t.active, 0);
        assert_eq!(t.active_tab().unwrap().title, "A");
    }

    #[test]
    fn activate_index_clamped() {
        let mut t = DocTabs::default();
        t.open("A", [10, 10]);
        assert!(t.activate_index(0));
        assert!(!t.activate_index(5)); // out of range
        assert_eq!(t.active, 0);
    }

    #[test]
    fn close_adjusts_active() {
        let mut t = DocTabs::default();
        let a = t.open("A", [10, 10]);
        let _b = t.open("B", [10, 10]);
        let c = t.open("C", [10, 10]);
        // active = 2 (C). Close A (idx 0) → active shifts left to 1, still C.
        t.close(a);
        assert_eq!(t.tabs.len(), 2);
        assert_eq!(t.active_tab().unwrap().id, c);
        // Close C (the active one) → active clamps to last remaining (B).
        t.close(c);
        assert_eq!(t.tabs.len(), 1);
        assert_eq!(t.active, 0);
    }

    #[test]
    fn close_last_resets_active() {
        let mut t = DocTabs::default();
        let a = t.open("A", [10, 10]);
        t.close(a);
        assert!(t.tabs.is_empty());
        assert_eq!(t.active, 0);
        assert!(t.active_tab().is_none());
    }

    #[test]
    fn reorder_keeps_active() {
        let mut t = DocTabs::default();
        let a = t.open("A", [10, 10]);
        let _b = t.open("B", [10, 10]);
        let _c = t.open("C", [10, 10]);
        t.activate(a); // active = A at index 0
        t.reorder(0, 2); // move A to the end
        assert_eq!(t.active_tab().unwrap().id, a);
        assert_eq!(t.tabs[2].id, a);
    }

    #[test]
    fn thumbnail_bounds_preserve_aspect() {
        let tab = DocTab {
            id: 0,
            title: "x".into(),
            doc_size: [200, 100],
            dirty: false,
            path: None,
        };
        let [_, _, w, h] = tab.thumbnail_bounds(64.0, 64.0);
        // 2:1 aspect → fits to 64×32.
        assert!((w - 64.0).abs() < 1e-3);
        assert!((h - 32.0).abs() < 1e-3);
    }

    #[test]
    fn navigator_fit_covers_document() {
        let n = NavigatorView::fit(1000.0, 500.0);
        assert_eq!(n.view_rect, [0.0, 0.0, 1000.0, 500.0]);
        assert_eq!(n.zoom, 1.0);
    }

    #[test]
    fn navigator_zoom_shrinks_viewport() {
        let mut n = NavigatorView::fit(1000.0, 500.0);
        n.set_zoom(2.0);
        // At 2x, viewport covers half each dimension, centered.
        assert!((n.view_rect[2] - 500.0).abs() < 1e-3);
        assert!((n.view_rect[3] - 250.0).abs() < 1e-3);
        let (cx, cy) = n.center();
        assert!((cx - 500.0).abs() < 1e-3);
        assert!((cy - 250.0).abs() < 1e-3);
    }

    #[test]
    fn navigator_pan_clamped_to_doc() {
        let mut n = NavigatorView::fit(1000.0, 500.0);
        n.set_zoom(2.0); // viewport 500×250
        // Pan far right — should clamp so the viewport stays inside the doc.
        n.pan(10000.0, 0.0);
        assert!((n.view_rect[0] - 500.0).abs() < 1e-3); // 1000 - 500
        // Pan far left — clamps to 0.
        n.pan(-10000.0, 0.0);
        assert!(n.view_rect[0].abs() < 1e-3);
    }

    #[test]
    fn apply_tab_actions() {
        let mut app = App::new();
        app.apply(Action::OpenDocTab {
            title: "Doc1".into(),
            width: 640,
            height: 480,
        });
        app.apply(Action::OpenDocTab {
            title: "Doc2".into(),
            width: 800,
            height: 600,
        });
        assert_eq!(app.doc_tabs.tabs.len(), 2);
        assert_eq!(app.doc_tabs.active, 1);
        let first_id = app.doc_tabs.tabs[0].id;
        app.apply(Action::ActivateDocTab(first_id));
        assert_eq!(app.doc_tabs.active, 0);
        assert_eq!(app.doc_tabs.active_tab().unwrap().title, "Doc1");
    }
}
