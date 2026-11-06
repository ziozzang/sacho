# Portable persistence and distributed foundations

Sacho v0.1.4 provides a native PostgreSQL migration adapter and a
versioned portable snapshot format. **Serving the page and scheduling probes still
use SQLite.** Do not put a PostgreSQL URL in the existing `database` setting or
switch a production deployment yet. SQL source probes are a separate feature.

## SQLite backup and restore

```sh
sacho database export --database ./data.db --output ./backup.json
sacho database verify --input ./backup.json
sacho database import --input ./backup.json --database ./restored.db
```

Export uses a consistent read transaction, including committed WAL data, and can
run while the server is open. It does not create a missing source database. Import
creates a new database, checks integrity and its canonical SHA-256 fingerprint,
then publishes the file atomically. An existing destination is never overwritten.
Unsupported versions, extra tables/columns, invalid references and fingerprints
fail instead of silently discarding data. Rows preserve IDs, timestamps, nullable
fields, check revisions, subscriber confirmation tokens and delivery retry state.

Snapshot and restored files are mode 0600 on Unix. **Snapshots contain private
subscriber tokens, email addresses, check scripts and logs. They are plaintext,
not an encrypted vault.** Keep them outside Git and public directories, and protect
backup storage appropriately. The fingerprint detects corruption; it is not a
signature authenticating the source. Maximum snapshot file size is 512 MiB.

Restore/cutover requires stopping the old server and notification workers. Keep
the original database until validation succeeds. Running both copies can repeat
notifications. Pending jobs should remain pending; do not clear them to migrate.

## PostgreSQL transfer

Supply `SACHO_DATABASE_URL` through the operator's private environment. Never put
passwords in command arguments, copied examples, shell history or repository
configuration. Use the driver's URL TLS parameters as appropriate for the server;
for example `sslmode=verify-full` for a certificate-validated remote connection.
The adapter is embedded in the binary and does not require `psql`.

```sh
sacho database import-postgres --input ./backup.json
sacho database export-postgres --output ./postgres-backup.json
sacho database verify --input ./postgres-backup.json
sacho database import --input ./postgres-backup.json --database ./returned.db
```

`--url-env NAME` selects another injected variable. Import requires a database
without a `sacho` namespace, leaves unrelated namespaces alone, and creates the
schema/data inside one transaction under a migration advisory lock. Failure rolls
back both DDL and data. The restored fingerprint must match before commit. Identity
sequences resume after imported IDs. Export uses a repeatable-read transaction,
byte-wise primary-key ordering and the same representation as SQLite.

These commands verify portable storage. They do not implement the PostgreSQL
web/API backend, active-active serving or automated operational cutover.

## Distributed ownership foundation

The native PostgreSQL lease repository is implemented and tested, but is not
activated by `serve` and has no worker-facing API yet. It supports atomic claim,
renewal, expiry/reclaim, release without resetting the fencing counter, a generation
identifier preventing replay after namespace replacement, and a fenced transaction
that holds ownership while committing domain changes. Lease time is database time.

Coordination rows live in `sacho_coordination`, separate from business snapshots.
They are transient ownership, not pending jobs or observations. A database cutover
must stop all old nodes; a fresh coordination namespace gives new generations.
The single SQLite controller now implements remote assignment persistence, node
admission, result idempotency and the outbound worker runtime. PostgreSQL runtime
and distributed outbox claims remain pending for multi-controller HA. See the
[role and Docker acceptance contract](architecture/DISTRIBUTED.md).

## Local qualification

```sh
cargo test --locked
python3 tests/storage_docker.py
```

The Docker test owns a disposable PostgreSQL 17 container, temporary memory-backed
data and a random loopback port. It verifies SQLite/PostgreSQL/SQLite roundtrips,
transactional failure, fingerprints, sequence continuation, concurrent claim,
failover and stale-token rejection. It removes its container on completion or
failure and uses no production credentials or GitHub Actions. This is storage and
coordination qualification; full distributed-process fault testing is still pending.

Snapshot catalog v2 includes the bounded remote assignment/receipt table. Legacy
v1 imports normalize to v2 with an empty remote table; their normalized fingerprint
changes with the catalog version. See [remote workers](REMOTE-WORKERS.md).
