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
