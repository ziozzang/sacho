#!/bin/sh
# Read-only process presence; requires ps and awk.
# Match the exact process command name, not a substring of its arguments.
# Set the ordinary input PROCESS_NAME to the daemon name on the executing node.
# /healthy >= 1 is the health condition, /value is the matching process count.
set -eu
: "${PROCESS_NAME:?Set PROCESS_NAME in this check's inputs}"
ps -eo comm= | awk -v wanted="$PROCESS_NAME" '
  {sub(/^[[:space:]]+/, ""); sub(/[[:space:]]+$/, ""); if ($0 == wanted) count++}
  END {printf "{\"healthy\":%d,\"value\":%d}\n", (count > 0), count}
'
