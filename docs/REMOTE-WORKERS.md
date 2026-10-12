# Remote monitoring workers

Use one controller and one or more remote workers to test services from their
actual network locations. The same executable supplies both roles. The controller
can keep using embedded SQLite; remote workers require neither a database nor an
inbound port. Multi-controller HA is deferred. This is monitoring only; incident
management stays in external webhook receivers.

## Controller registration

Add a unique node ID and a private token reference to the controller configuration:

```toml
[[remote_workers]]
id = "edge-west"
token_env = "SACHO_EDGE_WEST_TOKEN"
allow_scripts = false
```

Generate a cryptographically random token of at least 32 bytes and provision the
same value privately on the controller and that worker. Use a different token for
every node. Store it in the existing encrypted vault with `sacho secrets set`, or
inject it through the service environment. Never place the value in TOML, Git,
command arguments or logs. Do not include the admission token in `probe_env`.
Restart the controller after registration, token rotation or revocation. Removing
the node from configuration revokes both assignment and result requests.

## Worker installation

```sh
sacho example remote-worker > worker.toml
sacho worker --config worker.toml
```

Edit `control_url`, `node_id` and `token_env`. The token reference can have a
different name from the controller's reference, but the value must match. Configure
worker-local `[secrets]` and `probe_env` for the actual probe's inputs. A variable
being available on the controller does not make it available remotely.

Use HTTPS to the controller with certificate validation. `ca_file` adds a private
root certificate. Redirects are rejected, including redirects to a login page.
Configure the identity proxy so `/api/workers/*` reaches Sacho directly; Sacho
checks the node's bearer credential. The worker does not use an administrator's
Google session or forwarded identity. Plain HTTP is permitted by default only for
literal loopback/localhost; `allow_insecure_http=true` is an explicit isolated-test
option. The normal example uses HTTPS.

Workers run one assignment at a time, poll every two seconds while idle and back
off up to 30 seconds during controller/network failures. Run them under a service
manager with restart-on-failure. SIGTERM cancels the current probe and cleans up
its subprocess group. Jobs can be reclaimed after lease expiry. The worker role
currently requires operator-managed binary upgrades and restart; automatic
supervised updates remain a controller feature.

## Assigning checks

In the monitor editor, **Execution location** accepts a registered worker ID and
suggests configured IDs. Leave it empty for local execution. The administrator list
shows the execution location. **Run now** makes a saved check due for its worker;
remote checks are never silently run on the controller.

Remote draft execution is not implemented. The draft-test control is disabled for
remote checks and explains that execution starts after saving. Clear the location
only when deliberately testing locally; the resulting network and input values
may differ from those on the remote worker.

HTTP checks work without script capability. Lua, shell and Python require
`allow_scripts=true` on both controller registration and worker configuration.
Lua is embedded; shell/Python need their interpreters and any external clients on
the worker. These are privileged scripts from trusted monitoring administrators,
not an untrusted-code sandbox. Use a dedicated worker user/container and constrain
its filesystem, network and credentials. The environment allowlist limits direct
input injection; it is not full isolation against arbitrary executable code.

## Result and delivery guarantees

Assignments contain protocol version 1, a random run ID, monitor revision, spec
and lease expiry. The controller calculates time; worker clock skew cannot extend
a lease. A run leases the monitor for its probe deadline plus 30 seconds. Each node
has at most one active assignment; scale parallelism with separate node IDs.

Result acceptance verifies node, run ID, revision, enabled state and lease in the
same SQLite transaction as the observation, health transition and notification
intent. Same-result retries for the current run are idempotent; conflicting,
expired, replaced, edited or deleted-monitor results are rejected. Historical run
receipts are bounded to the current run per monitor. A delayed retry after a newer
assignment is rejected without creating another observation.

Execution is at least once. A paused worker may resume after another worker has
reclaimed a run, so scripts must be observational. There is one accepted result
for that run; this does not guarantee only one external request. Captured output
is bounded and may be shortened to fit the result request limit. Inputs and raw
output remain private; configured scalar displays follow normal publication rules.

Lost contact is missing data: the existing stale-data threshold changes public
health to unknown rather than leaving an old successful check permanently green.
The node registry exposes last accepted result, not a claim of live heartbeat.
Webhook delivery IDs are persisted UUIDs for new jobs, survive retention and
snapshot transfer, and are reused on retry. Legacy pending jobs keep their numeric
IDs for compatibility. Receivers should deduplicate `Idempotency-Key`; SMTP can
still repeat after a crash at remote acceptance.

## Storage and qualification

The bounded `remote_runs` table is an additive SQLite extension. Local check specs
omit empty worker fields to preserve the existing format. Snapshot catalog v2
includes assignments and result receipts; catalog v1 imports are normalized by
adding an empty remote table, preserving all original rows. The normalized v2
fingerprint differs from a v1 fingerprint because the catalog version changed.
PostgreSQL transfer includes v2; the controller's live backend is still SQLite.
Stop all controllers/workers before a database cutover or restore.

```sh
cargo test --locked
CC_x86_64_unknown_linux_musl=musl-gcc cargo build --locked --release --target x86_64-unknown-linux-musl
python3 tests/remote_docker.py
python3 tests/storage_docker.py
```

The remote Docker test owns an isolated network, unprivileged static-binary
containers and temporary fixture data. It verifies two network locations, local
injected inputs, credential/protocol rejection, duplicate result handling,
paused-worker takeover/stale rejection, disconnect/reconnect and controller
restart. It unpauses/removes its own containers and network on failure as well.
The fixture shortens a lease through its private test database instead of waiting
through production durations. No operational config, credentials or Actions run
are used. PostgreSQL tests separately qualify portable storage and ownership
primitives; they do not demonstrate multi-controller HA.
