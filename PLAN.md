# Plan

`BEHAVIOR.md` is the spec (what it does). This is the build order (how we
get there, in what sequence, verified at each step). Update this file's
checkboxes as phases land; don't let it drift from reality.

**v0.1.0**: running as my actual daily-driver bar, single- and
multi-monitor, benchmarked at 73 MB combined RSS vs. the 147 MB baseline
(see `bench/README.md`). Everything below "Phase 1" that's marked `[x]`
has been through real usage, not just a smoke test -- several rounds of
"look at it live, find the actual bug, fix it, screenshot to confirm"
are folded into the phase notes below rather than a separate log.

## Where we are

**Done:**

- [x] Forked [LawnGnome/niri-taskbar](https://github.com/LawnGnome/niri-taskbar)
      → [`yesvus/colonnade`](https://github.com/yesvus/colonnade), cloned to
      `~/src/colonnade`, `upstream` remote kept for pulling fixes.
- [x] Crate renamed `niri-taskbar` → `colonnade` in `Cargo.toml`;
      dependency resolution confirmed clean (`Cargo.lock` updated).
- [x] `bench/` — reproducible RSS/CPU suite: `fixture.sh` (tagged foot
      windows, not ambient state), `churn.sh` (deterministic focus
      round-robin), `env.sh` (commit/versions/hardware manifest),
      `report.py` (combined + per-process summary). Self-exclusion bug
      (sampler matching its own argv) found and fixed while building it.
- [x] Baseline captured: current Python daemon + waybar combined ≈
      **147 MB RSS**, includes a real bug found along the way (two
      `niri-tab-daemon.py` instances running simultaneously, ~66 MB for
      what should be one process).
- [x] `BEHAVIOR.md` — fused workspace+tabs layout, click semantics,
      keyboard-jump architecture (niri owns the keybind, Colonnade only
      renders the badge), overflow/fade approach, one-window-per-column
      as a hard invariant.
- [x] Live niri config updated to enforce that invariant — `Mod+W`,
      `Mod+Comma`/`Period`, `Mod+BracketLeft`/`Right` unbound (commented),
      validated, hot-reloaded.

**Phase 1, first pass** (superseded in detail by several later rounds of
"look at it live on a real screenshot, find the actual bug, fix it" —
see the v0.1.0 note at the top; kept below as the historical record of
what shipped in this pass specifically):

- [x] `src/animate.rs` — tick-callback tweening primitive, `Rc<Cell<State>>`
      based (not a raw pointer — an earlier draft used one and would have
      been a real use-after-free risk if a widget were destroyed mid-
      animation; caught before it shipped). Retargets from the current
      interpolated value, doesn't spawn a second competing tick callback.
- [x] `src/workspace_slot.rs` (ended up replacing the planned
      `workspace_marker.rs` — bloom/collapse and the marker turned out to
      belong in one widget, not two) — collapsed glyph marker
      (`src/glyph.rs`, ported from niri-workspaces-rs) and bloomed tab
      group, swapped via `gtk::Stack`. **Deviation from the original plan:**
      bloom/collapse uses `Stack`'s own native crossfade+resize transition
      instead of a hand-rolled `animate.rs` case — GTK already solves
      "animate between two differently-sized children," so re-deriving it
      seemed like avoidable risk. `animate.rs` still drives each tab's
      *width* once bloomed.
- [x] `src/column.rs` — column grouping + width. Real bug caught while
      writing it: `width_fraction * output_width` is circular (equals
      `tile_size` again, i.e. a full-width column would request a
      literal 1920px tab). Fixed with a `REFERENCE_WIDTH_PX` constant
      tabs scale relative to each other against — correct *ratios*,
      tunable absolute size once it's visible and can be judged by eye.
- [x] `src/tab.rs` (replaces `button.rs`) — animated width, single/double/
      middle/right-click semantics per BEHAVIOR.md. **Scope cut:**
      upstream's real per-app icon loading (`icon.rs`'s async Pixbuf
      cache) isn't wired in yet; uses the same Nerd Font glyph table the
      Python daemon already used instead. `icon.rs` is kept, unused, for
      when that lands.
- [x] `niri-ipc` widened to `26.4.0` (matches this machine's niri 26.04);
      `Action::FocusWorkspace`/`FocusColumn`/`CloseWindow`/
      `MaximizeWindowToEdges` and `Workspace`'s `idx`/`is_active`/
      `active_window_id` fields confirmed present in that version before
      writing code against them.
- [x] `cargo build --release` succeeds; `target/release/libcolonnade.so`
      produced automatically from the crate name, links `libgtk-3.so.0`
      correctly.
- [x] Smoke-tested: a scratch Waybar instance (not the live config) loaded
      the built `.so` against this machine's real niri session for 8s —
      28 snapshot events processed, zero panics, zero GTK critical
      warnings, output correctly resolved to `HDMI-A-1`.

**Known gaps, not silently dropped:**
- Tab insert/remove doesn't animate from/to zero-width yet — a new column
  appears at full target width immediately, a removed one disappears
  immediately. The plan called for this; it didn't make it into this
  pass. Tracked here, not forgotten.
- **(Resolved since.)** Nothing had been visually verified as of this
  pass — since fixed: several rounds of actual screenshots (`grim`,
  including on the live production bar) turned up and fixed real bugs
  this smoke test couldn't have caught (stale workspace state, `GtkStack`
  homogeneous sizing, label size-negotiation dominating width requests,
  a missing scroll event mask, a missing scroll event *mask on top of
  that*, two glyph-truncation gaps, an asymmetric shift threshold). None
  of that would have surfaced from compiling and not crashing alone.
- Notifications (urgent-highlighting), real app icons, and app-based CSS
  matching are upstream features intentionally not wired into the new
  `Instance` yet (visible as `dead_code` warnings on `cargo build`) —
  none of this is in `BEHAVIOR.md`'s spec, so cutting it was a scope
  decision, not an oversight, but it means the current build is visually
  and functionally sparser than upstream's `niri-taskbar` in those two
  specific ways.
- Multi-monitor lid-closed (`eDP-1` configured but disconnected) hasn't
  been exercised.

**Not started:** Phase 2 onward.

## Phase 1 — Skeleton + the animation primitive

Goal: the bar renders the right *structure* — collapsed workspace markers
either side of one expanded, column-grouped, proportionally-sized tab
group for the focused workspace — with correct click behavior, **and every
layout change (column resize, workspace bloom/collapse, tab insert/remove)
is already animated**, not snapping. This is a deliberate change from the
original sequencing: GTK3 doesn't animate layout (width/height/position)
for free — only paint properties (color, opacity) get that via CSS
transitions — so every one of those four cases is an instant jump unless
something manually drives it frame-by-frame. Originally that was deferred
to Phase 3 alongside fades, which would have meant judging the whole
concept through a jumpy, half-finished lens for two phases. Building the
one shared primitive now, and using it everywhere from the start, avoids
that.

Reuse from upstream as-is:
- `src/niri.rs`, `src/niri/*` — event stream, state snapshot. No changes
  expected; niri's `Window`/`Workspace` IPC types already carry
  `pos_in_scrolling_layout`, `tile_size`, `workspace_id`.
- `src/lib.rs`'s `Instance` — the snapshot→widget-diff loop
  (`process_window_snapshot`) is the right shape; it currently builds a
  flat `BTreeMap<u64, Button>` keyed by window ID. Needs restructuring to
  a two-level model (workspace → either a marker or a column group).
- `src/icon.rs`, `src/output.rs` — untouched.

New:
- `src/animate.rs` — the shared primitive. One function/struct that takes
  a widget, a current size/position, and a target, and drives it there
  over a fixed duration via `gtk::Widget::add_tick_callback`, easing with
  the same curve already used in the existing waybar CSS
  (`cubic-bezier(0.215, 0.61, 0.355, 1)`, ~150ms) so the animated version
  feels continuous with what's already shipped, not a different motion
  language bolted on. Must handle re-targeting cleanly: if a new target
  arrives mid-animation (rapid workspace switching, e.g. spamming
  `Mod+1`/`Mod+2`), the animation retargets from its *current* interpolated
  position, not the original start — it does not queue, restart from zero,
  or let two animations fight over the same widget.
- `src/workspace_marker.rs` — the collapsed-workspace widget: a
  `gtk::EventBox` wrapping a `gtk::Label` with the `█`/`▌`/`|`/`¦` Pango
  markup ported from `niri-workspaces-rs/src/main.rs`
  (`output_status`/`get_color`/`dim_color`/`window_sort_key` — port the
  glyph-selection logic, not the whole binary; skip its per-app color
  table per the existing monochrome-by-request state in that file).
  Click handler → `focus-workspace <id>`. Its width transition (marker ↔
  full tab group) goes through `animate.rs`.
- `src/column.rs` — groups windows by `pos_in_scrolling_layout.0` within
  the focused workspace, computes each tab's width from
  `tile_size.0 / output_width` (output width from the existing
  `output::Filter`/output lookup already in `lib.rs`). Width changes go
  through `animate.rs`.
- Rework `src/button.rs`'s `Button` (or add `src/tab.rs` alongside it) to
  take a target width and render inside a column group rather than a flat
  `gtk::Box`. Tab insert (new column) animates in from zero-width, not a
  pop-in; tab remove animates to zero-width before the widget is actually
  destroyed, not an instant disappearance.
- `Cargo.toml`: widen `niri-ipc = ">=25.11.0, <25.12.0"` to cover this
  machine's niri 26.04 — check upstream's changelog/tags for whether a
  newer `niri-taskbar` release already did this before hand-rolling it.

Verification:
- `cargo build --release` succeeds, produces
  `target/release/libcolonnade.so` (rename the lib output to match the
  new package name — check `[lib] name = ...` in `Cargo.toml`).
- Point a scratch Waybar config (`waybar -c /tmp/colonnade-test.jsonc`, not
  the live config) at the built `.so`, confirm against `BEHAVIOR.md`:
  correct grouping, correct proportional widths (compare visually against
  `niri msg -j windows` tile sizes), single vs double vs middle vs
  right-click all do the right thing, collapsed workspaces switch on
  click.
- Animation-specific: switch workspace focus back and forth slowly (watch
  it animate cleanly) and rapidly (`Mod+1`,`Mod+2`,`Mod+1`... as fast as
  possible — confirm it retargets smoothly instead of stuttering or
  snapping to a stale target); open/close a window in the focused
  workspace mid-animation of something else; resize a column
  (`Mod+R`) while its tab is on-screen.
- Multi-monitor-specific (see `BEHAVIOR.md`): with both `HDMI-A-1` and
  `eDP-1` active, confirm each instance only ever shows its own output's
  workspaces, and each blooms its own `is_active` workspace independent of
  which output currently has keyboard focus — put focus on HDMI-A-1 and
  confirm eDP-1's bar still has something bloomed, not everything
  collapsed. Also test with `eDP-1` configured but disconnected (lid
  closed) — confirm no crash and no cross-output leakage.

## Phase 2 — Keyboard jump + index badges

- Add `Mod+Shift+1`..`Mod+Shift+9` → `focus-column <index>` to
  `~/.config/niri/config.kdl` (this is the one piece of Phase 2 that lives
  outside the repo).
- Render the index badge on each tab (small numeral, matches
  `focus-column`'s 1-based indexing, no badge past index 9).
- Add `show_index_badges` (default `true`) to the CFFI module's config
  struct (`src/config.rs` already has the pattern for typed, optional
  Waybar-JSON config fields — follow it).

Verification: press `Mod+Shift+3` with 4+ columns open, confirm focus
lands on the labeled tab; toggle `show_index_badges` off in Waybar config,
confirm badges disappear without affecting click behavior.

## Phase 3 — Fades

Purely cosmetic and additive at this point — the animation work already
landed in Phase 1, so this phase can't destabilize interaction feel, only
add drawn detail on top of it.

- Per-tab label fade: cairo mask on the label's `draw` signal, gated on
  actual overflow (compare requested vs allocated width), not fixed
  truncation.
- Strip-edge fade: same cairo-mask approach, drawn only while the tab
  group is actually overflowing its allocation.
- Scroll position itself already animates via `src/animate.rs` (Phase 1);
  this phase just wraps the tab group in a `GtkScrolledWindow` and feeds
  `focus-column-left/right` events into that existing primitive rather
  than an instant `GtkAdjustment::set_value`.

Verification: visual — label fade only appears on titles that actually
overflow, edge fade only appears when there's something to scroll to
(mirrors the Firefox `scrolledtostart`/`scrolledtoend` behavior researched
earlier).

## Phase 4 — Benchmark against the baseline

- Add `bench/colonnade.sh`, mirroring `bench/baseline.sh`'s structure
  (same `fixture.sh`, same `churn.sh`, same `env.sh`, same
  `report.py`) but matching the Waybar process running with
  `libcolonnade.so` loaded instead of the Python daemon's process set.
- Run it, commit the result under `bench/results/`, and add a short
  comparison note (idle/churn combined RSS, delta vs the committed
  baseline) to `bench/README.md`.
- If regressions show up (GTK widget count, cairo redraw frequency),
  profile with `perf` (confirmed installed) before assuming a specific
  cause.

## Phase 5 — Cutover + packaging

- Update the live Waybar config: replace the `custom/tab-N` /
  `custom/fade-*` / `custom/page-*` modules and the
  `niri-tab-daemon.py` exec with a single `cffi/colonnade` module.
- Decommission `niri-tab-daemon.py` and `niri-taskbar.py`
  (`~/.config/waybar/`) once the CFFI module is confirmed stable for a
  few days of real use — don't delete immediately, disable first.
- `README.md`: flip the `Status` checklist items to done, update
  install/config sections to reference `colonnade`/`libcolonnade.so`
  instead of the inherited upstream naming.
- Package for AUR (`PKGBUILD`) once the above is stable — not before.

## Later, separate effort: Lumen

Not scoped in detail yet. Control panel + settings app, full Waybar
alternative, built once Colonnade itself is proven. Revisit after Phase 5.
