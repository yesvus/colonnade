//! Groups the bloomed workspace's windows by niri column and computes each
//! column's tab width as a fraction of the output. Pure data transform, no
//! GTK -- the widget layer (`tab.rs`) just reads `Column` values.

use crate::niri::Window;

/// A tab's width is `width_fraction * REFERENCE_WIDTH_PX`, not
/// `width_fraction * output_width_px` -- the latter would literally
/// request a tab as wide as the real window (e.g. 1920px for a
/// full-width column), since `width_fraction` is already `tile_size /
/// output_width`. "Proportional" means proportional *among tabs in the
/// bar*, not a 1:1 pixel copy of the real layout. This constant stands in
/// for "how much width the tab-group would have if given generous room" --
/// tabs scale relative to each other correctly regardless of its exact
/// value; Phase 3's scrollable strip is what handles the case where the
/// sum exceeds the bar's actual available space. Treat as tunable once
/// this is visible and can be judged by eye, not as a precise measurement.
const REFERENCE_WIDTH_PX: f64 = 600.0;

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
    /// width (e.g. a third-width column is ~0.333). `0.0` if the output
    /// width isn't known yet.
    pub width_fraction: f64,
}

impl Column<'_> {
    /// This column's tab width in pixels -- see `REFERENCE_WIDTH_PX` for
    /// why this isn't just `width_fraction * output_width`.
    pub fn target_width_px(&self) -> i32 {
        ((self.width_fraction * REFERENCE_WIDTH_PX).round() as i32).max(MIN_TAB_WIDTH_PX)
    }
}

/// Groups `windows` (expected to already be scoped to a single, bloomed
/// workspace) by column, sorted left to right, using `output_width` to
/// compute each column's proportional width.
pub fn group<'a>(windows: &'a [Window], output_width: f64) -> Vec<Column<'a>> {
    let mut columns: Vec<Column<'a>> = windows
        .iter()
        .filter_map(|window| {
            let (index, _tile_index) = window.layout.pos_in_scrolling_layout?;
            let width_fraction = if output_width > 0.0 {
                window.layout.tile_size.0 / output_width
            } else {
                0.0
            };
            Some(Column {
                index,
                window,
                width_fraction,
            })
        })
        .collect();

    columns.sort_by_key(|c| c.index);
    // Defensive only (see doc comment above): if the one-window-per-column
    // invariant is ever violated, keep the first window in each column
    // rather than rendering two tabs claiming the same index.
    columns.dedup_by_key(|c| c.index);

    columns
}
