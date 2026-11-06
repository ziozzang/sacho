#!/bin/sh
# Report a result from a job that Sacho cannot reach. Keep the token out of the URL,
# the command line and logs: this reads it from a private file.
#   push-report.sh https://status.example.com MONITOR_ID /etc/backup/sacho-token 12.5
set -eu
base=$1 id=$2 token_file=$3 size=$4
token=$(cat "$token_file")
curl -fsS --max-time 15 -K - -H 'Content-Type: application/json' \
    -d "{\"status\":\"operational\",\"data\":{\"size_gb\":$size}}" \
    "$base/api/push/$id" <<EOF
header = "Authorization: Bearer $token"
EOF
echo
