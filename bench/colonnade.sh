#!/usr/bin/env bash
# Mirrors baseline.sh exactly (same fixture.sh, same churn.sh, same
# env.sh, same report.py) but captures the Colonnade-loaded waybar
# process instead of the Python daemon's process set -- the two
# report.txt files are meant to sit side by side as a direct,
# apples-to-apples comparison.
#
# Pattern is deliberately just `waybar$`: unlike the Python daemon setup
# (waybar + niri-tab-daemon.py + its own niri msg event-stream child),
# Colonnade runs in-process via CFFI -- there is no second process to
# match. That's not a benchmark artifact, it's the actual point.
#
# Usage: colonnade.sh [duration_seconds] [fixture_window_count]
set -euo pipefail

cd "$(dirname "$0")"

duration="${1:-30}"
fixture_count="${2:-8}"
interval=1
churn_rate=4
stamp=$(date -u +%Y%m%dT%H%M%SZ)
outdir="results/${stamp}-colonnade"
mkdir -p "$outdir"

pattern='waybar$'

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
