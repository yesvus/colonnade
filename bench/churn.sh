#!/usr/bin/env bash
# Generates a steady, deterministic stream of niri focus-change events by
# round-robining focus-window across a known set of window IDs (from
# fixture.sh), rather than focus-column-left/right, whose behavior depends
# on whatever else happens to be open. Run in the background while
# sample.sh runs in the foreground.
#
# Usage: churn.sh <duration_seconds> <events_per_second> <ids_file>
set -euo pipefail

duration="${1:?duration_seconds}"
rate="${2:?events_per_second}"
ids_file="${3:?ids_file}"
sleep_between=$(awk -v r="$rate" 'BEGIN { printf "%.3f", 1 / r }')

mapfile -t ids < "$ids_file"
[[ ${#ids[@]} -ge 2 ]] || { echo "error: churn.sh needs at least 2 fixture windows to alternate between" >&2; exit 1; }

end=$(( $(date +%s) + duration ))
i=0
while [[ $(date +%s) -lt $end ]]; do
    niri msg action focus-window --id "${ids[$i]}" >/dev/null 2>&1 || true
    i=$(( (i + 1) % ${#ids[@]} ))
    sleep "$sleep_between"
done
