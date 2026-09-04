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

## Status

Early-stage fork of [LawnGnome/niri-taskbar][upstream], in active
development. Upstream gives us the hard parts already solved — a real
[Waybar CFFI module][cffi] with an in-process niri IPC event stream, so there
are no polling forks and no shelling out per window-focus-change. Everything
below is what Colonnade adds on top:

- [ ] Tabs grouped by niri column (`pos_in_scrolling_layout`), not a flat list
- [ ] Tab width proportional to column width (`tile_size` vs output width)
- [ ] Smooth scroll animation on focus change, driven by GTK's frame clock —
      not an instant relayout
- [ ] Cairo-masked label fade for overflowing titles, and a real edge fade on
      an overflowing tab strip (GTK3 has no CSS `mask-image`, so this has to
      be drawn, not styled)
- [ ] Rename the build artifact from `libniri_taskbar.so` to
      `libcolonnade.so` once the above lands

Until the checklist above is done, build/install instructions are inherited
from upstream and describe `niri-taskbar`, not `colonnade` — see below.

A performance budget is part of the spec, not an afterthought: see
[`bench/`](bench/) for the trusted, checked-in benchmark suite and the
current Python-daemon baseline it has to beat.

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
- Niri (with a version corresponding to the version in the `niri-taskbar`
  crate version; eg `0.4.0+niri-25.11` is specifically for Niri 25.11 — this
  constraint is being widened as part of the Colonnade work, see
  [Status](#status))
- Gtk+ 3 (including the development package on distros that separate those
  out)
- Waybar 0.12.0 (or any version that's API compatible with 0.12)

### Building

```bash
$ cargo build --release
```

This gives you a shared library module at `target/release/libniri_taskbar.so`
(pending the rename above).

## Configuration

Standard [CFFI Waybar module][cffi] configuration:

```jsonc
{
  "modules_left": ["cffi/niri-taskbar"],
  // ...
  "cffi/niri-taskbar": {
    "module_path": "/your/path/to/libniri_taskbar.so",
  },
}
```

See [upstream's README][upstream] for application highlighting, multi-output
support, notification integration, and styling — all inherited as-is for now.

## Credit

Forked from [LawnGnome/niri-taskbar][upstream] (MIT), which did the actual
hard work: the niri IPC event stream, the CFFI/GTK plumbing, icon lookup,
and notification matching. Colonnade's job is the tab strip on top of that
foundation.

[cffi]: https://github.com/Alexays/Waybar/wiki/Module:-CFFI
[upstream]: https://github.com/LawnGnome/niri-taskbar
