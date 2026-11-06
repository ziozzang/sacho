# Offline configuration reference

Copyright Jioh L. Jung <jung@jioh.net>.

Run `sacho agent-guide` for installation steps and API recipes, `sacho help --json`
for schema version 1 command/configuration metadata. Both work offline, without a
configuration or database. `sacho config check sacho.toml` validates server TOML;
add `--worker` for worker TOML. Exit 0 means success; nonzero means fix the reported
problem. Parsing errors report line/column without echoing configuration contents.
Validation reads referenced vaults/credentials but never opens the database.
Run under the service account with its actual environment and working directory.
All relative paths resolve from the working directory, not the TOML directory.

Settings have no implicit environment overrides. Only the explicitly named input
references below read the environment (then the vault); they are not literal values.
Optional tables are absent until supplied, and required members then become mandatory.
Unknown keys are rejected. Runtime-only Rust fields are not configuration keys.

| Scope | Key | Type | Default | Environment | Description |
| --- | --- | --- | --- | --- | --- |
| server | `bind` | string | "127.0.0.1:8080" | — | IP address and port; no-auth requires loopback. |
| server | `database` | string | "sacho.db" | — | SQLite file, relative to process working directory. |
| server | `public_url` | string | "http://127.0.0.1:8080" | — | HTTP(S) origin for links and request origin checks. |
| server | `public_origins` | array of strings | [] | — | Additional normalized HTTP(S) origins; at most 16. |
| server | `title` | string | "Sacho" | — | Initial page title; editable through admin settings. |
| server | `description` | string | "Service health, recorded with care." | — | Initial page description. |
| server | `auth` | table | default table | — | Authentication settings below. |
| server | `auth.mode` | string | "no-auth" | — | no-auth or auth-proxy; public access needs a trusted proxy. |
| server | `auth.email_header` | string | "X-Forwarded-Email" | — | Verified email header sanitized by the proxy. |
| server | `auth.admins` | array of strings | [] | — | Exact administrator emails; required in auth-proxy mode. |
| server | `auth.logout_url` | optional string | absent | — | Local absolute path or HTTPS logout URL; absent hides link. |
| server | `metrics` | table | default table | — | Prometheus exposition and direct Grafana data source settings. |
| server | `metrics.enabled` | boolean | true | — | Enable metrics endpoints by default; false returns 404. |
| server | `metrics.anonymous_enabled` | boolean | true | — | Allow anonymous reads; false requires token or trusted proxy identity. |
| server | `metrics.token_env` | optional string | absent | configured name (e.g. SACHO_METRICS_TOKEN) | Metrics token reference, 32..512 printable bytes; never expose to probes. Restart after rotation. |
| server | `metrics.token_scope` | string | "authenticated" | — | authenticated excludes group-restricted checks; explicitly choose all to include them. |
| server | `metrics.max_items` | integer | 1000 | — | Exposed items in ID order, 1..100000; excess visible items counted. |
| server | `metrics.max_query_series` | integer | 10000 | — | Series per query, 1..1000000; also capped by byte budget/512. |
| server | `metrics.max_query_samples` | integer | 2000000 | — | Stored observations, derived samples and generated points, 1000..500000000; also capped by byte budget/128. |
| server | `metrics.max_query_bytes` | integer | 67108864 | — | Retained timeline/series budget in bytes, 1024..1073741824; not total process RSS. |
| server | `metrics.max_query_range_seconds` | integer | 2678400 | — | History range limit in seconds, 1..31622400; default 31 days. |
| server | `metrics.query_timeout_seconds` | integer | 30 | — | SQL/evaluation deadline in seconds, 1..600. |
| server | `metrics.label_value_max_chars` | integer | 128 | — | Maximum label characters, 16..1024. |
| server | `smtp` | optional table | absent | — | Email disabled when absent. |
| server | `smtp.host` | string | required when smtp present | — | SMTP server hostname. |
| server | `smtp.port` | integer | 587 | — | SMTP port; commonly 587 STARTTLS or 465 TLS. |
| server | `smtp.from` | string | required when smtp present | — | Sender mailbox, e.g. Status <status@example.com>. |
| server | `smtp.username` | optional string | absent | — | SMTP account name. |
| server | `smtp.password_env` | optional string | absent | configured name (e.g. SACHO_SMTP_PASSWORD) | Password lookup name; environment overrides encrypted vault. |
| server | `smtp.security` | string | "starttls" | — | starttls, tls or plain; plain only on loopback/private IPv4. |
| server | `updates` | table | default table | — | Signed binary update settings. |
| server | `updates.enabled` | boolean | false | — | Enable automatic signed updates; requires writable binary directory. |
| server | `updates.interval_seconds` | integer | 21600 | — | Update polling interval in seconds, at least 60. |
| server | `updates.public_key` | optional string | absent (embedded release key) | — | Base64 Ed25519 verification key override. |
| server | `updates.manifest_url` | optional string | absent (GitHub release) | — | Custom signed manifest HTTPS URL. |
| server | `updates.ca_file` | optional string | absent | — | PEM certificate authority file for custom update host. |
| server | `retention_days` | integer | 100 | — | Observation retention in days, 90..3650. |
| server | `max_concurrent_checks` | integer | 16 | — | Concurrent monitor limit, 1..128. |
| server | `probe_env` | array of strings | [] | listed names | Allowlisted probe inputs; at most 32. Never include admission tokens. |
| server | `remote_workers` | array of tables | [] | — | Legacy static workers, at most 128; prefer enrollment. |
| server | `remote_workers[].id` | string | required per entry | — | Unique worker ID, ASCII letters/digits/underscore/hyphen, 1..64 bytes. |
| server | `remote_workers[].token_env` | string | required per entry | configured name | Unique admission token reference, 32..512 printable bytes. |
| server | `remote_workers[].allow_scripts` | boolean | false | — | Controller permits script execution; worker must also opt in. |
| server | `secrets` | optional table | absent | — | Encrypted input vault; both files must remain private. |
| server | `secrets.vault` | string | required when secrets present | — | Encrypted JSON vault path. |
| server | `secrets.key_file` | string | required when secrets present | — | Private random unlock key path. |
| server | `llm` | optional table | absent | — | OpenAI-compatible configuration assistant; absent hides assistant. |
| server | `llm.base_url` | string | required | — | API base ending in /v1; HTTPS, or loopback HTTP for local models. |
| server | `llm.model` | string | required | — | Provider model identifier. |
| server | `llm.api_key_env` | optional string | absent | configured name | API key environment/vault reference; choose exactly one key reference. |
| server | `llm.api_key_file` | optional string | absent | — | Private UTF-8 key file reference; never enter a literal key. |
| worker | `control_url` | string | required | — | Controller HTTP(S) origin, no credentials or path; HTTPS for remote hosts. |
| worker | `node_id` | string | required | — | Worker ID matching enrollment or controller entry. |
| worker | `token_env` | string | required | configured name (e.g. SACHO_WORKER_TOKEN) | Worker token lookup; never expose to probes. |
| worker | `controller_id` | optional string | absent | — | Pinned controller UUID; worker-join sets this. |
| worker | `label` | string | "" | — | Enrollment display label. |
| worker | `probe_env` | array of strings | [] | listed names | Worker-local input allowlist, same restrictions as server. |
| worker | `secrets` | optional table | absent | — | Worker-local encrypted input vault. |
| worker | `secrets.vault` | string | required when secrets present | — | Worker-local encrypted vault path. |
| worker | `secrets.key_file` | string | required when secrets present | — | Worker-local private unlock key path. |
| worker | `ca_file` | optional string | absent | — | PEM controller certificate authority file. |
| worker | `allow_scripts` | boolean | false | — | Allow scripts only for trusted controller operators. |
| worker | `allow_insecure_http` | boolean | false | — | Permit HTTP to a non-loopback controller; unsafe outside a protected network. |

Other environment variables: `RUST_LOG` controls logging (default `sacho=info`),
`SACHO_DATABASE_URL` supplies PostgreSQL transfer commands unless `--url-env` names
another variable, and `SACHO_RELEASE_SIGNING_KEY` supplies the release signer only.
No signing key is needed for installing or serving. `SACHO_PROBE_TIMEOUT` and
`SACHO_PROBE_ALLOWED_ENV` are internal subprocess controls, not operator settings.

## Environment checks and recovery

`sacho doctor --config sacho.toml` checks configuration, database directory write
access, SQLite integrity and monitor specifications. As before, it may create or
migrate the database. Back up before running against an existing installation.
It checks that the listener is free, or that the existing listener answers as Sacho
on `/healthz`. It checks an optional SMTP TCP connection (no email is sent), CA file
readability and automatic-update directory write access. TCP success does not prove
SMTP authentication/TLS or delivery. Use a designated test mailbox to confirm mail.
`sacho doctor --config worker.toml --worker` checks local worker credentials and a
bounded controller `/healthz` request, without claiming work or consuming enrollment.
Health success does not prove worker admission; verify the agent in Admin → Agents.

| Problem | Resolution |
| --- | --- |
| Invalid TOML at line/column | Correct syntax or unknown/mistyped key using the table above; never paste secrets into reports. |
| Missing named input | Inject the configured name into the service environment or encrypted vault; report names only. |
| no-auth must bind to loopback | Bind to 127.0.0.1, or configure auth-proxy and admin emails behind a private upstream. |
| Listener unavailable | Stop an unintended conflicting service or select a free port; check health of an intended running Sacho. |
| Database/path not writable | Create parent directories and set ownership for the service user. |
| SMTP/controller connection failed | Check hostname, port, routing, firewall, TLS trust and whether the destination is running. |
| Invalid worker credential | Supply a unique 32..512-byte printable token, or complete worker-join with a fresh invitation. |
| 401 / 403 from admin API | Check proxy identity, admin email, X-Sacho-Request and Origin. |

Keep tokens and passwords in private files/environment or the encrypted vault, never
command arguments or Git. Proxy identity headers must be sanitized and the upstream
must remain private. The diagnostics never print credential values or URL errors.

## Common configurations

Use `sacho init --config sacho.toml` for loopback defaults, then `config check`,
`doctor`, and `serve --config sacho.toml`. For a remote collector, export
`sacho template remote-worker`, follow `sacho help workers` for enrollment, then
validate and run `sacho worker --config worker.toml`. HTTP, DNS, TCP, ping, SNMP,
shell, Python and Lua checks use the existing monitor API; see `sacho help monitoring`.

Prometheus queries are supported through native checks and `sacho template prometheus`;
allowlist PROMETHEUS_URL and PROMETHEUS_QUERY for the Lua example. For exporting Sacho itself,
point a Grafana Prometheus data source at the Sacho origin or scrape `/metrics`. Read
`sacho help metrics` for token headers, publication rules and query limits. Anonymous
queries have separate slots and lower limits. Disabling trend publication exposes only
the latest stored observation, even to tokens and administrators.
Page title/description and layout use admin settings/layout APIs. This branch has
no configurable theme key; do not write theme settings into TOML. Notifications,
visibility and schedules are explained in the embedded monitoring guide.

## Per-subscription notification rules (database settings, not TOML)

Email and webhook selection rules use the [subscription key table](SUBSCRIPTIONS.md#rule-keys-and-api).
Discover them offline with `sacho subscriptions schema` or the `subscription_rules` field in `sacho help --json`.
`agent-guide` includes that table. Empty rules preserve all notifications; disabled rules remain muted.
