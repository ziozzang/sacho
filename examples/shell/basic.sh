#!/bin/sh
# A standalone check of the executing node's shell and clock.
# Read allowed inputs with "$NAME"; never hard-code credentials.
set -eu
stamp=$(date -u +%Y-%m-%dT%H:%M:%SZ)
printf '{"healthy":1,"display":"%s"}\n' "$stamp"
