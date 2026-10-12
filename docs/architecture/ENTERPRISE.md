# Production-grade monitoring and status platform

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

## Product scope: monitoring only

The governing principle is monitoring, status presentation and reliable outbound
notifications. Commercial feature research is a reference, not authorization to
expand into an incident-management or general enterprise workflow product.
Future expansion requires an explicit user request.

Sacho detects and records health changes, displays current values and history,
and emits durable webhook/email notifications. External webhook receivers own
incident creation, investigation, acknowledgement, ownership, escalation, on-call,
remediation and retrospectives. Do not introduce those workflows, screens or
state machines into Sacho.

Existing automatic outage/recovery records remain monitoring history. The legacy
`incidents` table, `/api/incidents`, `incident.started` / `incident.resolved` event
names and `incident_id` correlation field remain compatible. They do not represent
an operator-managed incident lifecycle. Renaming them must be versioned and must
not break deployed receivers, templates or historical snapshots.

## Monitoring domains and current gaps

| Domain | Capability | Current state |
| --- | --- | --- |
| Persistence | SQLite/PostgreSQL runtime repositories, verified transfer, backup and restore | SQLite runtime and native SQLite/PostgreSQL snapshot transfer implemented; PostgreSQL runtime pending |
| Monitors | Groups and explicit remote execution locations | Checks/groups implemented; remote worker selection implemented; dependency/consensus engines deferred |
| History | Automatic outage/recovery intervals, scalar values, latency and data freshness | Automatic records and timeline implemented; freshness and diagnostics to improve |
| Check scheduling | Intervals, deadlines, hysteresis and optional scheduled alert muting | Core scheduling implemented; muting windows pending; no editorial maintenance workflow |
| Subscriptions | Verified email and monitor/channel preferences | Verified all-monitor email implemented; preferences pending |
| Delivery | Generic Jinja email/JSON webhooks, signatures, retries, replay and failure visibility | Templates, email, JSON hooks and retries implemented; signatures/replay to improve |
| Access | Existing auth proxy, protected monitor settings, scoped worker/API credentials | Proxy allowlist implemented; worker/API admission pending; no team/IAM platform |
| Configuration | Validation, preview, change history and rollback for monitoring settings | Draft execution and validation implemented; durable change history pending; no approval workflow |
| Metrics | Availability, latency percentiles, source freshness, retention/downsampling and exports | Observed uptime/mean latency and retention implemented; richer metrics pending |
| Probes | HTTP/TLS/DNS/TCP/push/heartbeat, Lua/Python/shell, Prometheus/SQL sources and multi-location checks | HTTP/external scenarios and source examples implemented; additional probes pending |
| Operations | Backup/restore, readiness, signed update rollback, diagnostics and capacity controls | Snapshot transfer, readiness/update rollback and encrypted inputs implemented; operational metrics pending |
| Distribution | Remote network locations, leased assignments, revision/run fencing and durable result acceptance | Single-controller SQLite remote workers and Docker fault tests implemented; PostgreSQL HA and independent delivery claims deferred |
| Experience | Accessible English/Korean status UI, layout editor, live refresh and embedded LLM setup guides | Core UI, editor, refresh and guides implemented |

Incident workflows, on-call/escalation, ticketing, remediation, customer-specific
business portals, general multi-tenant SaaS, billing and compliance certification
are outside the current scope. Public/private presentation may be revisited only
when required for a specific monitoring use case; no audience platform is planned.

## Architecture decisions

1. Domain objects are monitors, probe runs, observations, automatic outage/recovery
   intervals, subscriptions, delivery jobs, nodes and monitoring configuration.
   Operator-managed incident state belongs to external webhook consumers.
2. Repository interfaces own SQL, migration versions, transactions and locking.
   HTTP handlers and schedulers do not contain backend-specific SQL. SQLite is
   the zero-configuration default; PostgreSQL is an explicit external backend.
3. A measured health transition, its history and notification intent commit in one
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
   One controller and outbound-only subordinate workers use the same executable;
   multiple control replicas are deferred until a concrete need exists.
   see [distributed execution and Docker qualification](DISTRIBUTED.md) for role,
   protocol, security, failover and container acceptance contracts.
6. Authentication uses the established identity proxy. Authorization protects
   monitoring settings and node/API access; it does not introduce team management. Secret references resolve
   from encrypted external inputs; plaintext secrets never enter exported settings,
   public data, templates or audit records.
7. External scenarios own product-specific integrations. Native reusable protocol
   primitives and templates are permitted; vendor-specific credential/usage code
   stays outside the engine.
8. Public states must distinguish missing/stale observations, muted checks and
   measured failures. Availability claims disclose sampling and exclusions.
9. Features ship with English/Korean setup instructions, API contracts and actual
   process/browser qualification. A UI control without durable behavior is not done.

## Implementation and qualification sequence

Polish single-controller remote checks and recovery → probe/source freshness →
monitor-level subscriptions and reliable webhook delivery → monitoring configuration
recovery and capacity/latency qualification → optional PostgreSQL runtime repository.
Multi-controller HA is deferred. No incident-management
or general enterprise workflow phase is included.

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

## Simplicity and product quality

Default to one executable, one controller, embedded SQLite and optional outbound
workers. Add dependencies and services only for a demonstrated monitoring need.
Keep native integrations generic; specific providers remain external Lua/scripts.
Do not build component dependency graphs, regional consensus policy engines, full
SLO/error-budget suites or general configuration approval systems now. Flat groups,
explicit worker IDs, honest freshness, useful histories and clear delivery state
are preferred.

Quality gates are understandable setup and errors, smooth accessible desktop/mobile
UI, preserved editor drafts, explicit local/remote execution, bounded resource use,
accurate missing-data presentation, stable notification identity, verified restore
and real process failure tests. New features must improve an actual monitoring task
without burdening the default installation.

Remote-worker increment: SQLite assignments/receipts and observation/outbox updates
commit atomically; saved remote checks never execute locally. Scripts require
controller and worker opt-in. The editor shows execution location and prevents
misleading local draft execution. Snapshot catalog v2 preserves the remote table;
v1 imports are normalized without losing base rows. New delivery IDs are persisted
UUIDs so retention cannot reuse their identity. Actual Docker control/worker tests
qualify location-specific Lua inputs, scoped authentication, duplicate/stale results,
paused-worker takeover, network reconnect and controller restart. Multi-controller
HA and supervised worker updates remain deferred.
