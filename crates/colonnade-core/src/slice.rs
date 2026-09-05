//! Which columns of a bloomed workspace are visible, and where the
//! visible slice's left edge is anchored. Pulled out of the GTK widget
//! code (`workspace_slot.rs` in the `colonnade` crate) so both that
//! renderer and Lumen's iced one compute the same slice the same way --
//! see BEHAVIOR.md's "Layout" section for the interaction this
//! implements.

use crate::column::Column;

/// The result of slicing a bloomed workspace's columns down to what fits
/// `budget_px`: which columns are visible, and the window id the visible
/// slice is now anchored on (feed this back in as `anchor` on the next
/// call).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BloomedSlice {
    /// Start index into the `columns` slice passed to `compute` (inclusive).
    pub start: usize,
    /// End index into the `columns` slice passed to `compute` (exclusive).
    pub end: usize,
    /// The window id anchoring the left edge of the visible slice --
    /// persist this and pass it back in as `anchor` next time, rather
    /// than recomputing from scratch, so the slice only moves once focus
    /// actually reaches its trailing edge instead of recentring on every
    /// focus change.
    pub anchor: u64,
}

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

/// Computes the visible slice of `columns` (non-empty) that fits within
/// `budget_px`, anchored per `anchor`'s doc comment on
/// [`BloomedSlice::anchor`]: reuses the previous anchor if its window is
/// still present, falling back to `current_idx` (the focused/active
/// column) otherwise, then shifts the slice by exactly one column at a
/// time -- symmetric, zero-lookahead -- whenever `current_idx` reaches the
/// slice's own edge in either direction.
///
/// Panics if `columns` is empty; callers should check for that case
/// themselves (an empty bloomed workspace has nothing to slice).
pub fn compute(columns: &[Column<'_>], current_idx: usize, anchor: Option<u64>, budget_px: i32) -> BloomedSlice {
    let total = columns.len();
    assert!(total > 0, "compute called with no columns");

    let mut start = anchor
        .and_then(|id| columns.iter().position(|c| c.window.id == id))
        .unwrap_or(current_idx);
    let mut end = expand_right(columns, start, budget_px);

    while current_idx <= start && start > 0 {
        start -= 1;
        end = expand_right(columns, start, budget_px);
    }
    while current_idx + 1 >= end && end < total {
        start += 1;
        end = expand_right(columns, start, budget_px);
    }

    BloomedSlice {
        start,
        end,
        anchor: columns[start].window.id,
    }
}
