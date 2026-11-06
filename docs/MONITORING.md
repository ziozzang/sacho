# Monitoring configuration

Read this guide offline with `sacho help monitoring`. Controllers and agents use
the same binary; update agents before assigning the optional fields below.

## Numeric warning and critical thresholds

Admin → Monitors → Edit provides a severity JSON pointer, direction and optional
warning/critical numbers. Blank thresholds disable the rules. Thresholds are
inclusive. For lower-is-worse allowance:

```json
{
  "value_rules": {
    "pointer": "/remaining_percent", "direction": "low",
    "warning": 20, "critical": 5
  },
  "publish_value": true, "display_pointer": "/display",
  "publish_trend": true, "trend_pointer": "/remaining_percent",
  "failure_status": "degraded"
}
```

At 20 or below the check is a warning; at 5 or below it is critical. `high` reverses
the direction, for example CPU or latency. Critical must be at least as severe
as warning. These are generic rules applied to the JSON output, including remote
checks. They never hide the scalar. Existing HTTP/exit/text/numeric conditions
still apply; a value rule cannot make another failure healthy. Missing or
non-numeric rule values fail using `failure_status`, with an admin explanation.
Existing consecutive failure/recovery gates apply to the final severity.

## Trends and freshness

Numeric history is public **only** with `publish_trend=true`. Select an explicit
`trend_pointer`; if blank, the rule pointer, display pointer, then condition
pointer is used. Each monitor publishes one numeric series. String summaries
can remain separate from numeric charts. Public charts show bucket average,
minimum/maximum on hover or keyboard focus, and configurable threshold lines.
The chart selector also shows response-time history. Missing samples are gaps,
not zeroes. Charts follow the page's existing range and supported precision.
Changing the numeric pointer clears previous scalar samples while preserving
check outputs and availability history, so different definitions are not mixed.
History begins again with the next sample of the new definition.

Each card shows the latest observation, last healthy check and next scheduled
check. The schedule is an estimate: queue capacity and agent availability can
make it later. Stale data is explicit. A retained scalar uses its own original
observation time; it is never labeled as a fresh successful query.

## Agent health

Admin → Agents reports connection state, last contact, reported binary version
and active leased checks. A dedicated authenticated heartbeat runs every 10
seconds, including during long probes. Idle claims also update contact. Contacts
older than 30 seconds are disconnected; a node that never contacted the server
is unknown. Agent heartbeat failure does not rewrite the monitored target's
history. Revoked agents remain revoked. Version is agent-reported metadata,
not a cryptographic attestation. Older agents still claim jobs but cannot send a
heartbeat during execution; update them for accurate long-running visibility.

## Setup validation, examples and export

Admin → Check templates offers HTTP, JSON/TPS, Prometheus, PostgreSQL and Docker.
Choosing a card opens a draft with its scalar/condition configuration and named
input dependencies; it never executes or saves automatically. Inject inputs on
the chosen node and allow them in that node's `probe_env`. PostgreSQL requires
external `psql`; Lua HTTP, Prometheus and Docker socket requests are embedded.

Declare `required_env` as a JSON array in API configurations, or comma-separated
names in the editor. Validate configuration checks the specification, Lua syntax,
registered agent and local required inputs without executing the scenario.
Common literal `sacho.env(...)` calls and HTTP header references are also checked;
dynamic names should be declared explicitly. Missing inputs are warnings, so a
configuration can be staged before injection. Remote input availability is
checked by execution on the selected agent, not by reading the controller's env.
Test draft executes local scenarios without saving or sending notifications and
shows selected values and pointer results. Run it to validate live JSON pointers.
Saving rejects invalid specifications and invalid Lua syntax.

Admin → Settings → Export safe configuration downloads
`sacho-monitor-metadata.json`. It includes monitor metadata, input reference
names, page layout and notification policy. It excludes all executable sources
and target URLs (which may contain embedded secrets), server auth/SMTP, vault and
environment values, worker credentials, subscriber addresses, webhook URLs and
notification templates. It is a metadata handoff, **not** a complete executable
backup or a blindly importable file. Supply sources and operational inputs
privately on the destination. Use the separate encrypted-vault and private
SQLite/PostgreSQL snapshot procedures for a complete installation backup.

Protected admin endpoints: `GET /api/admin/examples`, `POST /api/admin/validate`
(CheckSpec JSON), `GET /api/admin/export`, and
`GET|PUT /api/admin/notification-policy`. Public status never exposes scripts,
input names or private logs.

## Notification controls

Admin → Notifications configures these durable policy fields:

```json
{"settle_seconds": 30, "cooldown_seconds": 300, "maintenance_until": 0}
```

- `settle_seconds` (0–3600): delay problem notifications. If that same automatic
  monitoring problem resolves before delivery, suppress the delayed problem and
  its recovery notification. History still records both transitions.
- `cooldown_seconds` (0–86400): delay repeated problem notifications for the same
  monitor and destination until the cooldown since its last delivered problem
  has elapsed. A recovery after a delivered problem can be sent immediately.
- `maintenance_until`: server Unix timestamp, at most 30 days ahead. Mute only
  monitoring email/webhook notifications until then. Checks, status and history
  continue. Confirmation mail is unaffected. The admin date/time input uses the
  browser's local time. End maintenance clears the window.

Delayed problem messages use the current severity and latest approved display
scalar at send time. Turning off scalar publication also removes it from pending
problem messages. Defaults are zero, preserving immediate notifications. Existing per-monitor
consecutive gates remain the first defense against noisy observations. Suppressed
jobs appear in the delivery log with a reason and do not count as delivered or
retry after maintenance. Recipients do not receive an orphan recovery when their
problem message was suppressed. A problem that began during maintenance is not
replayed when the window ends. There is no acknowledgement, owner, escalation,
on-call, remediation or incident-management workflow. External webhooks handle
those responsibilities. Delivery remains at least once; state is checked just
before sending, so an in-flight external send cannot be recalled.

## Executable starters and private runtime inputs

The admin **Check templates** catalogue includes standalone **Shell script**, **Python code**
and **Lua script** starters, native ICMP/SNMP/Prometheus/DNS/TCP, external reports,
and Lua adapters for JSON APIs, PostgreSQL and Docker. Selecting a card creates a draft;
**Test draft** runs only the check, and **Save service** enables its schedule. Export
starters without a checkout with `sacho template shell`, `python`, `lua` or `ping`.
Python needs `python3`, Shell needs `/bin/sh`, and native ping needs ICMP socket permission. Lua and network protocols are embedded. A blocked ping does not imply a failed HTTP service.

The admin **Environment & secrets** tab lists names, injection permission, whether a value is stored and encrypted, and whether the effective source is the service environment,
vault, or missing. Encrypted values are never returned; explicitly ordinary values have plaintext readback. Initialize a vault with
`sacho secrets init --vault /private/inputs.enc --key-file /private/unlock.key`, configure
`[secrets]`, and restart once to enable editing. The unlock key stays outside the database
and should be backed up separately from the encrypted file. Register, rotate, disable
or delete input values in the admin. Subsequent checks use the updated value without
restarting. Changes do not modify inputs already captured by a running process.

New admin-managed input names grant local probe access explicitly; the grant names are
stored in SQLite, encrypted values in the vault and explicitly ordinary values in SQLite. Environment values take precedence.
Arbitrary inherited service environment names cannot be exposed through this UI;
existing environment inputs need an explicit `probe_env` entry in server configuration.
`PATH`, `LANG`, `SACHO_PROBE_*`, configured worker credentials and SMTP passwords cannot
be managed as probe inputs. Configuration-owned allowlist entries cannot be disabled
from the UI; deleting their vault value does not remove an overriding environment value.
Remote workers keep their own allowlists/vaults; the controller does not distribute secrets.

Read inputs as `"$API_TOKEN"` in Shell, `os.environ["API_TOKEN"]` in Python, or
`sacho.env("API_TOKEN")` in Lua. Never embed credentials into scripts, URLs or action
JSON bodies. Probe execution logs are private, but printing a credential can still expose
it to administrators. Safe metadata exports omit actions, sources, PromQL, DNS target
configuration and secret values. Complete database backups are private operational data.

## Native monitoring sources

- **Prometheus**: `kind="prometheus"`, `target` is the server base URL (including any
  reverse-proxy prefix), and `prometheus_query` is the PromQL expression. Sacho calls the
  instant `/api/v1/query` endpoint, requiring exactly one scalar or vector series.
  Aggregate multi-series results in PromQL, for example `sum(rate(http_requests_total[5m]))`.
  Empty, ambiguous and non-finite results fail the check. The normalized output is
  `{"value":42.0,"sample_at":...}`. Use `/value` for latest value, numeric rules and trend.
  Use `headers_env` for authentication. This queries an existing Prometheus;
  Sacho does not embed a PromQL engine, scraper or time-series database.
- **DNS**: `kind="dns"`, `target` is the queried domain, `dns_record_type` is A, AAAA,
  CNAME, TXT, MX, NS, SOA, SRV, PTR or CAA. `dns_server` is an optional DNS server IP or
  IP:port (IPv6 with a port must be bracketed). An empty server uses system resolvers.
  Each check makes a fresh lookup; Sacho's hosts-file lookup and resolver cache are disabled.
  `dns_expected` is an optional exact answer set, order-independent, using DNS presentation
  syntax. Domain-valued answers include their trailing dot; MX/SRV include numeric fields;
  TXT uses quoted presentation text. Leave it empty to accept any nonempty answer.
  NXDOMAIN, an empty answer, a mismatch, timeout or resolution failure fails the check.
  `/display` contains the answer text and `/count` the count. This is recursive DNS lookup,
  not DNSSEC validation or a full zone transfer.
- **TCP**: `kind="tcp"`, `target="example.com:443"` or `[2001:db8::1]:443`.
  `/value` is connect time in milliseconds, including hostname resolution when applicable.
  The socket is closed after connection; no application protocol or TLS handshake is tested.
  Use HTTP or a custom Lua scenario for application-level verification.

These native checks also run on registered agents with the updated executable. Script
permission is not required for these built-in network probes. PostgreSQL, Docker,
custom protocol, filesystem and device scenarios remain available through trusted scripts.
Native SNMP supports read-only v2c/v3 GET with numeric OIDs; see its options below. MIB-name resolution, SET, bulk discovery and trap reception are not implemented.

## External reports / heartbeat

Create a **Push report** check (`kind="push"`, `target="push"`), set `interval_seconds`
to the maximum allowed report interval, save it, and issue its **Reporting token**.
The token is shown once; only its SHA-256 hash is stored. Reissuing invalidates the old key,
and **Revoke reporting token** disables ingestion. Disabled/deleted checks reject reports.
Each key is bound to exactly one monitor. Keep it in the sending process's secret storage.

Send an authenticated request using a secret header supplied by your client:

```http
POST /api/push/MONITOR_ID
Authorization: Bearer REPORTING_TOKEN
Content-Type: application/json

{"status":"operational","data":{"value":123,"version":"1.2.3"}}
```

`status` is `operational`, `degraded` or `outage`; `data` is the JSON output to record.
Numeric health conditions and value rules also apply. Select `/value` or `/version` for
public display; raw reports remain private. Reports are at most 64 KiB. Receipt time is
server time, and latency is recorded as zero because push does not measure response time.
The endpoint returns `{"ok":true,"recorded_at":...}`. No administrative session is needed;
the reporting token is mandatory. Never put the token in a URL or commit it.

A new monitor allows one interval plus its initial wait for the first report. Missing
reports create failure observations once per interval, using the configured consecutive
failure gate. Last known values remain visible with their original sample timestamp.
A subsequent report can recover the check. Push checks do not use cron, remote agents or
draft execution. Token rotation is not enrollment: reporting keys are reusable until revoked.

## Cron and display timezones

Leave `cron` empty for interval checks. A five-field expression such as `*/5 * * * *`
runs every five minutes; six fields add seconds, and seven add a year. Set
`schedule_timezone` to an IANA name such as `UTC` or `Asia/Seoul`. Scheduling uses that
zone's daylight-saving rules. Set `initial_delay_seconds` for startup grace after monitor
creation. There is one execution at a time per monitor; missed slots after downtime are
coalesced into a single check. **Run now** overrides the schedule, but respects a post-action
hold. Remote agents use the controller's due times and existing fenced leases.

The header has separate language and display-timezone dropdowns. These preferences are
saved in the browser. Display timezone changes timestamps without changing cron execution
zones. Maintenance's datetime input remains in the browser's local timezone; its value is
stored as an absolute timestamp. Server times and API timestamps remain UTC Unix seconds.

## Per-check success and failure actions

In **Failure and recovery**, expand **On failure** or **On success / recovery**. Actions
are opt-in and default to none, including all existing monitors. Supported kinds are
Shell, Python, embedded Lua, or an HTTP(S) POST with a JSON Jinja template. Both HTTP and
HTTPS are accepted so private local endpoints remain usable. These actions execute on the
**controller**, including actions attached to remote observations. They use controller
inputs; they do not inherit agent credentials. Draft tests never queue or execute actions.

- **After consecutive checks** is combined with the monitor's existing failure/recovery
  threshold; the larger count must be reached.
- **Transition / threshold reached** executes once at that consecutive count. Initial
  success can trigger a success action, as can recovery; editing a monitor resets streaks.
- **Repeat** can run while the condition persists, with at least a five-second cooldown.
- **Delay before action** postpones execution. A recovery, pause, delete or configuration
  revision change suppresses a pending action whose condition is no longer valid.
- **Cooldown** bounds how often a new action is queued for that monitor and trigger.
- **Maximum attempts** is 1 by default, up to 5. Failed attempts use **Retry interval**.
  A retry keeps the same delivery identity. HTTP receives `Idempotency-Key`; scripts receive
  the identity inside the event JSON. Actions must be idempotent if retries are enabled.
- **Wait after action** pauses further scheduled checks after each attempt. This permits
  restart/startup grace. Already running probes are not cancelled by a hold.
- **Timeout** bounds each attempt. Script descendants are terminated on timeout/shutdown.

The action queue and attempts persist in SQLite. Attempts are recorded before execution;
a crash can leave an uncertain result. With one attempt, an uncertain action is not retried.
Explicit retries can run again after a crash, so exactly-once external side effects are
not guaranteed. In-flight actions cannot be recalled by an administrative edit. Monitoring
notification maintenance does not mute recovery actions; disable the action or pause its
monitor when execution must stop. Action history appears in the delivery log with actual
attempt counts and generic errors; script output is not added to public monitoring values.

Scripts receive `SACHO_EVENT_JSON`, with `event`, `item_id`, `name`, `status`,
`previous_status`, `at`, `latency_ms`, the selected `display_value`, `attempt` and
`delivery_id`. Lua can decode it using `sacho.json_decode(sacho.env("SACHO_EVENT_JSON"))`.
JSON templates use the same context, e.g.:

```jinja
{"event":{{ event|tojson }},"monitor":{{ name|tojson }},"status":{{ status|tojson }}}
```

No raw probe output or credential values are part of this context. JSON templates are
strict and bounded. HTTP authentication uses action-specific `headers_env` references.
Actions supplement common verified-subscriber mail and webhooks; existing notification
settle/cooldown/maintenance rules still govern those shared notifications.

## Exact admin API contract for setup automation

All `/api/admin/*` endpoints require the configured administrator identity. Mutations
require `X-Sacho-Request: 1`; a supplied browser Origin must match the configured canonical
origin or `public_origins`. Use the established authenticated proxy session. Do not assume
that forged identity headers sent to a public proxy confer administrator access.

| Endpoint | Method | Body / response |
| --- | --- | --- |
| `/api/admin/examples` | GET | Catalogue array with `id`, `title`, `spec`, `inputs`, `requirements` |
| `/api/admin/validate` | POST | A complete check spec; returns `valid`, `errors`, `warnings`, `required_env` |
| `/api/admin/test` | POST | A local check spec; returns observation and validation without saving or actions |
| `/api/admin/items` | POST | A check spec; returns its generated `id` |
| `/api/admin/items/{id}` | PUT | Replacement check spec; resets streaks and invalidates queued old-revision actions |
| `/api/admin/inputs` | GET | `vault_configured`, `scope:"local"`, and `inputs` metadata; encrypted values omitted, ordinary values readable |
| `/api/admin/inputs/{name}` | PUT | `{"value":"new secret","enabled":true,"encrypted":true}`; omit `value` to change injection permission only |
| `/api/admin/inputs/{name}` | DELETE | Removes the vault value and dynamic grant; preserves server configuration |
| `/api/admin/push/{id}/token` | POST | Returns one-time-visible `token`, `endpoint`, `shown_once`; replaces the previous key |
| `/api/admin/push/{id}/token` | DELETE | Revokes the reporting key |

Read a catalogue `spec`, modify only the needed fields, validate, then save. Store real
secrets through the protected inputs endpoint, never in a saved `target`, `actions[].body`
or literal HTTP header. API errors do not echo secret values. GET cannot recover
a previously stored encrypted secret; retain it in the operator's own secret system if needed.

A check's optional `actions` array supports one entry per trigger (at most two):

```json
{
  "trigger":"failure",
  "mode":"transition",
  "kind":"https",
  "target":"https://example.com/monitor-hook",
  "body":"{\"event\":{{ event|tojson }},\"status\":{{ status|tojson }}}",
  "headers_env":{"Authorization":"HOOK_AUTHORIZATION"},
  "after_count":3,
  "delay_seconds":10,
  "cooldown_seconds":300,
  "max_attempts":1,
  "retry_seconds":30,
  "wait_after_seconds":30,
  "timeout_seconds":15
}
```

`trigger` is `success|failure`, `mode` is `transition|repeat`, and `kind` is
`shell|python|lua|https`. The `https` kind represents HTTP(S) POST; `target` for scripts is
the script source. Counts are 1..100, attempts 1..5, timeout 1..300 seconds, timing fields
0..86400 except retry 1..86400. Repeat mode requires cooldown >=5. These controls do not
change the monitoring observation into an action result. Test the desired target in an
isolated environment before enabling actions on a live monitor.

## Source-aware controls and graph presentation (v0.1.10)

The check editor hides and disables source-specific controls: PromQL only for Prometheus,
DNS options only for DNS, expected HTTP status/credential headers only for HTTP/Prometheus,
and exit code only for scripts. Push hides local execution and cron controls. Optional
value/trend selections enable their dependent inputs. Inactive malformed HTTP-header JSON
cannot block a script save. Values/graphs can be folded; the Values section shortcut opens
its fold. Existing configured rules/trends open automatically. Public numeric graphs have
independent folds, metric/latency selection and Smooth/Straight line choices, retained while
polling. Smooth uses shape-preserving interpolation through real bucket values; missing
buckets split paths, and hover markers still report actual bucket averages/min/max.

## Plain and encrypted input choices; bindings owned by one check

The global Environment & secrets panel has an **Encrypt secret value** checkbox.
`PUT /api/admin/inputs/{name}` adds `encrypted` (default true for backward compatibility).
With false, the value is ordinary plaintext in SQLite and may be returned in the admin GET.
With true, it is stored in the encrypted vault and GET returns no value. Changing encryption
mode requires a newly supplied value; unchecking the box does not reveal a stored secret.
Ordinary global inputs work without a configured vault. Secrets require vault initialization.
Deleting removes both storage and dynamic injection permission, subject to configuration-owned
allowlists. Runtime changes take effect on subsequent executions, including after restart.

In a check's Inputs section, add rows with a name, value and encryption choice. These
bindings override same-name global inputs only for that check and its controller actions.
They do not grant other checks access. Saved encrypted values stay blank in the editor;
leave blank to retain, type a value to rotate, or remove a row to delete. Plain values are
readable/editable. Secret rotations create immutable new vault entries before atomically
changing the check spec; old entries are cleaned up after successful save. A failed save
cleans newly staged entries. Monitor deletion also cleans its encrypted input entries.
An interrupted save/cleanup can leave unused encrypted entries; names with `SACHO_ITEM_`
are internal and never appear as reusable global inputs. Vault data/key backups remain
private; safe metadata exports omit all binding values and reference maps.

Check-save and local draft-test bodies accept transient `input_updates`:

```json
"input_updates": [
  {"name":"CHECK_URL","value":"https://example.com/metrics","encrypted":false},
  {"name":"AUTHORIZATION","value":"Bearer YOUR_PRIVATE_TOKEN","encrypted":true}
]
```

Updates are never serialized into saved specs or API readback. Saved `input_bindings`
contain ordinary values or encrypted internal references; retain these entries unchanged
when editing, remove a key to delete a binding, and use `input_updates` to replace values.
An encrypted reference cannot be rebound to a different monitor. Draft tests resolve supplied
inputs in memory without writing vault/SQLite. Arbitrary scripts can still print inputs;
avoid putting credentials in probe output or selecting them for public display.

For remote checks, inline ordinary inputs are carried over the authenticated worker channel.
Controller-encrypted bindings are rejected. Configure secret inputs in the agent's own
allowlist/vault and reference their variable names in the check; the UI explains this and
disables inline remote encryption. Controller actions use controller-owned inputs.

## Native ICMP and SNMP options

`kind="ping"` uses ICMP echo directly in Rust; no Lua or external ping executable.
The target is a hostname or IPv4/IPv6 address. Optional `ping` settings are
`count` (1..10, default 3), `min_replies` (1..count, default 1), `wait_ms`
(100..10000, default 1000), `interval_ms` (100..5000, default 100). Their worst-case
sum must fit `timeout_seconds`. The check fails if fewer than min_replies return.
Output includes `healthy`, `rtt_ms` (mean actual ICMP RTT), `rtt_min_ms`, `rtt_max_ms`,
`loss_percent`, `sent`, `received`, and a safe summary `message`. No replies means RTT is
null rather than fabricated zero. Select `/rtt_ms` for display/trend or `/loss_percent`
for packet-loss rules. Hostname targets use one resolved address per check; use separate
checks to verify both IP families. The executing node must permit Linux ping sockets
(`ping_group_range`) or have appropriate raw-socket capability. Sacho does not change
system permissions automatically, and ICMP blocking does not imply a failed HTTP service.

`kind="snmp"` targets `host:161` or `[IPv6]:161`. Its `snmp` object supports:

```json
{
  "version":"3", "oid":"1.3.6.1.2.1.1.3.0", "username":"monitor-user",
  "security":"authPriv", "auth_protocol":"sha256",
  "auth_env":"SNMP_AUTH_PASSWORD", "privacy_env":"SNMP_PRIVACY_PASSWORD"
}
```

For version `2c`, use `community_env` (default SNMP_COMMUNITY). Version `3` supports
noAuthNoPriv, authNoPriv and authPriv; authentication supports sha1/sha256 and privacy
uses AES128. Password fields are input names, never credential literals. Engine discovery
and bounded engine-time synchronization happen natively. Output is `value`, `type`, `oid`;
select `/value` for latest scalar/rules/trends. Numeric types include signed integers,
counter32/counter64, gauge32 and timeticks (hundredths of a second); text, IP and OID
scalars are displayable. Missing OIDs, authorization errors, unsupported structured values
and timeout fail the check. Only GET is used: no SET, external snmpget, MIB database,
trap daemon or device discovery. SNMPv2c sends community information without protocol
privacy; use v3 authPriv where needed. These native probes also run on updated agents
without enabling script execution.

The preferred catalogue API is `/api/admin/check-templates`; `/api/admin/examples` is
retained as an alias. CLI uses `sacho template`, with `example` retained as an alias.
Available native exports include `ping` and `snmp`. Detailed Shell exports are
`shell-disk`, `shell-process`, `shell-http`: read-only filesystem percent (df/awk), exact
process presence (ps/awk), and custom authenticated HTTP (curl/sed). Authorization is
provided to curl through stdin configuration rather than argv. Standard HTTP/JSON uses
a native template. Lua adapters are explicitly labeled custom; select them only for
custom scenarios. Templates open drafts and never enable monitoring by themselves.

SNMPv3 authentication/privacy passwords must have at least eight bytes. Short or empty values fail safely before protocol key derivation.

### v0.2: Remembered charts

The public page starts at **1 hour** with a supported resolution (expanding the
range if all checks run less often). “Remember view” stores only range, resolution,
per-monitor series, line style and folds in this browser's local storage. It works
for signed-in and anonymous visitors, stores no account or authentication tokens,
and can be disabled to clear these preferences. Language/timezone remain their
existing separate preferences. Sacho administrator login lifetime is controlled
by your authentication proxy; this setting does not extend it.

Graphs connect observations within the greater of the bucket width and the
monitor's configured interval. Longer missing stretches stay disconnected; bars
still report unobserved buckets as no data. Smooth lines require at least three
points in a connected segment. If no numeric values exist, a new graph selection
uses response time. An explicitly selected numeric series still shows no data
when no numeric values are available. Changing style or series redraws immediately.

### v0.2: authenticated data visibility

Check `visibility` is `public` (the existing default), `authenticated`, or `groups`.
`access_groups` lists group identifiers for the last mode: matching any group grants
read access. Administrators configured in the authentication proxy settings can
read all monitors. Groups never grant administrator/edit/run privileges.

Admin **Settings → Data access groups** maps identifiers to exact member email
addresses. Members are normalized to lowercase; wildcards and client-supplied group
claims are not supported. The same group can grant several monitors. Group changes
and revocations take effect on the next server request without a restart. Missing
identity, unknown groups and malformed/duplicate identity headers do not grant access.

Every status, history and incident-history request evaluates the current verified
proxy email against the current group matrix. Unauthorized items are omitted from
status, layout IDs/titles and incident history; direct history requests return 404.
The overall health reflects only the caller's visible monitors. Disabled/deleted
monitors and their history are not publicly exposed. Restricted monitors never
queue public subscriber email, and pending email jobs recheck the current grade
before delivery. Administrator-configured webhooks and per-check actions remain
privileged operational integrations and can receive restricted monitoring events.

This is application-level data authorization built on the existing trusted auth
proxy, not a replacement for identity verification. The proxy must authenticate
users, strip inbound identity headers and supply exactly one verified email on all
public APIs as well as admin routes. Keep Sacho reachable only from that proxy
(loopback/firewall or an authenticated protected upstream); exposing the backend
port lets callers impersonate header identities. The deployed loopback setup uses
this boundary. Protect proxy-to-backend traffic when it crosses hosts. OAuth cookies
remain managed by the proxy; browser local storage is never an access credential.

View storage policy is configurable in admin Settings: all visitors, signed-in
visitors only, or off. Preferences are kept separately for anonymous visitors and
each authenticated identity using a hashed local key, with an optional browser
opt-out. Only display choices are stored, never graph values, roles, email addresses,
or authentication tokens. Permission remains server-side even if storage is edited.
Old check definitions default to public for compatibility; choose a restrictive
visibility explicitly when onboarding sensitive monitors. Worker binaries older
than v0.2 may not accept non-default visibility fields; upgrade them together.
