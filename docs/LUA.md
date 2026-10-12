# Lua scenarios

[한국어](LUA.ko.md)

The admin console embeds the CodeMirror editor adapted from Hangang. It highlights
Lua, numbers lines, supports undo/redo, brackets and Ctrl+Space completion, and
keeps the underlying textarea synchronized. It is bundled in the executable and
uses a per-response CSP style nonce. No browser-side Lua execution or CDN is used.
**Test draft** runs the server-side script once without persisting its observation.
Changing UI language preserves the draft. Closing the dialog destroys editor state.

Each scheduled Lua run is a fresh, bounded subprocess of `sacho probe-lua`. It uses
16 MiB Lua allocation, the configured timeout (1–300 seconds) and a 64 KiB script
limit. Unix process groups are killed on timeout/cancellation, including descendants.
Shell/Python output is drained but captured only up to 64 KiB per stream; stored
output is capped at 64 KiB. HTTP request/response bodies are capped at 64 KiB.

The standard Lua base/table/string/math/utf8 facilities are available. `os`, `io`,
`package`, `debug`, filesystem access and dynamic external modules are absent.
Host calls can access the operator's network; scripts are written by trusted admins.

## API version 1

| Member | Behavior |
| --- | --- |
| `sacho.api_version` | Number `1`. |
| `sacho.http(url)` | GET; returns `{status, body, latency_ms}`. |
| `sacho.http({url, method, headers, body})` | Explicit method/headers/body. HTTP(S) only, TLS validation enabled, redirects not followed. |
| `sacho.json_decode(text)` | JSON to Lua values/tables. |
| `sacho.json_encode(value)` | Lua value to a JSON string. Use this for conditions/display. |
| `sacho.env(name)` | Read only an environment name explicitly listed in `probe_env`. |
| `sacho.exec({program, arg1, ...})` | Run an external command without shell expansion. Returns `{exit_code, stdout, stderr}`; captures at most 64 KiB per stream and shares the scenario timeout/process group. |

Returning `true`, a number, string or nil exits successfully; `false`, an assertion
failure or runtime error fails. Returned numbers are printed as numeric JSON;
strings are printed as text (including JSON returned by `json_encode`). A table
must be JSON-encoded explicitly. HTTP error status does not throw automatically:
assert the status you actually expect. Connection/TLS/timeouts throw an error.

Do not print credentials. Secrets are not inherited implicitly; add required names
to the configuration allowlist and provide values in the service environment.

```toml
probe_env = ["SACHO_DORANG_BEARER"]
```

```lua
local response = sacho.http({
  url = "http://127.0.0.1:4000/admin/credentials/health",
  headers = {Authorization = sacho.env("SACHO_DORANG_BEARER")},
})
assert(response.status == 200, "Dorang query failed")
local data = sacho.json_decode(response.body)
local tps = 0
for _, credential in ipairs(data.credentials) do
  tps = tps + (credential.tokens_per_sec or 0)
end
return sacho.json_encode({tps = tps})
```

Set `publish_value=true`, `display_pointer="/tps"`, `metric_unit=" TPS"` to display
this latest value. To gate health independently, set `json_pointer="/tps"`,
`comparison="gte"` and `threshold=10`; otherwise omit the threshold. Scalar output
can be displayed directly with an empty display pointer. Text is bounded to 160
non-control characters and escaped by the frontend; objects/arrays are not public.

Provider-specific API and credential handling belongs in external scenarios, never
in the Sacho engine. `exec` permits trusted administrators to invoke external
programs with the service user's filesystem permissions, just like shell/Python
checks. Commands receive only the allowlisted probe environment and fixed
PATH/LANG; stdin is closed. Assert `exit_code == 0` before decoding stdout.
Missing data must be treated as unavailable instead of a successful full allowance.

See [examples/lua](../examples/lua) for Hangang, Dorang, TPS and quota scenarios.

### Values when a check fails

A published scalar remains visible when a later check has no scalar. The API
reports `display_value_at` and `display_value_stale`; the page labels retained
values as **Last known value** with their observation time. No prior value is
shown as **Value unavailable**, never as a fabricated zero. A value in a failed
check is still displayed; health and display extraction are independent.
Set `public_message_pointer="/message"` to publish a short JSON string selected
from the **latest** check output, including failed conditions. This is opt-in:
raw logs and internal errors remain private. Return structured JSON with a
numeric condition instead of asserting before returning it if a failure needs
a public explanation. Never include secrets in this selected field.

### Docker containers through a Unix socket

`sacho example docker` prints an embedded container inspection scenario.
`sacho.http({url="http://localhost/...", unix_socket="/absolute/docker.sock"})`
uses the built-in HTTP client, requires GET without a body, disables proxies
and follows no redirects. Configure `DOCKER_SOCKET` and `DOCKER_CONTAINER` in
the executing node's `probe_env`; use `/healthy gte 1`, publish `/display`, and
optionally publish `/message`. Running containers without a HEALTHCHECK pass;
`starting`, `unhealthy`, stopped and missing containers fail. No Docker CLI,
external Lua library or new dependency is required. A rootless Docker socket
or a remote worker on the Docker host can also be used.

Grant socket access only to trusted monitoring code. Docker socket possession
provides powerful host access; a read-only bind mount does not restrict API
methods. The built-in helper restricts socket requests to GET, but script
execution is not a security sandbox. Use a restricted socket proxy if stronger
isolation is required. Container inspection responses are bounded to 64 KiB;
large responses fail rather than exposing a partial document. Only explicitly
selected scalars and public messages are published, never container environment.

For quota, informational or collector checks, set `failure_status="degraded"`
to treat condition failures, request errors and timeouts as warnings. The default
is `outage`. This changes only severity; failed observations and explanations
remain visible and still trigger monitoring notifications.
