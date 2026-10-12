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

Admin → Check examples offers HTTP, JSON/TPS, Prometheus, PostgreSQL and Docker.
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
