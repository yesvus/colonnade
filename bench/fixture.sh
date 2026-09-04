#!/usr/bin/env bash
# Opens/closes a fixed, tagged set of foot windows so the benchmark measures
# against a known workload instead of whatever happens to be open on the
# machine running it. This is what makes results comparable across machines
# and across runs.
#
# Usage:
#   fixture.sh open <count>   # opens <count> windows, writes their niri
#                              # window IDs to .fixture-ids, prints them
#   fixture.sh close          # closes every window listed in .fixture-ids
set -euo pipefail

cd "$(dirname "$0")"
ids_file=".fixture-ids"
title_prefix="colonnade-bench-"

cmd="${1:?usage: fixture.sh open <count> | close}"

case "$cmd" in
    open)
        count="${2:?usage: fixture.sh open <count>}"
        [[ -f "$ids_file" ]] && { echo "error: $ids_file already exists; run 'fixture.sh close' first" >&2; exit 1; }

        for i in $(seq 1 "$count"); do
            foot --title "${title_prefix}${i}" -e sleep infinity &
            disown
        done

        # Poll until niri reports all of them, rather than a fixed sleep --
        # window creation time varies with system load, and a fixed sleep
        # would be exactly the kind of non-reproducible fudge factor we're
        # trying to avoid.
        deadline=$(( $(date +%s) + 15 ))
        ids=""
        while [[ $(date +%s) -lt $deadline ]]; do
            ids=$(niri msg -j windows | python3 -c "
import json, sys
title_prefix = sys.argv[1]
wins = json.load(sys.stdin)
matched = [w['id'] for w in wins if (w.get('title') or '').startswith(title_prefix)]
print(' '.join(str(i) for i in matched))
" "$title_prefix")
            got=$(wc -w <<< "$ids")
            [[ "$got" -eq "$count" ]] && break
            sleep 0.2
        done

        got=$(wc -w <<< "$ids")
        if [[ "$got" -ne "$count" ]]; then
            echo "error: expected $count fixture windows, niri reports $got after 15s" >&2
            exit 1
        fi

        echo "$ids" | tr ' ' '\n' > "$ids_file"
        # Let allocations settle before anyone measures against this fixture.
        sleep 2
        echo "$ids"
        ;;
    close)
        [[ -f "$ids_file" ]] || { echo "error: $ids_file not found; nothing to close" >&2; exit 1; }
        while read -r id; do
            [[ -z "$id" ]] && continue
            niri msg action close-window --id "$id" >/dev/null 2>&1 || true
        done < "$ids_file"
        rm -f "$ids_file"
        ;;
    *)
        echo "usage: fixture.sh open <count> | close" >&2
        exit 1
        ;;
esac
