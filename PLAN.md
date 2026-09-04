# Plan

`BEHAVIOR.md` is the spec (what it does). This is the build order (how we
get there, in what sequence, verified at each step). Update this file's
checkboxes as phases land; don't let it drift from reality.

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

**Not started:** anything in `src/`. Upstream's code is still what's there.

## Phase 1 — Skeleton: fused layout, no polish

Goal: the bar renders the right *structure* — collapsed workspace markers
either side of one expanded, column-grouped, proportionally-sized tab
group for the focused workspace — with correct click behavior. No fades,
no animation, no index badges yet. This is the phase that proves the
architecture before investing in polish.

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
- `src/workspace_marker.rs` — the collapsed-workspace widget: a
  `gtk::EventBox` wrapping a `gtk::Label` with the `█`/`▌`/`|`/`¦` Pango
  markup ported from `niri-workspaces-rs/src/main.rs`
  (`output_status`/`get_color`/`dim_color`/`window_sort_key` — port the
  glyph-selection logic, not the whole binary; skip its per-app color
  table per the existing monochrome-by-request state in that file).
  Click handler → `focus-workspace <id>`.
- `src/column.rs` — groups windows by `pos_in_scrolling_layout.0` within
  the focused workspace, computes each tab's width from
  `tile_size.0 / output_width` (output width from the existing
  `output::Filter`/output lookup already in `lib.rs`).
- Rework `src/button.rs`'s `Button` (or add `src/tab.rs` alongside it) to
  take a target width and render inside a column group rather than a flat
  `gtk::Box`.
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

## Phase 3 — Fades and animation

- Per-tab label fade: cairo mask on the label's `draw` signal, gated on
  actual overflow (compare requested vs allocated width), not fixed
  truncation.
- Strip-edge fade: same cairo-mask approach, drawn only while the tab
  group is actually overflowing its allocation.
- Smooth scroll: wrap the tab group in a `GtkScrolledWindow`, drive its
  `GtkAdjustment` from a `gtk::Widget::add_tick_callback` frame-clock
  callback on focus-column-left/right, instead of an instant `set_value`.

Verification: visual — no more instant jumps on focus change, label fade
only appears on titles that actually overflow, edge fade only appears
when there's something to scroll to (mirrors the Firefox
`scrolledtostart`/`scrolledtoend` behavior researched earlier).

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
