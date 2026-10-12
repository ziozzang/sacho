# Sacho setup how-to for LLM agents

This document is embedded in the executable. Read it with `sacho help setup` or
`sacho setup-howto`; use `--language ko` for Korean. No network, configuration or
running server is required to read it. Ordinary `sacho help serve` still works.

## Handoff prompt

> Configure this installation using `sacho help setup` as your runbook. Inspect
> existing configuration and API state first. Keep operating addresses, accounts,
> scripts, credentials and local TODO files outside Git. Preserve monitor IDs and
> history. Test drafts before saving. Use the documented API and external scenarios;
> do not add service-specific clients to the engine. Build/test locally; do not run
> GitHub Actions unless explicitly requested. Report the resulting configuration,
> validation and unresolved inputs without revealing secret values.

## Inputs and ownership

Discover the existing binary/version, service manager, configuration path, database,
public origin, proxy configuration and named credential sources. Obtain unresolved
inputs from the operator: allowed admin emails, source URLs/queries, polling and
health conditions, SMTP sender/relay/security, optional webhooks and update policy.
Do not guess missing credentials or replace a working deployment wholesale.

The native binary embeds its UI, Lua interpreter, HTTP/JSON host functions, SQLite,
this guide and cryptography. Python/shell/`psql` remain external programs when used.
No Lua package installation is needed for HTTP, JSON, environment or command calls.
All sample domains/emails are fictional. Paths below are examples, not discovery.

## Start or inspect

```sh
sacho --version
sacho help setup --language en
# Fresh local installation only; init refuses to overwrite an existing file:
umask 077
sacho init --config ./sacho.toml
sacho doctor --config ./sacho.toml
sacho serve --config ./sacho.toml
```

The defaults bind `127.0.0.1:8080` with no-auth. Open `/` and `/admin` locally.
`doctor` validates configuration, monitors and SQLite integrity; it can create or
migrate the configured database, so use the intended path. For an existing service,
read its actual configuration and `GET /api/admin/items` before changing anything.
Use absolute database/script/configuration paths in production. Back up config and
SQLite consistently (SQLite backup API, or stop the service before copying files).

## Production configuration

Example configuration; replace each environment-specific input deliberately:

```toml
bind = "127.0.0.1:8080"
database = "/var/lib/sacho/sacho.db"
public_url = "https://status.example.com"
retention_days = 100
max_concurrent_checks = 16
# Top-level: declare before [auth] or other TOML sections.
probe_env = ["SOURCE_BEARER", "CHECK_CONTEXT"]

[auth]
mode = "auth-proxy"
email_header = "X-Forwarded-Email"
admins = ["operator@example.com"]
# Optional proxy endpoint; omit when unsupported:
logout_url = "/identity/logout"

[updates]
enabled = false
interval_seconds = 21600

# Optional email subscriptions:
# [smtp]
# host = "smtp.example.com"
# port = 587
# from = "Status <status@example.com>"
# username = "status@example.com"
# password_env = "SMTP_PASSWORD"
# security = "starttls"
```

External configuration changes require restarting `serve`. The admin settings API
changes public title/description, not authentication/SMTP/environment configuration.
SMTP supports `starttls`, `tls`, or `plain` only on loopback/private IPv4 relays.
SMTP password access does not require adding it to `probe_env`. Subscriptions are
inactive without SMTP; confirmation is required before an email is subscribed.

Proxy `/admin`, `/api/admin/` and `/api/account` through the trusted authentication
service. Strip client identity headers, then inject the verified email. The public
status/history/incidents remain anonymous. For Hopae additionally protect `/_hopae`
on the application host for login handoff and logout, and set `logout_url` to
`/_hopae/logout`. Reuse the existing authentication contract; never copy credentials
into the guide. Test public access, unauthenticated rejection and forged-header
rejection separately. Sacho trusts its private upstream; keep it inaccessible to
untrusted clients. no-auth is accepted only on loopback and is for local operation.

## Inject variables, context and encrypted secrets

Nonsecret inputs can come from a service EnvironmentFile outside the repository:

```text
CHECK_CONTEXT='{"environment":"staging","region":"example"}'
```

Name every required input in top-level `probe_env`. Lua reads `sacho.env("NAME")`;
HTTP checks use `headers_env = { Authorization = "SOURCE_BEARER" }`. That variable
must contain the complete header value, including `Bearer ` if applicable. External
commands inherit the same allowlisted variables plus fixed PATH/LANG. The allowlist
is installation-wide, not a permission boundary between trusted administrators.

For secrets that must not remain grep-readable, use the encrypted local vault.
Create parent directories first, owned by the service user with mode 0700:

```sh
sacho secrets init --vault /etc/sacho/secrets.enc --key-file /etc/sacho/unlock.key
# Feed the value through stdin from your approved credential source:
credential-reader | sacho secrets set SOURCE_BEARER --vault /etc/sacho/secrets.enc --key-file /etc/sacho/unlock.key
sacho secrets list --vault /etc/sacho/secrets.enc --key-file /etc/sacho/unlock.key
```

`credential-reader` is a placeholder for the operator's existing secret source,
not a bundled command. Do not put values in arguments, shell history, logs or Git.
Add to configuration:

```toml
[secrets]
vault = "/etc/sacho/secrets.enc"
key_file = "/etc/sacho/unlock.key"
```

The vault encrypts its JSON map with authenticated encryption. Key and ciphertext
files must be private; protect the key separately and back up both securely. The
key is random key material, not a password. This protects stored secret values
against plaintext search, not an operator who can read both files or process memory.
On startup Sacho decrypts into memory; only `probe_env` names are passed to checks.
The same named lookup works for SMTP passwords and HTTP headers. Existing process
environment values override vault entries. Restart after vault/environment edits.
Never print `sacho.env` values. To inject structured context, JSON-decode
`sacho.env("CHECK_CONTEXT")`; publish only deliberate nonsensitive scalars.

## Register checks through the API

Use an authenticated browser/session through the proxy. An operator with local
access may call the private loopback upstream with its configured identity header;
adding a header to the public URL does not authenticate. Mutation requests require
`X-Sacho-Request: 1` and, if supplied, Origin matching `public_url`. JSON bodies use
`Content-Type: application/json`.

Endpoints:

| Operation | Method and path | Body/result |
| --- | --- | --- |
| Inspect checks | GET `/api/admin/items` | Array of `{id, spec, status, revision, ...}`. |
| Test without saving | POST `/api/admin/test` | CheckSpec; returns status, latency_ms, detail, output, display_value. No observation/incident/alert is persisted, but the script itself executes. |
| Create | POST `/api/admin/items` | CheckSpec; returns `{id}`. |
| Update | PUT `/api/admin/items/{id}` | Full CheckSpec; returns `{ok:true}`. Preserve unmodified fields from existing `spec`. |
| Run saved check | POST `/api/admin/items/{id}/run` | No body; queues a persisted check that can trigger alerts. |
| Inspect logs | GET `/api/admin/items/{id}/logs` | Execution history; keep raw outputs private. |
| Remove | DELETE `/api/admin/items/{id}` | Deletes its observations; avoid for migrations. |
| Public title | PUT `/api/admin/settings` | `{title,description}`. |
| Layout | PUT `/api/admin/layout` | `{sections:[{title,item_ids:[id,...]}]}`. |
| Webhooks | GET/POST `/api/admin/webhooks` | Creation body `{name,url}`; DELETE `/{id}` removes one. |
| Delivery history | GET `/api/admin/notifications` | Delivery/retry outcomes. |

Avoid simultaneous configuration writers; updates have no client revision/CAS
precondition. Match existing IDs deliberately; ambiguous names need resolution.
Create once or update that ID, rather than delete/recreate. Unknown fields are errors.

Example CheckSpec to test the local Sacho health endpoint and display its version:

```json
{
  "name": "Status process", "group": "Infrastructure", "kind": "http",
  "target": "http://127.0.0.1:8080/healthz",
  "interval_seconds": 60, "timeout_seconds": 10,
  "expected_status": 200, "enabled": true,
  "publish_value": true, "display_pointer": "/version",
  "display_label": "Version", "metric_unit": "", "threshold": null
}
```

Use POST `/api/admin/test` first; expect operational and a version display. Save
only when its meaning matches the desired monitor. Poll logs/public data until the
saved check has a fresh observation; a queued response alone is not a success.

CheckSpec fields (omitted fields use defaults):

| Fields | Meaning and limits |
| --- | --- |
| name, description, group | Name required, at most 120 bytes; description 2000, group 120. |
| kind, target | http/shell/python/lua; nonempty URL/script, at most 64 KiB. |
| interval_seconds, timeout_seconds | Interval 5..86400; timeout 1..300 and no greater than interval. |
| expected_status, expected_exit, contains | HTTP code defaults 200, command exit 0, optional substring. |
| headers_env | HTTP header names mapped to allowlisted environment names; no inline header secret. |
| json_pointer, comparison, threshold | Numeric condition; lt/lte/gt/gte/eq. Empty pointer reads a numeric scalar; null threshold disables numeric gating. |
| degraded_ms | Optional latency threshold in milliseconds. |
| failure_threshold, recovery_threshold | Consecutive failures/successes required, 1..100, defaults 1. |
| enabled | Default true. |
| publish_value, display_pointer, display_label, metric_unit | Display-only scalar extraction; separate from health gating. Public display is opt-in. |

For TPS JSON `{tps:42.5}`, set display_pointer `/tps`, unit ` TPS`, and threshold
null for display only. To gate health use json_pointer `/tps`, comparison `gte`,
threshold 10. Version strings use `/version` with no numeric threshold. Missing
values are unavailable, never fabricated as zero or a full allowance.

## Lua and external data sources

```lua
local context = sacho.json_decode(sacho.env("CHECK_CONTEXT"))
local result = sacho.exec({"python3", "/opt/checks/health.py"})
assert(result.exit_code == 0, "External scenario failed")
return sacho.json_encode(sacho.json_decode(result.stdout))
```

`exec` takes literal argv, without shell expansion; stdout/stderr capture is bounded
to 64 KiB per stream. Timeout/cancellation kills the scenario process group. HTTP
bodies are bounded to 64 KiB, redirects are not followed, and TLS is verified.
Do not return private context or credential-bearing stderr as public display.
Lua has JSON/HTTP/env/exec but no io/os/package/debug or dynamic modules. Provider
logic belongs in external scripts/scenarios, never in the Sacho binary.

Export the embedded Lua examples with `sacho example prometheus`,
`sacho example postgresql`, or `sacho example external-command`; no checkout is needed.

Prometheus: use `sacho example prometheus` with injected PROMETHEUS_URL and
PROMETHEUS_QUERY. It calls the instant query API, accepts a scalar or exactly one
float vector sample, and rejects empty/multiple/nonfinite results. Select or
aggregate deliberately; `sum(rate(http_requests_total[5m]))` produces a TPS-style
rate, while `up{job="example",instance="example:9100"}` can gate health with
`/value gte 1`. Query `time()-timestamp(metric)` separately to gate source age;
the query evaluation timestamp is not proof of fresh source scraping. This is
periodic sampling, not importing Prometheus historical data or remote-write storage.

PostgreSQL: install `psql` on the service host, then use `sacho example postgresql`.
Inject PGHOST, PGPORT, PGDATABASE, PGUSER, PGPASSFILE, PGCONNECT_TIMEOUT and
PGSQL_QUERY. Set PGSQL_QUERY to a SELECT returning one JSON object with a scalar `value` field, e.g.
`SELECT json_build_object('value',1,'version',version())`. Use a read-only DB role;
the example wraps execution in a read-only transaction with a 5s statement timeout,
no password prompt and no psqlrc. Keep password files private, or use an encrypted
vault PGPASSWORD entry when needed. Only expose selected aggregates, not raw rows.
Other SQL engines can use the same external command contract with their own client.
Neither a PostgreSQL driver nor its client is embedded in Sacho.

## Email and webhook templates

In Admin → Notifications, edit templates and preview before saving. Templates use
MiniJinja: `{{ status_name }}` inserts a value; `{% if recovered %}` is control flow.
Email is plain text. Webhook templates must produce a JSON object; use `tojson` to
escape inserted strings. Empty webhook_body preserves the default event payload.

```jinja
{{ site_title }} · {{ service_name }} · {{ status_name }}
{% if recovered %}Recovered{% else %}Incident detected{% endif %}: {{ service_name }}
{% if display_value %}{{ display_label }}: {{ display_value }}{{ metric_unit }}{% endif %}
{{ time }} · {{ public_url }}
```

Webhook body example:

```jinja
{"text": {{ (service_name ~ ": " ~ status_name) | tojson }}, "event": {{ event | tojson }}}
```

GET `/api/admin/templates` returns templates/defaults/variables. PUT the full
`templates` object to the same endpoint to save. Its fields are subject, body,
verification_subject, verification_body and optional webhook_body (null = default).
POST `/api/admin/templates/preview` with `{templates: {...}, event: "incident.started"}`
renders fixtures without sending or saving. Also preview incident.resolved and
subscription.verify. Unknown variables, invalid JSON, multiline subjects and
excessive source/output/fuel are rejected. Confirmation/unsubscribe links are
appended if custom bodies omit them. Changes apply to pending delivery attempts.

Available variables: site_title, public_url, service_name, description, group,
status, status_name, event, event_name, recovered, incident_id, item_id, delivery_id,
at (Unix seconds), time (UTC ISO), display_value, display_label, metric_unit,
latency_ms, confirm_url, unsubscribe_url. Events are incident.started,
incident.resolved and subscription.verify. Missing optional display data is empty;
older queued events may have empty metadata. No credential values, environment
lookup, raw probe output or filesystem/network loader are available to templates.
Preview uses fictional service data and links, never an actual subscriber token.

## Verify, refresh, recover

Check `/healthz`, public `/api/status`, saved-check logs, and expected scalar display.
Check public history after multiple observations; gray means no data. The chart
uses supported gaps for the selected period; a 1h view cannot use a daily gap.

The public browser refetches status, incidents and charts about every 30s, or every
5s while awaiting data. Hidden tabs and open dialogs pause periodic status fetching;
returning to the tab or regaining connectivity triggers immediate refresh when no
dialog is open. It retains the selected period/gap. Scheduling uses a monotonic
browser clock; failed refresh shows a stale-data notice and retries. GET requests
have a 15s timeout. This is separate from each monitor's polling interval. Account
subscription state refreshes every 30s and on tab/network restoration.

For SMTP use a designated test mailbox/relay; subscription confirmation must work,
then verify one incident-start and one recovery delivery using an authorized test
monitor. Draft tests cannot validate durable notification delivery. Do not trigger
incidents in live monitors solely to test email. Diagnose delivery logs privately.

Typical failures: 401 = identity/proxy; 403 = mutation header/Origin; 400 = invalid
fields/configuration; 429 = too many concurrent draft tests. Missing environment
names need both injection and probe_env; SQL needs the client and credentials in
the service environment, not merely in your interactive shell. After an external
config change, run doctor in the service environment and restart. Preserve a backup
for rollback. Automatic updates are optional signed releases; configure enabled
only when authorized, and use serve so its supervisor can restart/rollback safely.

Completion report: public/admin access, monitor IDs and fresh observations,
display/health semantics, variable names (no values), SMTP/update enablement,
external dependencies, local validation and unresolved operator inputs.

References: [Prometheus query API](https://prometheus.io/docs/prometheus/latest/querying/api/),
[psql](https://www.postgresql.org/docs/current/app-psql.html),
[libpq environment](https://www.postgresql.org/docs/current/libpq-envars.html),
[RustCrypto encryption](https://docs.rs/chacha20poly1305/0.10.1/chacha20poly1305/).

Development storage tools: `sacho help storage` embeds the complete backup,
restore and SQLite/PostgreSQL transfer guide. The CLI also provides `sacho database
export --database ./data.db --output ./backup.json`, `database verify --input
./backup.json`, and `database import --input ./backup.json --database ./restored.db`.
Snapshots are private plaintext (including subscriber tokens), created mode 0600,
never overwritten, and must stay outside Git/public paths. `import-postgres` and
`export-postgres` read SACHO_DATABASE_URL privately from the environment. Serving
still uses SQLite: do not switch a live config to PostgreSQL or enable distributed
workers until runtime repository and worker qualification are completed.

Product boundary: implement monitoring, current values/history and reliable
notifications only. External webhook consumers own incident-management workflows,
on-call and remediation. Existing incident.* events and incident_id are compatible
outage/recovery signals, not a request to implement incident management. Do not
expand into general enterprise workflows without an explicit operator request.

Remote monitoring uses one SQLite controller and outbound `sacho worker` processes.
`sacho help workers` embeds the complete guide and `sacho example remote-worker`
prints its configuration. Register per-node token references in remote_workers;
assign a monitor's worker ID in the editor. Scripts require opt-in on both ends;
inputs stay local to workers. Remote draft tests are unavailable, and saved checks
never silently execute locally. Multi-controller HA remains deferred.

Prefer bootstrap enrollment: issue a node-bound one-use key in Admin → Agents,
then feed it to worker-join via stdin. The worker saves its own credential encrypted;
the controller stores hashes only. Lost replies recover with that credential, not
bootstrap replay. Select/revoke agents in admin. The complete procedure is embedded
in `sacho help workers`. Keep the legacy static registration path compatible.

Docker monitoring: `sacho example docker` prints a built-in Lua scenario using
GET requests over a local Unix socket. Inject `DOCKER_SOCKET` and
`DOCKER_CONTAINER` on the selected execution node, allow them in `probe_env`,
set the health condition `/healthy gte 1`, publish `/display`, and optionally
select `/message` as `public_message_pointer`. No Docker CLI is needed. Restrict
socket access to trusted scripts; the socket itself grants host control even
when mounted read-only. Published scalars survive failed queries as timestamped
last-known values; missing values show unavailable. Only explicitly selected
public messages are exposed, never raw errors or container environment values.
Quota or collector checks can set `failure_status="degraded"` to signal warning
instead of an availability outage on condition failures, errors and timeouts.
The default remains `outage`; use updated workers for these optional fields.
