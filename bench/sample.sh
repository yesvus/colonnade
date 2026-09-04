#!/usr/bin/env bash
# Samples RSS/CPU for a set of running processes at a fixed interval and
# appends rows to a CSV. Used by baseline.sh and later by colonnade's own
# post-rewrite capture, so both runs land in the same comparable format.
#
# Usage: sample.sh <label> <duration_seconds> <interval_seconds> <out_csv> <pid_match_regex>
set -euo pipefail

label="${1:?label}"
duration="${2:?duration_seconds}"
interval="${3:?interval_seconds}"
out_csv="${4:?out_csv}"
pattern="${5:?pid_match_regex}"

mkdir -p "$(dirname "$out_csv")"
if [[ ! -s "$out_csv" ]]; then
    echo "timestamp,label,pid,comm,rss_kb,pcpu" > "$out_csv"
fi

end=$(( $(date +%s) + duration ))
while [[ $(date +%s) -lt $end ]]; do
    ts=$(date -u +%FT%TZ)
    # One ps call per tick: pid, rss, cpu%, comm, full args (args last so it
    # can contain spaces without breaking the fixed-width fields before it).
    ps -eo pid,rss,pcpu,comm,args --no-headers | while read -r pid rss pcpu comm args; do
        # Exclude our own bench scripts: their argv literally contains the
        # match pattern text (it's passed as an argument), which would
        # otherwise make the sampler measure itself.
        [[ "$args" == *sample.sh* || "$args" == *baseline.sh* || "$args" == *churn.sh* ]] && continue
        if [[ "$args" =~ $pattern ]]; then
            echo "${ts},${label},${pid},${comm},${rss},${pcpu}" >> "$out_csv"
        fi
    done
    sleep "$interval"
done
