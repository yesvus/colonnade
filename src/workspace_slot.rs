//! One slot in the bar per visible workspace (see BEHAVIOR.md's "Layout"
//! section) -- either a collapsed glyph marker or, for the one bloomed
//! workspace on this output, a full group of tabs. Uses `gtk::Stack`
//! rather than `animate.rs`'s hand-rolled tick-callback primitive for the
//! bloom/collapse transition itself: `Stack` already animates both the
//! crossfade *and* the resize between differently-sized children natively
//! -- a well-tested GTK mechanism, not something worth re-deriving.
//! `animate.rs` is still what drives each individual tab's width once
//! bloomed (see `tab.rs`).
//!
//! Overflow: a fixed number of visible tab slots, sliced and centered on
//! the current column, matching the already-proven Python daemon this
//! replaces -- not a GtkScrolledWindow letting GTK auto-size/scroll
//! arbitrary content. That was tried first and abandoned: GTK3's
//! content-size propagation through nested containers (ScrolledWindow
//! inside Stack inside Box) is a known-fragile mechanism, and it showed
//! exactly that fragility here (the width cap silently didn't apply,
//! confirmed by screenshot with 31 real windows on one workspace). Fixed
//! slots avoids that whole mechanism rather than fighting it.

use std::{collections::BTreeMap, fmt::Debug};

use waybar_cffi::gtk::{
    self,
    gdk::ScrollDirection,
    glib::{self, Cast},
    prelude::{BoxExt, ContainerExt, LabelExt, StackExt, StyleContextExt, WidgetExt},
};

use crate::{
    column::{self, Column},
    glyph,
    niri::{Window, WorkspaceInfo},
    state::State,
    tab::Tab,
};

const MARKER: &str = "marker";
const GROUP: &str = "group";

/// How many tabs are ever visible at once, sliced and centered on the
/// current column when there are more than this. Matches the Python
/// daemon's `max_slots` default exactly.
const MAX_VISIBLE_TABS: usize = 6;

pub struct WorkspaceSlot {
    stack: gtk::Stack,
    marker: gtk::Label,
    group: gtk::Box,
    /// Workspace idx (or name), shown before the first tab -- the bloomed
    /// view otherwise has no cue at all for which workspace you're
    /// looking at. Matches niri-workspaces-rs's original convention of
    /// prefixing the focused workspace's own glyph row the same way.
    number: gtk::Label,
    /// Non-interactive glyph ticks (same vocabulary as collapsed-workspace
    /// markers -- see `glyph.rs`), shown at either end when tabs are
    /// sliced out of view, so there's a hint that more exists without a
    /// numeral competing visually with real tabs. Matches the Python
    /// daemon's pagination instinct without yet building the real fade
    /// treatment (Phase 3).
    overflow_left: gtk::Label,
    overflow_right: gtk::Label,
    tabs: BTreeMap<u64, Tab>,
    state: State,
    workspace_id: u64,
}

impl Debug for WorkspaceSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkspaceSlot")
            .field("workspace_id", &self.workspace_id)
            .finish()
    }
}

impl WorkspaceSlot {
    pub fn new(state: &State, workspace_id: u64) -> Self {
        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        stack.set_transition_duration(150); // matches animate.rs's DURATION_MS
        // GtkStack defaults to sizing itself for its *largest* child
        // regardless of which is visible -- without this, a collapsed
        // marker's slot silently claims the bloomed group's full width
        // anyway, spreading every other slot out across invisible empty
        // space instead of packing tight to the left. (Found by looking
        // at it live: layout looked broken/spread out, not left-aligned.)
        stack.set_hhomogeneous(false);
        stack.style_context().add_class("workspace-slot");

        let marker = gtk::Label::new(None);
        marker.style_context().add_class("workspace-marker");
        let marker_box = gtk::EventBox::new();
        marker_box.add(&marker);
        stack.add_named(&marker_box, MARKER);

        let group = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        group.style_context().add_class("workspace-group");
        stack.add_named(&group, GROUP);

        let number = gtk::Label::new(None);
        number.style_context().add_class("workspace-number");
        group.add(&number);

        let overflow_left = gtk::Label::new(None);
        overflow_left.style_context().add_class("overflow-left");
        group.add(&overflow_left);

        let overflow_right = gtk::Label::new(None);
        overflow_right.style_context().add_class("overflow-right");
        group.add(&overflow_right);

        let slot = Self {
            stack,
            marker,
            group,
            number,
            overflow_left,
            overflow_right,
            tabs: BTreeMap::new(),
            state: state.clone(),
            workspace_id,
        };

        slot.connect_marker_click();
        slot.connect_scroll();
        slot
    }

    pub fn widget(&self) -> &gtk::Stack {
        &self.stack
    }

    fn connect_marker_click(&self) {
        let state = self.state.clone();
        let workspace_id = self.workspace_id;

        if let Some(marker_box) = self
            .stack
            .child_by_name(MARKER)
            .and_then(|w| w.downcast::<gtk::EventBox>().ok())
        {
            marker_box.connect_button_release_event(move |_, _| {
                if let Err(e) = state.niri().focus_workspace(workspace_id) {
                    tracing::warn!(%e, id = workspace_id, "error switching workspace");
                }
                glib::Propagation::Stop
            });
        }
    }

    /// Scroll anywhere on the strip -- BEHAVIOR.md: `focus-column-left` /
    /// `focus-column-right`. Purely a niri action, no local view-state to
    /// update: the next snapshot re-centers the visible slice on whatever
    /// column becomes current, the same way the Python daemon's on-scroll
    /// handler worked.
    fn connect_scroll(&self) {
        let state = self.state.clone();

        self.group.connect_scroll_event(move |_, event| {
            let going_left = match event.direction() {
                ScrollDirection::Up | ScrollDirection::Left => Some(true),
                ScrollDirection::Down | ScrollDirection::Right => Some(false),
                ScrollDirection::Smooth => {
                    let (dx, dy) = event.delta();
                    let delta = if dx.abs() > dy.abs() { dx } else { dy };
                    (delta.abs() > f64::EPSILON).then_some(delta < 0.0)
                }
                _ => None,
            };
            let Some(going_left) = going_left else {
                return glib::Propagation::Proceed;
            };

            let result = if going_left {
                state.niri().focus_column_left()
            } else {
                state.niri().focus_column_right()
            };
            if let Err(e) = result {
                tracing::warn!(%e, "error scrolling columns");
            }
            glib::Propagation::Stop
        });
    }

    /// Renders this workspace as bloomed: a workspace-number label,
    /// followed by up to `MAX_VISIBLE_TABS` tabs sliced and centered on
    /// the current column.
    pub fn set_bloomed(&mut self, workspace: &WorkspaceInfo, windows: &[Window], output_width: f64) {
        let label = workspace
            .name
            .as_deref()
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| workspace.idx.to_string());
        self.number.set_text(&label);

        let columns = column::group(windows, output_width);
        let total = columns.len();

        let current_idx = columns
            .iter()
            .position(|c| c.window.is_focused || Some(c.window.id) == workspace.active_window_id)
            .unwrap_or(0);

        let (start, visible): (usize, &[Column<'_>]) = if total <= MAX_VISIBLE_TABS {
            (0, &columns)
        } else {
            let start = current_idx
                .saturating_sub(MAX_VISIBLE_TABS / 2)
                .min(total - MAX_VISIBLE_TABS);
            (start, &columns[start..start + MAX_VISIBLE_TABS])
        };
        // Windows outside the visible slice render as the same glyph tick
        // marks collapsed workspaces use, not a "+N" numeral -- one visual
        // language for "more windows exist here" everywhere it appears,
        // rather than two different indicator styles.
        let left_text: String = columns[..start]
            .iter()
            .map(|c| glyph::glyph_for(workspace, c.window))
            .collect();
        let right_text: String = columns[start + visible.len()..]
            .iter()
            .map(|c| glyph::glyph_for(workspace, c.window))
            .collect();
        self.overflow_left.set_text(&left_text);
        self.overflow_right.set_text(&right_text);

        let mut seen = std::collections::BTreeSet::new();

        // Position 0 = workspace number, 1 = left overflow count, then
        // tabs, then the right overflow count comes last (reordered to
        // -1, i.e. end-of-box, after the loop).
        for (i, col) in visible.iter().enumerate() {
            seen.insert(col.window.id);
            match self.tabs.get(&col.window.id) {
                Some(existing) => existing.update(col),
                None => {
                    let tab = Tab::new(&self.state, col);
                    self.group.add(tab.widget());
                    self.tabs.insert(col.window.id, tab);
                }
            }
            if let Some(tab) = self.tabs.get(&col.window.id) {
                self.group.reorder_child(tab.widget(), (i + 2) as i32);
            }
        }
        self.group.reorder_child(&self.overflow_right, -1);

        self.tabs.retain(|id, tab| {
            if seen.contains(id) {
                true
            } else {
                self.group.remove(tab.widget());
                false
            }
        });

        self.group.show_all();
        self.stack.set_visible_child_name(GROUP);
    }

    /// Renders this workspace as collapsed: a compact, non-interactive
    /// (per-window) glyph marker. `windows` should be every window on this
    /// output, not pre-filtered -- `glyph::marker_text` filters by
    /// workspace itself.
    pub fn set_collapsed(&mut self, workspace: &WorkspaceInfo, windows: &[Window]) {
        let text = glyph::marker_text(workspace, windows);
        // An empty, focused (bloom-eligible) workspace is handled by
        // set_bloomed's caller instead; a collapsed workspace with no
        // windows is never shown at all -- the caller doesn't create a
        // slot for it. Still guard here rather than show a blank marker if
        // that assumption is ever violated.
        self.marker
            .set_text(if text.is_empty() { "·" } else { &text });
        self.stack.set_visible_child_name(MARKER);
    }
}
