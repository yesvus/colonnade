#!/usr/bin/env bash
# Captures a trusted, reproducible baseline: RSS/CPU of the currently-running
# Python waybar taskbar (niri-tab-daemon.py + its niri msg event-stream
# child + waybar itself), idle and under focus-churn load, against a fixed,
# scripted workload rather than whatever windows happen to be open. Run
# this BEFORE switching to colonnade so there's a real number to beat.
#
# Reproducibility: every run opens/closes its own tagged fixture windows,
# and writes env.json recording tool versions, hardware, and every
# parameter used, so a run from another machine (or six months from now)
# can be checked against this one on equal terms.
#
# Usage: baseline.sh [duration_seconds] [fixture_window_count]
set -euo pipefail

cd "$(dirname "$0")"

duration="${1:-30}"
fixture_count="${2:-8}"
interval=1
churn_rate=4
stamp=$(date -u +%Y%m%dT%H%M%SZ)
outdir="results/${stamp}-baseline-python"
mkdir -p "$outdir"

pattern='waybar$|niri-tab-daemon\.py|niri msg -j event-stream'

cleanup() {
    ./fixture.sh close 2>/dev/null || true
}
trap cleanup EXIT

echo "Opening $fixture_count fixture windows..."
./fixture.sh open "$fixture_count" > /dev/null
ids_file=".fixture-ids"

echo "Capturing idle baseline for ${duration}s..."
./sample.sh idle "$duration" "$interval" "$outdir/samples.csv" "$pattern"

echo "Capturing churn baseline for ${duration}s (cycling focus across fixture windows)..."
./churn.sh "$duration" "$churn_rate" "$ids_file" &
churn_pid=$!
./sample.sh churn "$duration" "$interval" "$outdir/samples.csv" "$pattern"
wait "$churn_pid" 2>/dev/null || true

./env.sh "$fixture_count" "$duration" "$interval" "$churn_rate" > "$outdir/env.json"

echo "Done. Raw samples: $outdir/samples.csv"
echo "Environment manifest: $outdir/env.json"
python3 report.py "$outdir/samples.csv" | tee "$outdir/report.txt"
