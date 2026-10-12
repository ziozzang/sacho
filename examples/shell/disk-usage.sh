#!/bin/sh
# Read-only filesystem utilization; requires df and awk.
# CHECK_PATH is an ordinary per-check input, for example / or /srv/data.
# value is used percent; select /value for a graph and high-value thresholds.
set -eu
path=${CHECK_PATH:-/}
report=$(df -Pk -- "$path" | awk '
  NR == 2 {
    percent=$5; sub(/%$/, "", percent)
    if (percent !~ /^[0-9]+$/ || $2 !~ /^[0-9]+$/ || $4 !~ /^[0-9]+$/) exit 1
    printf "{\"healthy\":1,\"value\":%d,\"total_kib\":%s,\"free_kib\":%s}\n", percent, $2, $4
    found=1
  }
  END {if (!found) exit 1}
')
printf '%s\n' "$report"
