//! Groups the bloomed workspace's windows by niri column and computes each
//! column's tab width as a fraction of the output. Pure data transform, no
//! GTK -- the widget layer (`tab.rs`) just reads `Column` values.

use crate::niri::Window;

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
    /// This column's tab width in pixels -- `width_fraction *
    /// width_scale_px` (a `Config::tab_width_scale_px()` value, see
    /// `group`'s doc comment), independent of any other column.
    pub target_width_px: i32,
}

/// Groups `windows` (expected to already be scoped to a single, bloomed
/// workspace) by column, sorted left to right, using `output_width` to
/// compute each column's own width -- independently of the others.
///
/// Each column's width is `width_fraction * width_scale_px`, computed
/// independently of every other column -- **not** normalized against
/// sibling count. Niri's own model is explicit about this: "opening a new
/// window never causes existing windows to resize." An earlier version of
/// this normalized against the group's total, which shrank every existing
/// tab whenever a new one opened -- exactly the behavior niri itself
/// refuses to do. `width_scale_px` is what "proportional" is measured
/// against instead of the literal output width (which would request a
/// full-width column's tab at a literal 1920px). The group's total width
/// is allowed to grow unboundedly as more columns open, same as niri's
/// own strip; the caller (`workspace_slot.rs`) is what bounds how many of
/// them are actually shown.
///
/// `dynamic` selects between this proportional sizing (`true`, the
/// default -- `Config::dynamic_tab_width()`) and a uniform width for
/// every tab (`false`: every tab gets exactly `width_scale_px`,
/// regardless of its real column's proportion) for anyone who'd rather
/// have a traditional flat taskbar look than width that varies with
/// window size.
pub fn group<'a>(
    windows: &'a [Window],
    output_width: f64,
    width_scale_px: f64,
    min_tab_width_px: i32,
    dynamic: bool,
) -> Vec<Column<'a>> {
    let mut columns: Vec<Column<'a>> = windows
        .iter()
        .filter_map(|window| {
            let (index, _tile_index) = window.layout.pos_in_scrolling_layout?;
            let width_fraction = if output_width > 0.0 {
                window.layout.tile_size.0 / output_width
            } else {
                0.0
            };
            let target_width_px = if dynamic {
                ((width_fraction * width_scale_px).round() as i32).max(min_tab_width_px)
            } else {
                (width_scale_px.round() as i32).max(min_tab_width_px)
            };
            Some(Column {
                index,
                window,
                width_fraction,
                target_width_px,
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
