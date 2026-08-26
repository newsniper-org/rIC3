#!/usr/bin/env bash
# Watch for external-swap usage during a scored measurement run.
#
# Why: docs/BASELINE.md §4.0 forbids changing swap configuration mid-run,
# because a borderline instance that starts paging to a spinning external disk
# crosses the 3600 s limit and is scored as a timeout. Against a reference that
# solved it, that registers as a REGRESSION -- and regressions=0 is the release
# gate. A false regression cannot be told from a real one after the fact.
#
# /dev/sda3 (80 GB, external) is active at PRIO -1 while zram0 is at PRIO 100,
# so zram is consumed first and sda3 is a last resort. As long as sda3 usage
# stays at 0 the measurement is unaffected. This records the moment that stops
# being true, together with how many cases had completed, so the affected ones
# can be identified and re-measured rather than silently trusted.
#
# Deliberately free of regexes: an earlier version embedded a character class
# in a `hub start` argument list and the shell mangled it.
#
# Usage:  bench/swap_watch.sh [interval_seconds]
# Output: appends to bench/results/baseline-840/swap-watch.log

set -u

interval="${1:-60}"
results="bench/results/baseline-840/results.jsonl"
log="bench/results/baseline-840/swap-watch.log"

mkdir -p "$(dirname "$log")"

while true; do
    sda3=$(awk '$1 == "/dev/sda3" { print $4 }' /proc/swaps)
    zram=$(awk '$1 == "/dev/zram0" { print $4 }' /proc/swaps)
    avail=$(awk '$1 == "MemAvailable:" { print $2 }' /proc/meminfo)
    cases=$(wc -l < "$results" 2>/dev/null || echo NA)

    flag=""
    # Any non-zero external swap usage means the timing model changed.
    if [ -n "${sda3:-}" ] && [ "${sda3:-0}" != "0" ]; then
        flag=" EXTERNAL_SWAP_IN_USE"
    fi

    printf '%s sda3_kb=%s zram_kb=%s memavail_kb=%s cases=%s%s\n' \
        "$(date -Is)" "${sda3:-NA}" "${zram:-NA}" "${avail:-NA}" \
        "${cases// /}" "$flag" >> "$log"

    sleep "$interval"
done
