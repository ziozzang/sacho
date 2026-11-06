#!/bin/sh
# Proves a Sacho deployment's identity boundary from the outside. Exit 0 = all passed.
#
#   check-proxy.sh BASE_URL ADMIN_EMAIL 'ADMIN_COOKIE' ['READER_COOKIE']
#
# BASE_URL      public address, e.g. https://status.example.com
# ADMIN_EMAIL   an address listed in [auth].admins (sent as a forged header)
# ADMIN_COOKIE  Cookie header value of a real administrator session
# READER_COOKIE optional session of a signed-in person who is not an administrator
# IDENTITY_HEADER  environment override when [auth].email_header is not the default
# CURL_OPTS        extra curl options, e.g. "--cacert ca.pem"
#
# Nothing is changed on the server: the write probes are refused before they act.
set -u
base=${1%/}
email=$2
admin_cookie=$3
reader_cookie=${4:-}
header=${IDENTITY_HEADER:-X-Forwarded-Email}
failures=0

code() { # curl arguments; prints the HTTP status
    # shellcheck disable=SC2086
    curl -sS -o /dev/null -w '%{http_code}' --max-time 15 ${CURL_OPTS:-} "$@"
}
expect() { # description, accepted codes (space-separated), curl arguments
    what=$1 accepted=$2
    shift 2
    got=$(code "$@") || got=error
    case " $accepted " in
    *" $got "*) printf 'PASS %s (%s)\n' "$what" "$got" ;;
    *)
        printf 'FAIL %s: got %s, want %s\n' "$what" "$got" "$accepted"
        failures=$((failures + 1))
        ;;
    esac
}

expect "status page is public" 200 "$base/"
expect "health endpoint is public" 200 "$base/healthz"
expect "status API is public" 200 "$base/api/status"
expect "admin page needs a session" "302 303 307 401 403" "$base/admin"
expect "admin API needs a session" "302 303 307 401 403" "$base/api/admin/items"
expect "forged identity gets no admin API" "302 303 307 401 403" \
    -H "$header: $email" "$base/api/admin/items"
expect "forged identity with a junk cookie gets no admin API" "302 303 307 401 403" \
    -H "$header: $email" -H "Cookie: session=forged" "$base/api/admin/items"
expect "admin session reads the admin API" 200 -H "Cookie: $admin_cookie" "$base/api/admin/items"
expect "admin session reads the admin page" 200 -H "Cookie: $admin_cookie" "$base/admin"
expect "write without X-Sacho-Request is refused" 403 -X POST \
    -H "Cookie: $admin_cookie" -H 'Content-Type: application/json' -d '{}' "$base/api/admin/items"
expect "write from a foreign Origin is refused" 403 -X POST \
    -H "Cookie: $admin_cookie" -H 'X-Sacho-Request: 1' -H 'Origin: https://attacker.invalid' \
    -H 'Content-Type: application/json' -d '{}' "$base/api/admin/items"
expect "push endpoint is reachable and wants a token" "400 401 422" -X POST \
    -H 'Content-Type: application/json' -d '{"status":"operational","data":{}}' \
    "$base/api/push/no-such-monitor"
expect "worker endpoint is reachable, without a sign-in redirect" "400 401 403 404 415 422" -X POST \
    -H 'Content-Type: application/json' -d '{}' "$base/api/workers/no-such-node/claim"
if [ -n "$reader_cookie" ]; then
    expect "non-administrator session gets no admin API" 401 \
        -H "Cookie: $reader_cookie" "$base/api/admin/items"
    expect "non-administrator cannot claim admin rights with a header" 401 \
        -H "Cookie: $reader_cookie" -H "$header: $email" "$base/api/admin/items"
fi

[ "$failures" -eq 0 ] && echo "All checks passed." || echo "$failures check(s) failed."
[ "$failures" -eq 0 ]
