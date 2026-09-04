# Benchmark suite

Trusted, reproducible measurement of RSS and CPU for the taskbar, so
"colonnade is lighter than the Python daemon" is a checked-in number, not a
claim.

## Why this needs to be more than `ps`

A one-off `ps` snapshot depends on whatever happens to be open on whoever's
machine runs it, which makes numbers impossible to compare across machines
or across time. This suite fixes that:

- **Fixed workload.** `fixture.sh` opens a known count of tagged, inert
  `foot` windows (`sleep infinity`, titled `colonnade-bench-N`) and polls
  niri until they've actually registered before measurement starts. Nothing
  ambient gets counted.
- **Deterministic load.** `churn.sh` round-robins `niri msg action
  focus-window` across exactly those fixture window IDs at a fixed rate,
  instead of `focus-column-left/right`, whose effect depends on whatever
  else is open.
- **Self-exclusion.** The sampler explicitly excludes its own scripts from
  the process match — an early version of this suite briefly measured
  itself, because the match pattern is passed as one of its own arguments.
- **Recorded environment.** Every run writes `env.json`: colonnade's git
  commit (and whether the tree was dirty), niri/waybar versions, kernel,
  CPU model, core count, memory, and every parameter used for that run. Two
  runs are only comparable if `env.json` says they should be.

## Running it

```bash
./baseline.sh [duration_seconds] [fixture_window_count]   # default: 30 8
```

Captures the currently-installed Python daemon (`niri-tab-daemon.py` + its
`niri msg event-stream` child + `waybar` itself) idle, then under churn.
Fixture windows are always cleaned up on exit, including on failure (trap).

Once colonnade itself is buildable, an equivalent `colonnade.sh` capture
script will measure the CFFI module in-process instead — same fixture, same
`env.sh`, same `report.py`, so the two `report.txt` files are a direct,
apples-to-apples comparison.

Results land in `results/<UTC timestamp>-<label>/`, each with `samples.csv`
(raw), `env.json` (manifest), and `report.txt` (summary). Commit these when
they represent a milestone worth keeping as a reference point — don't commit
every exploratory run.

## Reading `report.py`'s output

"Combined RSS/CPU" sums every matched process at each timestamp before
averaging — so a leaked duplicate process (this repo found one: two
`niri-tab-daemon.py` instances were running simultaneously, ~66 MB for what
should've been one) shows up as extra footprint, exactly as a user
experiences it. The per-process breakdown underneath is for figuring out
*where* the weight comes from once you know the total.

## Known limitation

The fixture is currently 8 identical terminal windows in a single column
group each (no niri column-grouping stress test). Once colonnade implements
column-grouped tabs, extend `fixture.sh` to open windows into deliberately
varied column widths (some tiled 1/3, some 1/2, some fullscreen) so the
benchmark also exercises the proportional-sizing and column-grouping logic,
not just raw window count.
