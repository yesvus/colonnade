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
//! Overflow: a real pixel-width budget (`Config::max_group_width_px()`),
//! not a fixed tab count and not a GtkScrolledWindow letting GTK auto-
//! size/scroll arbitrary content. The GtkScrolledWindow approach was
//! tried first and abandoned: GTK3's content-size propagation through
//! nested containers (ScrolledWindow inside Stack inside Box) is a
//! known-fragile mechanism, and it showed exactly that fragility here
//! (the width cap silently didn't apply, confirmed by screenshot with 31
//! real windows on one workspace). A fixed *count* was tried next and
//! also replaced: it could still let a handful of wide tabs blow past
//! the screen. The current approach (`expand_right` below) greedily
//! includes columns by their real, unshrunk width until the budget is
//! spent, anchored on the current column and shifted symmetrically, with
//! zero lookahead, only once focus reaches the visible slice's own edge
//! tab in either direction.

use std::{collections::BTreeMap, fmt::Debug};

use waybar_cffi::gtk::{
    self,
    gdk::{self, ScrollDirection},
    glib,
    prelude::{
        BoxExt, ContainerExt, LabelExt, StackExt, StyleContextExt, WidgetExt, WidgetExtManual,
    },
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

/// Greedily includes columns rightward from `start` while the running
/// total stays within `budget_px`. Always includes at least the starting
/// column even if it alone exceeds budget, so this never returns an
/// empty range for a non-empty input.
fn expand_right(columns: &[Column<'_>], start: usize, budget_px: i32) -> usize {
    let mut width = 0;
    let mut end = start;
    while end < columns.len() {
        let w = columns[end].target_width_px;
        if end > start && width + w > budget_px {
            break;
        }
        width += w;
        end += 1;
    }
    end
}

/// Workspace idx, or its custom name if it has one. Shown always now, both
/// bloomed and collapsed -- there was previously no way to tell which
/// workspace a collapsed marker even was.
fn workspace_label(workspace: &WorkspaceInfo) -> String {
    workspace
        .name
        .as_deref()
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| workspace.idx.to_string())
}

pub struct WorkspaceSlot {
    /// The slot itself: the workspace number, then the stack that swaps
    /// between collapsed glyphs and bloomed tabs.
    root: gtk::Box,
    stack: gtk::Stack,
    marker: gtk::Label,
    group: gtk::Box,
    /// Workspace idx (or name), shown in both states -- there is otherwise
    /// no cue at all for which workspace you're looking at. Matches
    /// niri-workspaces-rs's original convention of prefixing the focused
    /// workspace's own glyph row the same way.
    ///
    /// Deliberately a sibling of `stack`, not a child of either stack
    /// page. It used to be both: a label inside `group` when bloomed, and
    /// a prefix baked into `marker`'s own text when collapsed. Two
    /// widgets meant two sets of horizontal padding for the same visible
    /// number, so switching workspaces made the number jump sideways by
    /// the difference (measured 2px: the group label's 4px inset versus
    /// the marker's 6px). One widget outside the stack can't drift from
    /// itself, whatever either page's padding later becomes.
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
    /// The window id anchoring the left edge of the visible slice.
    /// Persisted across renders rather than recomputed by centering on
    /// the current column every time -- centering on every focus change
    /// meant the current tab always sat at the same fixed offset (e.g.
    /// position 4 of 6), so the slice shifted far more eagerly than it
    /// should have. With a persisted anchor, the slice only moves once
    /// focus actually reaches its trailing edge (see `set_bloomed`).
    anchor: Option<u64>,
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

        let marker = gtk::Label::new(None);
        marker.style_context().add_class("workspace-marker");
        let marker_box = gtk::EventBox::new();
        marker_box.add(&marker);
        stack.add_named(&marker_box, MARKER);

        let group = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        group.style_context().add_class("workspace-group");
        // GtkBox is a "windowless" widget in GTK3 -- no GdkWindow of its
        // own, so it structurally cannot receive pointer/scroll events no
        // matter what's connected to it. Same reason the marker needs an
        // EventBox (above); missed it here originally, which is why
        // mouse-wheel scrolling silently did nothing at all.
        let group_events = gtk::EventBox::new();
        // EventBox alone wasn't enough either: GTK3's default widget event
        // mask doesn't include scroll (button press/release get connected
        // automatically when you hook connect_button_*_event, but scroll
        // apparently doesn't get the same treatment) -- has to be
        // requested explicitly, or the EventBox's own GdkWindow never asks
        // the compositor for scroll events in the first place, and no
        // scroll signal ever fires no matter what's connected to it.
        group_events.add_events(gdk::EventMask::SCROLL_MASK | gdk::EventMask::SMOOTH_SCROLL_MASK);
        group_events.add(&group);
        stack.add_named(&group_events, GROUP);

        let number = gtk::Label::new(None);
        number.style_context().add_class("workspace-number");
        // Same EventBox reasoning as the marker and the group: a bare
        // GtkLabel has no GdkWindow, so it can't take clicks or scrolls
        // itself. The number needs both, because it used to inherit them
        // from whichever page it lived in (clicking a collapsed marker's
        // number focused that workspace; scrolling over the bloomed
        // number scrolled columns, since it sat inside the group's own
        // EventBox), and pulling it out of the stack would silently drop
        // both.
        let number_events = gtk::EventBox::new();
        number_events.add_events(gdk::EventMask::SCROLL_MASK | gdk::EventMask::SMOOTH_SCROLL_MASK);
        number_events.add(&number);

        let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        root.style_context().add_class("workspace-slot");
        root.add(&number_events);
        root.add(&stack);

        // An empty GtkLabel is still an allocated widget: with no text it
        // kept its stylesheet padding, so with nothing overflowing, the
        // left tick silently sat between the workspace number and the
        // first tab as several pixels of nothing, making that gap wider
        // than the equivalent gap in a collapsed marker (which is one
        // literal space character). Hidden when empty instead, so the
        // spacing is only ever the number's own padding. no_show_all
        // because `group.show_all()` at the end of every render would
        // otherwise put them straight back.
        let overflow_left = gtk::Label::new(None);
        overflow_left.style_context().add_class("overflow-left");
        overflow_left.set_no_show_all(true);
        group.add(&overflow_left);

        let overflow_right = gtk::Label::new(None);
        overflow_right.style_context().add_class("overflow-right");
        overflow_right.set_no_show_all(true);
        group.add(&overflow_right);

        let slot = Self {
            root,
            stack,
            marker,
            group,
            number,
            overflow_left,
            overflow_right,
            tabs: BTreeMap::new(),
            anchor: None,
            state: state.clone(),
            workspace_id,
        };

        slot.connect_focus_click(&marker_box);
        slot.connect_focus_click(&number_events);
        slot.connect_scroll(&group_events);
        slot.connect_scroll(&number_events);
        slot
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    /// Bright number for the bloomed workspace, dim for collapsed ones.
    /// This used to come for free from which widget the number lived in
    /// (`.workspace-number` when bloomed, the dim `.workspace-marker`'s
    /// own text when collapsed). Now that it's one widget in both states,
    /// the distinction has to be said out loud -- same "focused" class
    /// `tab.rs` uses, so the stylesheet keeps one vocabulary for "this is
    /// the active one".
    fn set_number_focused(&self, focused: bool) {
        let context = self.number.style_context();
        if focused {
            context.add_class("focused");
        } else {
            context.remove_class("focused");
        }
    }

    /// Sets both overflow ticks, showing each only when it actually has
    /// glyphs -- see the `no_show_all` note in `new`.
    fn set_overflow_text(&self, left: &str, right: &str) {
        for (label, text) in [(&self.overflow_left, left), (&self.overflow_right, right)] {
            label.set_text(text);
            label.set_visible(!text.is_empty());
        }
    }

    /// Click-to-focus-this-workspace, for any part of the slot that isn't
    /// a tab: the collapsed marker, and the workspace number in either
    /// state. Takes the EventBox rather than looking one up by stack page
    /// name, since the number's box isn't in the stack at all.
    fn connect_focus_click(&self, events: &gtk::EventBox) {
        let state = self.state.clone();
        let workspace_id = self.workspace_id;

        events.connect_button_release_event(move |_, _| {
            if let Err(e) = state.niri().focus_workspace(workspace_id) {
                tracing::warn!(%e, id = workspace_id, "error switching workspace");
            }
            glib::Propagation::Stop
        });
    }

    /// Scroll anywhere on the strip -- BEHAVIOR.md: `focus-column-left` /
    /// `focus-column-right`. Purely a niri action, no local view-state to
    /// update: the next snapshot re-centers the visible slice on whatever
    /// column becomes current, the same way the Python daemon's on-scroll
    /// handler worked.
    fn connect_scroll(&self, events: &gtk::EventBox) {
        let state = self.state.clone();

        events.connect_scroll_event(move |_, event| {
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
    /// followed by however many tabs fit `Config::max_group_width_px()`
    /// at their real (unshrunk) width, anchored per `Self::anchor`'s doc
    /// comment.
    pub fn set_bloomed(&mut self, workspace: &WorkspaceInfo, windows: &[Window], output_width: f64) {
        self.number.set_text(&workspace_label(workspace));
        self.set_number_focused(true);

        let config = self.state.config();
        let max_group_width_px = config.max_group_width_px();
        let max_overflow_glyphs = config.max_overflow_glyphs();

        let columns = column::group(
            windows,
            output_width,
            config.tab_width_scale_px(),
            config.min_tab_width_px(),
            config.dynamic_tab_width(),
        );
        let total = columns.len();

        if total == 0 {
            self.set_overflow_text("", "");
            for (_, tab) in self.tabs.iter() {
                self.group.remove(tab.widget());
            }
            self.tabs.clear();
            self.anchor = None;
            self.group.show_all();
            self.stack.set_visible_child_name(GROUP);
            return;
        }

        let current_idx = columns
            .iter()
            .position(|c| c.window.is_focused || Some(c.window.id) == workspace.active_window_id)
            .unwrap_or(0);

        let mut start = self
            .anchor
            .and_then(|id| columns.iter().position(|c| c.window.id == id))
            .unwrap_or(current_idx);
        let mut end = expand_right(&columns, start, max_group_width_px);

        // Symmetric, zero-lookahead: shift exactly when focus reaches the
        // visible slice's own edge tab, in either direction -- not one
        // before it (too eager: with 5 visible tabs this was shifting at
        // the 4th, not the 5th) and not one past it (too late: the
        // previous left-edge behavior only reacted once focus had already
        // moved one step *beyond* the first visible tab, i.e. off the end
        // of the current slice, rather than reacting on the edge tab
        // itself the way the right side now does). Both loops handle a
        // single-column shift still leaving current at the edge (e.g. the
        // next column is unusually wide), and a current that jumped more
        // than one column away from the old anchor in a single update.
        while current_idx <= start && start > 0 {
            start -= 1;
            end = expand_right(&columns, start, max_group_width_px);
        }
        while current_idx + 1 >= end && end < total {
            start += 1;
            end = expand_right(&columns, start, max_group_width_px);
        }

        self.anchor = Some(columns[start].window.id);
        let visible = &columns[start..end];
        // Windows outside the visible slice render as the same glyph tick
        // marks collapsed workspaces use, not a "+N" numeral -- one visual
        // language for "more windows exist here" everywhere it appears,
        // rather than two different indicator styles. Capped the same way
        // marker_text is: a lot of overflow (the same 31-window workspace
        // that broke the old GtkScrolledWindow approach) would otherwise
        // just recreate that unbounded-width problem one level up.
        let left_text = glyph::capped(
            columns[..start].iter().map(|c| glyph::glyph_for(workspace, c.window)),
            max_overflow_glyphs,
        );
        let right_text = glyph::capped(
            columns[start + visible.len()..]
                .iter()
                .map(|c| glyph::glyph_for(workspace, c.window)),
            max_overflow_glyphs,
        );
        self.set_overflow_text(&left_text, &right_text);

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
        let glyphs = glyph::marker_text(workspace, windows, self.state.config().max_overflow_glyphs());
        // An empty, focused (bloom-eligible) workspace is handled by
        // set_bloomed's caller instead; a collapsed workspace with no
        // windows is never shown at all -- the caller doesn't create a
        // slot for it. Still guard here rather than show a blank marker if
        // that assumption is ever violated.
        let glyphs = if glyphs.is_empty() { "·" } else { &glyphs };
        // Glyphs only. The workspace number is `self.number`, a sibling
        // of the stack, in this state exactly as in the bloomed one --
        // see that field's own doc comment for why it isn't baked into
        // this string any more.
        self.number.set_text(&workspace_label(workspace));
        self.set_number_focused(false);
        self.marker.set_text(glyphs);
        self.stack.set_visible_child_name(MARKER);
    }
}
