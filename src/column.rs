//! Groups the bloomed workspace's windows by niri column and computes each
//! column's tab width as a fraction of the output. Pure data transform, no
//! GTK -- the widget layer (`tab.rs`) just reads `Column` values.

use crate::niri::Window;

/// The tab group's total width budget, in pixels -- each column's
/// `width_fraction` is normalized *against the other columns currently in
/// this workspace*, then distributed across this budget, rather than each
/// tab claiming a flat width independent of how many others exist. That
/// earlier flat-per-tab approach grew the group unboundedly as more
/// windows opened, which is wrong for a bar (found by actually looking at
/// it: tabs were too wide). Normalizing keeps the group's total size
/// roughly consistent regardless of window count -- tabs shrink as more
/// columns exist, like flex distribution. Tunable by eye; Phase 3's
/// scrollable strip is what handles the case where even MIN_TAB_WIDTH_PX
/// floors push the total past the bar's actual available space.
const TOTAL_GROUP_BUDGET_PX: f64 = 420.0;

/// Floor so a tiny or momentarily-zero `width_fraction` (e.g. mid-resize)
/// never produces a degenerate, barely-clickable tab.
const MIN_TAB_WIDTH_PX: i32 = 40;

/// One niri column within the bloomed workspace. Exactly one window per
/// column is a hard invariant (see BEHAVIOR.md) -- niri's own config has
/// the mechanisms that would violate it (tabbed columns, consume-into-
/// column) unbound, so this doesn't defend against it beyond picking the
/// first window if it's ever violated anyway, rather than panicking.
pub struct Column<'a> {
    /// 1-based column index, matching niri's own `focus-column <index>`
    /// and `pos_in_scrolling_layout` indexing.
    pub index: usize,
    pub window: &'a Window,
    /// This column's tile width as a fraction of the output's logical
    /// width (e.g. a third-width column is ~0.333). Kept for reference/
    /// debugging; `target_width_px` (normalized against the other columns
    /// present) is what actually drives the tab's size.
    pub width_fraction: f64,
    /// This column's tab width in pixels, already normalized against its
    /// siblings and distributed across `TOTAL_GROUP_BUDGET_PX`.
    pub target_width_px: i32,
}

/// Groups `windows` (expected to already be scoped to a single, bloomed
/// workspace) by column, sorted left to right, using `output_width` to
/// compute each column's proportional width relative to the others.
pub fn group<'a>(windows: &'a [Window], output_width: f64) -> Vec<Column<'a>> {
    struct Raw<'a> {
        index: usize,
        window: &'a Window,
        fraction: f64,
    }

    let mut raw: Vec<Raw<'a>> = windows
        .iter()
        .filter_map(|window| {
            let (index, _tile_index) = window.layout.pos_in_scrolling_layout?;
            let fraction = if output_width > 0.0 {
                window.layout.tile_size.0 / output_width
            } else {
                0.0
            };
            Some(Raw {
                index,
                window,
                fraction,
            })
        })
        .collect();

    raw.sort_by_key(|c| c.index);
    // Defensive only (see doc comment above): if the one-window-per-column
    // invariant is ever violated, keep the first window in each column
    // rather than rendering two tabs claiming the same index.
    raw.dedup_by_key(|c| c.index);

    let total_fraction: f64 = raw.iter().map(|c| c.fraction).sum();
    let count = raw.len().max(1) as f64;

    raw.into_iter()
        .map(|c| {
            // Split evenly if fractions are all zero (output width not yet
            // known) rather than collapsing every tab to MIN_TAB_WIDTH_PX.
            let normalized = if total_fraction > 0.0 {
                c.fraction / total_fraction
            } else {
                1.0 / count
            };
            let target_width_px =
                ((normalized * TOTAL_GROUP_BUDGET_PX).round() as i32).max(MIN_TAB_WIDTH_PX);
            Column {
                index: c.index,
                window: c.window,
                width_fraction: c.fraction,
                target_width_px,
            }
        })
        .collect()
}
