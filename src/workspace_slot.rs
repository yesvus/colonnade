//! One slot in the bar per visible workspace (see BEHAVIOR.md's "Layout"
//! section) -- either a collapsed glyph marker or, for the one bloomed
//! workspace on this output, a full group of tabs. Uses `gtk::Stack`
//! rather than `animate.rs`'s hand-rolled tick-callback primitive for the
//! bloom/collapse transition itself: `Stack` already animates both the
//! crossfade *and* the resize between differently-sized children natively
//! -- a well-tested GTK mechanism, not something worth re-deriving.
//! `animate.rs` is still what drives each individual tab's width once
//! bloomed (see `tab.rs`).

use std::{collections::BTreeMap, fmt::Debug};

use waybar_cffi::gtk::{
    self,
    glib::Cast,
    prelude::{BoxExt, ContainerExt, LabelExt, StackExt, StyleContextExt, WidgetExt},
};

use crate::{column, glyph, niri::WorkspaceInfo, state::State, tab::Tab};

const MARKER: &str = "marker";
const GROUP: &str = "group";

pub struct WorkspaceSlot {
    stack: gtk::Stack,
    marker: gtk::Label,
    group: gtk::Box,
    /// Workspace idx (or name), shown before the first tab -- the bloomed
    /// view otherwise has no cue at all for which workspace you're
    /// looking at. Matches niri-workspaces-rs's original convention of
    /// prefixing the focused workspace's own glyph row the same way.
    number: gtk::Label,
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

        let slot = Self {
            stack,
            marker,
            group,
            number,
            tabs: BTreeMap::new(),
            state: state.clone(),
            workspace_id,
        };

        slot.connect_marker_click();
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
                waybar_cffi::gtk::glib::Propagation::Stop
            });
        }
    }

    /// Renders this workspace as bloomed: a workspace-number label
    /// followed by a full, column-grouped, animated tab strip.
    pub fn set_bloomed(
        &mut self,
        workspace: &WorkspaceInfo,
        windows: &[crate::niri::Window],
        output_width: f64,
    ) {
        let label = workspace
            .name
            .as_deref()
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| workspace.idx.to_string());
        self.number.set_text(&label);
        self.group.reorder_child(&self.number, 0);

        let columns = column::group(windows, output_width);
        let mut seen = std::collections::BTreeSet::new();

        // Tabs start at position 1 -- position 0 is the workspace number.
        for (i, col) in columns.iter().enumerate() {
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
                self.group.reorder_child(tab.widget(), (i + 1) as i32);
            }
        }

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
    pub fn set_collapsed(&mut self, workspace: &WorkspaceInfo, windows: &[crate::niri::Window]) {
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
