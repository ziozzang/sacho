# Distributed execution and Docker qualification

Status: one SQLite-backed controller with outbound remote workers is implemented
and qualified using actual Docker processes. PostgreSQL lease/fencing primitives
are separately qualified. Multi-controller HA is deferred, not enabled.

Distributed roles execute monitoring checks and deliver notifications only.
They do not implement incident management, on-call, ticketing or remediation;
webhook consumers own any such workflows.

## Current roles and storage

- `sacho serve`: embedded SQLite, web/API, local scheduler and deliveries in one
  binary. Registered remote workers claim their own monitors through bearer-auth
  API routes. The controller retains the existing single-instance process lock.
- `sacho worker --config worker.toml`: outbound-only connection to the controller;
  no public listener or database credentials. Executes one assigned probe at a
  time using local allowed inputs, then submits a bounded result.

This is the preferred initial distributed topology: one controller, many remote
network locations. No broker, Redis, mandatory PostgreSQL or separate delivery
service is introduced. Lua remains the generic integration boundary.

SQLite files must not be shared across hosts or placed on network storage.
PostgreSQL runtime repositories and multiple control replicas may be added later
when required; neither snapshot support nor lease primitives imply HA readiness.

See [the embedded worker setup guide](../REMOTE-WORKERS.md) for current protocol,
capability admission, security and result semantics. Current SQLite ownership uses
random execution IDs and revision guards, a bounded receipt row per monitor, and
an atomic result/history/outbox commit. Lease replacement creates a new execution
ID, so stale results cannot affect its successor. The following numbered-token,
slot-deduplication and rolling-version rules are reserved for a future shared
PostgreSQL coordinator; they are not claims about the current worker protocol.

## Ownership and result acceptance

Each scheduled run has a stable ID, probe revision, scheduled time, assigned node,
lease expiry and monotonically increasing fencing token. Claiming a run and
incrementing its token are atomic. Lease time comes from the database, rather
than worker clocks. Heartbeats extend only the currently owned token. Expiry
allows another worker to claim the run; deleting lease records must not reset
the token sequence.

Result acceptance verifies the run ID, owner, token, lease and probe revision
inside the transaction that records the observation, changes health and queues
notifications. A unique run-result constraint makes retries idempotent. A late
result from a paused/replaced worker cannot overwrite newer state. Administrative
changes invalidate stale probe revisions. Scheduling uses a unique constraint on
probe ID and scheduled slot to prevent duplicate runs across control replicas.

Execution is at least once: lease expiry can cause overlapping external requests.
Fencing guarantees one accepted result, not one external side effect. Probe scripts
must be observational; any side-effecting integration needs its own idempotency
contract. The existing subprocess deadline and process-group cleanup apply to
workers as well.

Delivery claims also carry expiry and fencing tokens. Delivery IDs are globally
stable across pruning and database migration. Webhooks include an idempotency
key and may be retried. SMTP cannot guarantee exactly-once delivery after a crash
between remote acceptance and the local commit; document this limitation rather
than hiding it behind a deduplication claim.

## Secrets, admission and upgrades

Workers receive only probe-specific allowed inputs. Node-local encrypted vaults
and environment references are preferred for credentials. Control responses,
assignment logs and admin node views never expose secret values. Remote executable
probes are privileged: require an explicit execution capability, bounded payloads,
TLS and authenticated control, and a protected worker environment. Do not execute
code received from an unauthenticated endpoint.

The current protocol has an explicit version and script capability opt-in. Unsupported
versions fail admission before receiving work. Shutdown drains assignments; forced
shutdown leaves them recoverable after lease expiry. Future rolling updates must preserve
protocol compatibility. Workers currently use operator-managed binary upgrades;
the automatic supervised signed updater belongs to the controller. Database schema
migrations run once under a separate migration lock before incompatible processes
start. No independent replica may opportunistically rewrite shared schema.

## Current qualification and future HA acceptance matrix

Current qualification is `tests/remote_docker.py`: one actual controller, workers
in two locations and a paused/replacement worker. It covers scoped admission,
local injected inputs, duplicates, takeover, stale results, reconnect and restart.
SQLite/PostgreSQL snapshot and ownership qualification is separate.

For future HA qualification, use isolated, owned containers: one PostgreSQL instance, two controls, three
workers with distinct labels, a recording HTTP probe target, a webhook receiver
and an SMTP test relay. Bind exposed ports to loopback with dynamically selected
host ports. Use disposable test credentials and temporary storage; never mount
operator configuration, home directories or live databases. Build the local
binary into the test image; do not depend on GitHub Actions or production images.

| Scenario | Required assertions |
| --- | --- |
| Normal scheduling | One durable run per probe/slot; each accepted run has one observation |
| Concurrent controls | Leadership changes without duplicate slots or notification intents |
| Worker killed during probe | Lease expires, another worker accepts the result; no permanently stuck run |
| Paused worker resumes | New token wins; stale result and stale heartbeat are rejected |
| Repeated result submission | Same run/result is idempotent; conflicting duplicate is rejected |
| Control or database disconnect | Workers back off; no unbounded queue, retry storm or incorrect healthy state |
| Probe edited/deleted in flight | Old revision cannot update health or recreate deleted state |
| Delivery interrupted | Jobs recover after expiry; stable webhook key survives retries |
| Clock skew | Worker time cannot extend ownership; authoritative lease time remains consistent |
| Node admission and revocation | Invalid, wrong-scope and revoked credentials receive no assignments |
| Regional selection | Labels constrain assignment; missing eligible workers produce stale/unknown data |
| Rolling upgrade | Compatible nodes drain/rejoin; incompatible nodes fail admission |
| SQLite to PostgreSQL migration | IDs, observations, subscriptions and pending deliveries preserved and verified |

Use a controllable short lease or protocol test clock rather than long sleeps.
For network faults, disconnect only the owned test container from its test
network. For pause/resume fencing, use Docker pause/unpause and verify the token
directly through an authenticated test API or the fixture database. Readiness must
exercise host-accessible PostgreSQL TCP, not merely its initialization socket.
Always unpause and remove owned containers, networks and volumes in a finalizer,
including test failure and interruption. Never use global Docker prune commands.

Qualification is complete only when these assertions run against actual control
and worker processes. A Compose file or isolated lease unit test is insufficient
evidence for distributed production support.
