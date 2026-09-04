# Behavior spec

Single source of truth for how Colonnade behaves, settled before writing the
widget code so we're not re-litigating interaction design mid-implementation.
Update this file first when a behavior changes; the code should follow it,
not the other way around.

## Layout: fused workspaces + tabs

One bar, not two modules. Per workspace, left to right by `idx`:

- **Focused workspace**: blooms into full tabs — one real GTK button per
  niri column, width proportional to `tile_size.0 / output_width` (a
  third-width column gets a third-width tab), grouped by
  `pos_in_scrolling_layout`. This is the only place clicking selects an
  individual window.
- **Every other workspace**: a single compact, non-interactive-per-window
  marker, reusing the glyph vocabulary from
  [niri-workspaces-rs](https://github.com/1jehuang/niri-workspaces-rs):
  `█` focused window, `▌` active-but-unfocused window, `|` other, `¦`
  background tmux. Dimmed relative to the focused workspace. An empty,
  unfocused workspace is hidden entirely; an empty, focused workspace shows
  a single dim `·`.
- Click anywhere on a collapsed workspace's marker → switch to that
  workspace as a whole (`niri msg action focus-workspace <id>`). No
  per-glyph click target there — only the focused workspace's tabs are
  individually clickable.

This absorbs niri-workspaces-rs's job into Colonnade rather than running it
as a second daemon alongside the tab strip — see README for the resource
argument (zero extra processes, both live in the same CFFI module already
inside waybar's GTK process).

## Toolkit: GTK3, not a choice

Colonnade is a Waybar CFFI module, which means it's handed a raw
`GtkWidget*` from Waybar's own already-running process
(`InitInfo::get_root_widget()`). Waybar 0.15.0 links `libgtk-3.so.0` and
`libgtk-layer-shell.so.0` (confirmed via `ldd`, and `waybar-cffi`'s own
docs: *"Waybar still uses Gtk 3 for its UI, so modules are required to also
use it"*) — not GTK4. A GTK4 widget can't be embedded in a GTK3 container;
they're separate object systems with separate rendering backends (GTK3:
direct cairo per-widget draw; GTK4: GSK scene-graph). This isn't a
preference, it's a consequence of living inside Waybar's process at all,
which is the entire premise of the zero-extra-process pitch.

The tradeoff this forces: GTK4 has `gtk_snapshot_push_mask()`, a real API
for exactly the label/edge-fade masking this project needs — GTK3 doesn't,
so Phase 3 does it manually via cairo `draw`-signal masking instead
(works, just not a one-liner). Considered and rejected: dropping the
Waybar-module architecture entirely to own a `wlr-layer-shell` surface
directly, which would make GTK4 available. Rejected because it trades
"zero extra RSS, shares Waybar's already-loaded GTK3" for "a second
GTK4+GSK+driver stack in a new process" — directly against the ~147 MB
baseline this project exists to beat. That tradeoff is worth taking later,
for Lumen, where owning the process is already the plan — not now, for a
module whose whole point is *not* owning a process.

## Hard invariant: one window per column

Colonnade's tab model — one tab per column, width proportional to that
column's screen fraction — only holds if a column always contains exactly
one window. If niri allows multiple windows to share a column, "the tab for
that column" has no well-defined meaning.

So this isn't a default to design around, it's an invariant to enforce at
the source. niri's config (`~/.config/niri/config.kdl`) has the mechanisms
that create multi-window columns unbound:

- `Mod+W` (`toggle-column-tabbed-display`) — niri's own native tab strip
  for stacked windows within a column. The same underlying feature this
  project is meant to replace, just scoped to one column instead of one
  workspace.
- `Mod+Comma` / `Mod+Period` (`consume-window-into-column` /
  `expel-window-from-column`)
- `Mod+BracketLeft` / `Mod+BracketRight`
  (`consume-or-expel-window-left/right`)

All commented out, not deleted, and niri's `tab-indicator` style block is
commented alongside them since it can now never draw. If this invariant is
ever relaxed, undo both edits together.

## Ordering: stable, never by recency

Tabs and workspace markers are ordered by niri's actual model (column
position within a workspace; `idx` across workspaces) — **never** reordered
by focus or recency. This mirrors a real macOS convention: menu bar extras
keep a stable left-to-right order that the OS doesn't rearrange on its own
(users may drag to reorder; apps don't reorder themselves). A tab strip
that jumps around when you switch windows is disorienting in exactly the
way this avoids.

## Click semantics (per tab, focused workspace only)

Carried forward from the current Python implementation, which already got
this right:

- Single click → focus window
- Double click → focus + maximize-to-edges
- Middle click → close window
- Right click → reserved, does nothing yet (future: context menu — rename,
  possibly others; see "Future ideas" below)
- Scroll (anywhere on the strip) → `focus-column-left` / `focus-column-right`

## Keyboard: niri owns the keybind, Colonnade only renders the hint

niri already runs a scrollable-tiling compositor with global keybinds, so
there's no reason for Colonnade to grab keyboard focus on a layer-shell
surface (unusual and complex for a status bar) — niri intercepts the key
event and acts, regardless of what has input focus.

- `Mod+1`..`Mod+9` — already bound (this user's config) to
  `focus-workspace <n>`.
- `Mod+Ctrl+1`..`Mod+Ctrl+9` — already bound to `move-column-to-workspace
  <n>`.
- `Mod+Shift+1`..`Mod+Shift+9` — **free**, to be bound to niri's native
  `focus-column <index>` action. Mirrors the existing pattern (plain
  number = jump workspace, Shift+number = jump within it) and needs no new
  code in Colonnade beyond rendering the index.
- This is a config change in `niri/config.kdl`, not a Colonnade feature —
  Colonnade doesn't implement the jump, only displays which number maps to
  which tab.
- Columns beyond index 9 have no bound shortcut (same ceiling browsers hit
  with `Cmd+1`-`Cmd+8`); no badge is shown for those.

### Badge display

Always visible, small numeral badge per tab showing its column index —
exposed as a config toggle (`show_index_badges`, default `true`) so it can
be turned off without losing the underlying behavior. This goes in the
same config file the existing tab daemon already reads
(`~/.config/niri-tabs/config.json` today, likely renamed alongside the
`libniri_taskbar.so` → `libcolonnade.so` rename).

## Overflow (tab strip only; collapsed workspace markers never overflow)

Two independent problems:

- **Per-tab label fade**: cairo mask on the label, applied only when the
  title actually overflows its allocated width — not a fixed-position
  fade regardless of content, which is what the Python version currently
  does. GTK3 has no CSS `mask-image` (see the GTK3-not-GTK4 note above),
  so this is drawn, not styled.
- **Strip-edge fade + scroll**: cairo-masked edge fade drawn only when the
  strip is actually overflowing (mirrors niri-workspaces-rs's existing
  "hide when not relevant" instinct). Scroll position itself is not
  special-cased — it's one more thing driven through the same shared
  animation primitive (`src/animate.rs`) that also handles column resize,
  workspace bloom/collapse, and tab insert/remove, built in Phase 1 rather
  than treated as a scroll-specific feature.

## Explicitly out of scope for now

- Hover-preview thumbnails (Windows-style Alt-Tab peek). Not pursued —
  adds a compositor-side screenshot/preview pipeline this project doesn't
  need yet.
- macOS-style "hover-to-switch between open menus while one is open."
  Doesn't map cleanly onto a tab strip, which has no modal open/closed
  menu state to switch between — tabs act immediately on click rather than
  opening something. Noted as considered, not applicable here.
- Drag-to-reorder tabs. Ordering is derived from niri's own column
  positions (see above); reordering tabs independent of that would fight
  the source of truth.

## Future ideas (not scheduled, not designed yet)

- **Right-click context menu**, with rename as its first real entry: a
  local title override Colonnade displays instead of the real window
  title (niri windows have no native "rename" concept — the title always
  comes from the app itself). Right-click is already reserved, doing
  nothing, specifically so this has somewhere to go.
- **Drag-to-reorder tabs** — raised again as a feature request, but it
  directly conflicts with two things already decided above: "Ordering:
  stable, never by recency" and this section's own existing "explicitly
  out of scope" entry for the same idea. Needs a real decision (does
  drag override niri's column order, just for display? does it try to
  reorder the columns in niri itself via IPC, if that's even possible?)
  before it goes anywhere near BEHAVIOR.md as settled — not adding it
  quietly alongside a contradicting rule.
