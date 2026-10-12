# Enterprise status and incident platform

Status: implementation in progress. This is a product architecture and qualification
record, not a claim that planned features already exist. Runtime configuration,
accounts, deployment addresses, local TODOs and credentials remain outside Git.
Build and qualify locally; GitHub Actions stays opt-in.

## Evidence and product boundary

The public [OpenAI status page](https://status.openai.com/) links its powered-by
provider to incident.io. Its observable priorities include product/component groups,
incident progress and history, subscription, and honest aggregate availability.
Do not infer OpenAI's unpublished internal incident stack from the public page.

Primary feature references:
[incident.io Status Pages](https://incident.io/status-pages),
[Atlassian Statuspage](https://www.atlassian.com/software/statuspage/features/core),
[Statuspage enterprise features](https://www.atlassian.com/software/statuspage),
[Better Stack status pages](https://betterstack.com/status-page).
Implement the capabilities independently; do not copy proprietary code or assets.

## Required domains and current gaps

| Domain | Capability | Current state |
| --- | --- | --- |
| Persistence | SQLite/PostgreSQL repositories, versioned migrations, export/import, verification and rollback | SQLite runtime and SQLite/PostgreSQL verified snapshot transfer implemented; PostgreSQL runtime repository pending |
| Components | Groups, dependencies, regions/products, impact aggregation and manual overrides | Flat checks/groups implemented; richer component model planned |
| Incidents | investigating/identified/monitoring/resolved, affected components, updates, severity, owner, permalink and retrospective | Automatic start/recovery implemented; editorial workflow planned |
| Maintenance | One-off/recurring schedules, time zones, component scope, reminders and notification suppression | Planned |
| Subscribers | Component/channel preferences, verified email, self-service management, locale and consent | Verified email/all-component notifications implemented; preferences planned |
| Communication | Jinja templates, email/webhooks, signed deliveries, retries, replay, dead-letter queue, RSS/Atom, Slack/Teams presets | Templates/email/JSON hooks/retries implemented; signatures/feed/channel presets planned |
| Audiences | Public/private/internal pages, scoped visibility, branded custom domains and restricted metrics | One public page/auth-proxy admin implemented; audience model planned |
| Enterprise access | Teams, SSO/proxy contracts, roles/scopes, API credentials, least privilege and revocation | Email allowlist/proxy implemented; roles/scopes planned |
| Governance | Durable attributed audit, configuration versions/diffs, approvals, export/import and reversible publishing | Planned |
| Reliability | SLI/SLO/error budgets, percentiles, maintenance exclusions, retention/downsampling and exports | Observed uptime/mean latency and retention implemented; SLO model planned |
| Probes | HTTP/TLS/DNS/TCP/push/heartbeat, external Lua/Python/shell, Prometheus/SQL sources, retries and multi-location consensus | HTTP/external scenarios and source examples implemented; remaining protocols/consensus planned |
| Response | Acknowledgement, routing, escalation, on-call calendars and integrations | Planned |
| Operations | Backup/restore, backend migration, readiness, signed update rollback, diagnostics/Prometheus metrics and capacity controls | Readiness/update rollback/input encryption implemented; migration/operational metrics planned |
| Availability | Single-node default; leases and fenced ownership for multi-replica workers, claimed outbox and durable deduplication | Single-node supervisor and native PostgreSQL lease/fencing primitives implemented; worker runtime and outbox claims pending |
| Experience | Accessible responsive UI, localization, layouts, incident search, live data, embedded LLM runbooks and schemas | Core UI/layouts/live data/runbook implemented; search/richer schemas planned |

## Architecture decisions

1. Domain objects are components, probes, observations, incidents/updates,
   maintenance windows, pages/audiences, subscriptions, delivery jobs, actors and
   audit events. Probe health and editorial incident lifecycle are separate.
2. Repository interfaces own SQL, migration versions, transactions and locking.
   HTTP handlers and schedulers do not contain backend-specific SQL. SQLite is
   the zero-configuration default; PostgreSQL is an explicit external backend.
3. An incident transition, its audit record and notification intent commit in one
   transaction. Delivery runs after commit. Stable delivery IDs survive pruning,
   retries, backend migration and crash recovery; transport adapters stay generic.
4. Migration uses a versioned portable representation preserving identity and
   references. Export a consistent source, load an empty target transactionally,
   compare counts and canonical fingerprints, and switch configuration only after
   verification. Keep the source for rollback. Avoid active writers/deliverers
   during cutover; migration must not create duplicate alerts.
5. The single-node process lock remains the default ownership model. PostgreSQL
   availability does not imply active-active readiness. Multi-replica execution
   requires leases, fencing and atomic job claims before claiming HA support.
   Control replicas and outbound-only subordinate workers use the same executable;
   see [distributed execution and Docker qualification](DISTRIBUTED.md) for role,
   protocol, security, failover and container acceptance contracts.
6. Authentication uses the established identity proxy; scoped authorization and
   durable actor attribution belong in the application. Secret references resolve
   from encrypted external inputs; plaintext secrets never enter exported settings,
   public data, templates or audit records.
7. External scenarios own product-specific integrations. Native reusable protocol
   primitives and templates are permitted; vendor-specific credential/usage code
   stays outside the engine.
8. Public states must distinguish missing/stale observations, scheduled work,
   overrides and measured failures. SLO claims disclose sampling and exclusions.
9. Features ship with English/Korean setup instructions, API contracts and actual
   process/browser qualification. A UI control without durable behavior is not done.

## Implementation and qualification sequence

Persistence/migration → domain lifecycle and maintenance → subscriber/audience
scopes → durable authorization/audit → delivery security/channel adapters → SLOs
and probe families → escalation/on-call → fenced multi-replica operations.

Each phase records source commit, schema compatibility, tested scenarios and
remaining limitations. Validate SQLite and an owned PostgreSQL test database;
never experiment with a production database or publish operational test fixtures.
Only switch live persistence after the complete engine/API flow and recovery
qualification pass. Existing deployments remain usable during development.

## Qualified foundation increment

Implemented in the development source:

- Portable schema-v1 snapshots covering all seven business tables; consistent
  SQLite/PG export, fresh-target transactional restore, canonical fingerprints,
  catalog checks and private atomic file publication.
- Native async PostgreSQL driver with TLS support, isolated application namespace,
  migration advisory locking, repeatable-read export and identity continuation.
- PostgreSQL ownership primitives: concurrent claim, authoritative expiry,
  renewal/release, monotonic fencing, generation identity and fenced transactions.
  Renewal rechecks expiry after waiting for a row lock.
- Embedded English/Korean storage runbooks and actual CLI qualification.

Local qualification passed the Rust suite, strict Clippy, CLI and existing probe/
SMTP/webhook smoke tests, plus the owned PostgreSQL Docker roundtrip, failure and
contention matrix. The musl single-executable build is retained. No Actions run,
production database migration or distributed runtime activation is part of this
increment. Pending work: replace serving repositories, wire durable runs/node
admission/worker protocol/outbox claims, then perform full process fault testing.
