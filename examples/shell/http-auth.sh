#!/bin/sh
# For custom HTTP workflows only; standard HTTP/JSON checks are native in Sacho.
# Requires curl and sed. CHECK_URL is ordinary; AUTHORIZATION is an encrypted input.
# Send the header through stdin configuration rather than exposing it in argv.
# The response body is the check output. Select a JSON pointer to publish one scalar.
set -eu
: "${CHECK_URL:?Set CHECK_URL}"
: "${AUTHORIZATION:?Set the encrypted AUTHORIZATION input}"
# Reject line breaks; escape curl's quoted config syntax.
case "$AUTHORIZATION" in
  *"
"*|*"$(printf '\r')"*) printf '%s\n' 'Authorization contains a line break' >&2; exit 1;;
esac
header=$(printf '%s' "$AUTHORIZATION" | sed 's/\\/\\\\/g; s/"/\\"/g')
printf 'header = "Authorization: %s"\n' "$header" |
  curl --config - --fail --silent --show-error --max-time 8 --proto '=http,https' "$CHECK_URL"
