#!/usr/bin/env bash
# Captures everything needed to say whether two benchmark runs are actually
# comparable: tool versions, hardware, and the exact parameters used. Called
# by baseline.sh at the end of each run and written as env.json alongside
# the samples.
#
# Usage: env.sh <fixture_count> <duration> <interval> <churn_rate>
set -euo pipefail

fixture_count="${1:?fixture_count}"
duration="${2:?duration}"
interval="${3:?interval}"
churn_rate="${4:?churn_rate}"

repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
git_commit=$(git -C "$repo_dir" rev-parse --short HEAD 2>/dev/null || echo "unknown")
git_dirty=$( [[ -n "$(git -C "$repo_dir" status --porcelain 2>/dev/null)" ]] && echo "true" || echo "false" )

niri_version=$(niri msg -j version 2>/dev/null || niri --version 2>&1 || echo "unknown")
waybar_version=$(waybar --version 2>&1 || echo "unknown")
cpu_model=$(grep -m1 "model name" /proc/cpuinfo 2>/dev/null | sed 's/.*: //' || echo "unknown")
cpu_count=$(nproc 2>/dev/null || echo "unknown")
mem_total_kb=$(grep -m1 MemTotal /proc/meminfo 2>/dev/null | awk '{print $2}' || echo "unknown")

cat <<EOF
{
  "captured_at": "$(date -u +%FT%TZ)",
  "colonnade_commit": "$git_commit",
  "colonnade_dirty": $git_dirty,
  "niri_version": $(python3 -c 'import json,sys; print(json.dumps(sys.argv[1]))' "$niri_version"),
  "waybar_version": $(python3 -c 'import json,sys; print(json.dumps(sys.argv[1]))' "$waybar_version"),
  "kernel": "$(uname -srm)",
  "cpu_model": $(python3 -c 'import json,sys; print(json.dumps(sys.argv[1]))' "$cpu_model"),
  "cpu_count": $cpu_count,
  "mem_total_kb": $mem_total_kb,
  "params": {
    "fixture_window_count": $fixture_count,
    "duration_seconds": $duration,
    "sample_interval_seconds": $interval,
    "churn_events_per_second": $churn_rate
  }
}
EOF
