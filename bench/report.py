#!/usr/bin/env python3
"""
Summarizes a bench/*.csv sample file into per-label totals: the combined
RSS/CPU of every matched process at each timestamp (so a leaked duplicate
process shows up as extra footprint, exactly as the user experiences it),
plus a per-process breakdown for diagnosing where the weight comes from.

Usage: report.py <samples.csv>
"""
import csv
import sys
from collections import defaultdict


def mean(xs):
    xs = list(xs)
    return sum(xs) / len(xs) if xs else 0.0


def main():
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        sys.exit(1)

    path = sys.argv[1]
    # totals[label][timestamp] = summed rss/cpu across all matched pids
    totals_rss = defaultdict(lambda: defaultdict(float))
    totals_cpu = defaultdict(lambda: defaultdict(float))
    # per_proc[label][comm] = list of rss samples (one per pid per tick)
    per_proc_rss = defaultdict(lambda: defaultdict(list))
    labels_seen = []

    with open(path, newline="", encoding="utf-8") as f:
        for row in csv.DictReader(f):
            label = row["label"]
            if label not in labels_seen:
                labels_seen.append(label)
            ts = row["timestamp"]
            rss = float(row["rss_kb"])
            cpu = float(row["pcpu"])
            comm = row["comm"]
            totals_rss[label][ts] += rss
            totals_cpu[label][ts] += cpu
            per_proc_rss[label][comm].append(rss)

    for label in labels_seen:
        rss_series = list(totals_rss[label].values())
        cpu_series = list(totals_cpu[label].values())
        print(f"== {label} ==")
        print(
            f"  combined RSS: mean {mean(rss_series) / 1024:.1f} MB, "
            f"max {max(rss_series) / 1024:.1f} MB, "
            f"samples {len(rss_series)}"
        )
        print(
            f"  combined CPU: mean {mean(cpu_series):.1f}%, "
            f"max {max(cpu_series):.1f}%"
        )
        print("  by process:")
        for comm, samples in sorted(
            per_proc_rss[label].items(), key=lambda kv: -mean(kv[1])
        ):
            print(f"    {comm:<24} mean {mean(samples) / 1024:6.1f} MB  n={len(samples)}")
        print()


if __name__ == "__main__":
    main()
