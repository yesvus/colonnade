//! Glyph vocabulary for collapsed workspace markers, ported from
//! [niri-workspaces-rs](https://github.com/1jehuang/niri-workspaces-rs):
//! `█` the globally-focused window, `▌` this workspace's own "active"
//! window, `|` any other window, `¦` a background tmux pane. No color
//! coding here -- CSS handles dimming via a class on the whole marker,
//! since the glyph shapes already carry the meaning on their own.
//!
//! In practice `█` can only appear on the bloomed workspace (see
//! BEHAVIOR.md's "Multi-monitor" section: a collapsed workspace can never
//! contain the single globally-focused window), but the check is kept for
//! completeness rather than assumed away.

use crate::niri::{Window, WorkspaceInfo};

/// The marker text for one collapsed workspace: one glyph per window,
/// left to right in column order.
pub fn marker_text(workspace: &WorkspaceInfo, windows: &[Window]) -> String {
    let mut in_workspace: Vec<&Window> = windows
        .iter()
        .filter(|w| w.workspace_id == Some(workspace.id))
        .collect();
    in_workspace.sort_by_key(|w| w.layout.pos_in_scrolling_layout.unwrap_or_default());

    in_workspace
        .into_iter()
        .map(|w| glyph_for(workspace, w))
        .collect()
}

/// Public so overflow ticks (the bloomed workspace's own windows that fall
/// outside the visible tab slice) can reuse the exact same glyph
/// vocabulary as collapsed-workspace markers, instead of a numeral -- one
/// consistent visual language rather than two different indicator styles.
pub fn glyph_for(workspace: &WorkspaceInfo, window: &Window) -> char {
    if window.is_focused {
        '█'
    } else if workspace.active_window_id == Some(window.id) {
        '▌'
    } else if is_tmux_title(window.title.as_deref().unwrap_or_default()) {
        '¦'
    } else {
        '|'
    }
}

fn is_tmux_title(title: &str) -> bool {
    title.to_lowercase().contains("tmux")
}
