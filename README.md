# Colonnade

**The taskbar Windows XP got right, rebuilt for niri's columns.**

[niri](https://github.com/YaLTeR/niri) arranges windows "in columns on an
infinite strip going to the right" — its own words. Opening a window never
resizes the ones you already have open. It's a genuinely better model than a
single-app-at-a-time desktop, but it leaves you without the one thing that
made task-switching effortless on a desktop from twenty years ago: a row of
labeled buttons you can see and click, one per open window, always in the
same place.

Colonnade is that row, built to match niri's own model rather than bolting a
generic taskbar on top of it. A *colonnade* is a row of columns holding up a
roof — which is exactly what this renders: niri's columns, as a row of tabs.
Each tab is sized to the fraction of the screen its column actually occupies
(a third-width column gets a third-width tab), so the bar is a small, honest
map of your workspace, not just a list of names.

![Full bar, in context](images/full-bar.png)

A tab strip alone loses "which workspace am I even in," so Colonnade fuses
two things that are normally two separate modules (and two separate
processes) into one bar: every workspace gets a compact, non-interactive
marker — click it to switch — except the *focused* one, which is the only
one that blooms into full clickable, column-grouped tabs. Collapsed
workspaces reuse the glyph vocabulary already proven in
[niri-workspaces-rs](https://github.com/1jehuang/niri-workspaces-rs) (`█`
focused window, `▌` active-but-unfocused, `|` other, `¦` background tmux),
so Colonnade effectively absorbs that project's job rather than running
alongside it as a second daemon. Because both live in the same CFFI module
inside waybar's own already-running GTK process, this costs **zero extra
processes** — not "a lighter second process," none at all.

![Bloomed workspace with real apps](images/tabs-close-up.png)

## Status

Fork of [LawnGnome/niri-taskbar][upstream], now well past the fork point.
The tab strip, workspace fusion, and click/scroll/keyboard navigation are
built, running as my actual daily-driver bar (both single- and
multi-monitor), and benchmarked against the Python daemon setup it
replaces. Some polish is still ahead; see the checklist.

- [x] Tabs grouped by niri column (`pos_in_scrolling_layout`), width
      proportional to column width, independent per column — no
      shrink-on-open, matching niri's own "opening a window never resizes
      the others" rule
- [x] Fused workspace markers, absorbing niri-workspaces-rs's job
- [x] Click (single/double/middle), scroll, and workspace-marker-click
      navigation, all wired to niri's own actions
- [x] Overflow handling: a real pixel-width budget (not a fixed tab count,
      not a `GtkScrolledWindow` — see `BEHAVIOR.md`'s "Overflow" section
      for why that was tried and abandoned), anchored and shifted
      symmetrically with zero lookahead
- [x] Layout is config-tunable (tab width, group width budget, overflow
      glyph cap) — see [Configuration](#configuration) — not hardcoded
      constants requiring a rebuild to adjust
- [x] Benchmarked: see [`bench/`](bench/) — **73 MB combined vs. the
      147 MB Python-daemon baseline**, roughly half
- [ ] Keyboard index badges + `Mod+Shift+1-9` direct-jump (niri already
      supports the underlying action; Colonnade doesn't render the badge
      yet)
- [ ] Cairo-masked label fade for overflowing titles, and a real edge fade
      on an overflowing tab strip (GTK3 has no CSS `mask-image`, so this
      has to be drawn, not styled)
- [ ] Animated slice transitions (tabs currently pop in/out instantly when
      the visible window shifts, rather than animating)
- [ ] Real per-app icons (currently a Nerd Font glyph table, same as the
      Python daemon it replaces, not upstream's async Pixbuf icon cache)

Full behavior spec: [`BEHAVIOR.md`](BEHAVIOR.md). Build order and what's
landed so far, phase by phase: [`PLAN.md`](PLAN.md).

## Roadmap: Lumen

Colonnade is meant to also work as a **standalone drop-in module** for
anyone already running Waybar — you should never be forced into more than
you asked for. But it's also the first piece of a larger idea: **Lumen**, a
complete, polished, low-resource menubar suite for niri (bar + control panel
+ settings app), for people who want a finished shell rather than assembling
one module at a time. Colonnade ships first, on its own, either way.

## Installation

### Requirements

- Rust 1.87.0 or later
- Niri 26.04 or later (`niri-ipc` is pinned `>=25.11.0, <27`)
- Gtk+ 3 (including the development package on distros that separate those
  out)
- Waybar 0.12.0 or later — specifically a build still linked against GTK3
  (confirmed via `ldd $(which waybar) | grep gtk`); this is a hard
  requirement, not a version floor, see `BEHAVIOR.md`'s "Toolkit" section

### Building

```bash
cargo build --release
```

This produces `target/release/libcolonnade.so`, derived automatically from
the crate name.

## Configuration

Standard [CFFI Waybar module][cffi] configuration:

```jsonc
{
  "modules-left": ["cffi/colonnade"],
  // ...
  "cffi/colonnade": {
    "module_path": "/your/path/to/libcolonnade.so",

    // Optional -- every one has a sane default; only override what you
    // need. See BEHAVIOR.md's "Layout is config-tunable" section.
    "layout": {
      "tab_width_scale_px": 260,   // width unit a full-width column's tab
                                    // scales against (not output_width --
                                    // see column.rs's doc comment)
      "min_tab_width_px": 40,      // floor so a tiny column never
                                    // produces a barely-clickable tab
      "max_group_width_px": 620,   // real pixel budget for the visible
                                    // tab group. Colonnade only knows
                                    // about its own space on modules-left
                                    // -- it has no idea what else is on
                                    // the bar. If you have anything in
                                    // modules-center, or modules-right is
                                    // wide, LOWER this so the tab group
                                    // can't grow into them. Raise it if
                                    // you have plenty of empty bar space.
      "max_overflow_glyphs": 10,   // caps collapsed-marker and overflow-
                                    // tick glyph strings ("…" when
                                    // truncated)
      "font_size_pt": 9.0,         // every piece of text Colonnade draws.
                                    // Text size only -- it is not the
                                    // lever on tab height, see below
      "tab_height_px": 22,         // a tab pill's drawn height, border
                                    // included. Tabs sit centred in the
                                    // bar, so this is exact rather than
                                    // "text plus whatever padding". Even
                                    // values centre evenly in an even bar
                                    // height. Floor is text height plus
                                    // border, ~17px at 9pt
      "dynamic_tab_width": true    // false gives every tab the same width
                                    // (a flat-taskbar look) instead of
                                    // tracking each column's proportion
    }
  }
}
```

See [upstream's README][upstream] for application highlighting, multi-output
support, and notification integration — all inherited as-is for now
(notification "urgent" highlighting isn't wired into the fused layout yet,
though the underlying D-Bus listener still runs; see `PLAN.md`).

## Credit

Forked from [LawnGnome/niri-taskbar][upstream] (MIT), which did the actual
hard work: the niri IPC event stream, the CFFI/GTK plumbing, icon lookup,
and notification matching. Colonnade's job is the tab strip and fused
workspace layout on top of that foundation.

Collapsed-workspace markers port the glyph vocabulary (`█`/`▌`/`|`/`¦`) and
focus-semantics directly from
[1jehuang/niri-workspaces-rs](https://github.com/1jehuang/niri-workspaces-rs)
(`src/glyph.rs`) — Colonnade's fused layout absorbs that project's job
rather than running it as a second process alongside the tab strip, but
the glyph logic itself is theirs.

[cffi]: https://github.com/Alexays/Waybar/wiki/Module:-CFFI
[upstream]: https://github.com/LawnGnome/niri-taskbar
