# Installing Sacho as an automated agent

[한국어](AGENT-GUIDE.ko.md) · [Full deployment guide](DEPLOY.md)

This is a runbook for an automated agent, or anyone scripting a setup. Every step has a
command and a pass condition, and needs no web browser. `DEPLOY.md` explains the reasons,
and `sacho help setup` gives more API detail offline.


## Set up using only the binary / 바이너리만으로 설정하기

`sacho agent-guide` prints this runbook, the complete configuration reference and
existing detailed setup recipes offline. Use `--language ko` for the Korean runbook
and setup recipes (the complete key reference uses English). No documentation ZIP
is required for a local installation. Server setup, worker enrollment, Prometheus
queries and API recipes are embedded; `sacho help --json` prints schema version 1
metadata for automation. Configuration keys have no implicit environment overrides.

```sh
sacho --version
sacho help --json
sacho agent-guide --language ko
umask 077
sacho init --config sacho.toml
sacho config check sacho.toml
sacho doctor --config sacho.toml
sacho serve --config sacho.toml
# From another terminal: curl -fsS http://127.0.0.1:8080/healthz
```

`init` refuses to overwrite existing files. `config check` does not open the
database. Run diagnostics with the service account and environment; doctor may
create/migrate SQLite. For remote worker files use `config check worker.toml
--worker` and `doctor --config worker.toml --worker` after enrollment. The latter
contacts controller health without claiming a task. Use `sacho help workers` for
enrollment. Treat missing inputs as unresolved; never fabricate credentials.
The remaining deployment archive steps are optional proxy/service-manager recipes.


## Rules

- **Look before you change.** If Sacho is already installed, read its version, unit,
  configuration and `GET /api/admin/items`. Do not replace a working deployment. Update
  monitors by ID, so their history survives; do not delete and re-create them.
- **Never reveal secrets.** Pass them through standard input or private files, never as
  arguments, in logs, in TOML, in check scripts or in your report. Report names, not values.
- **Do not guess what only the operator knows.** Stop and ask for the values marked
  "ask" below.
- **Never expose Sacho's port.** Anything that reaches it can claim any identity.
- **Do not trigger real incidents to test alerts**, and do not point test mail at real people.

## Step 1: Decide these values

| Value | Example | Source | Notes |
| --- | --- | --- | --- |
| Install type | system service | Choose | System service if you have root, otherwise user service. |
| Public hostname | `status.example.com` | Ask | Must resolve to the proxy. Sets `public_url`. |
| Extra hostnames | none | Ask | `public_origins`. |
| Administrator emails | `admin@example.com` | Ask | Exact addresses for `[auth].admins`. |
| Reverse proxy | nginx | Detect, else ask | nginx, Caddy, Traefik or another with forward-auth. |
| Authentication service | oauth2-proxy | Detect, else ask | Needs its verification address and the header it returns. |
| Identity header | `X-Forwarded-Email` | Derived | Default, except Traefik uses the verifier's own header. |
| Upstream address | `127.0.0.1:8080` | Choose a free port | Must not be public. |
| Mail relay | none | Ask | Host, port, sender, security, password source. Can be added later. |
| Automatic updates | off | Ask | Restarts the service unattended. Off unless authorized. |
| Monitors | HTTP targets | Ask | Targets, intervals, what is `public`. |
| Secret source | password manager | Ask | Where passwords come from. |

If the proxy or authentication service is absent, ask. Do not stand up the test verifier as a
substitute on a reachable address. With no answer, you can finish on loopback in `no-auth`
mode and report the remainder as open.

## Step 2: Install

```sh
VERSION=v0.2.2
TARGET=x86_64-unknown-linux-musl
BASE=https://github.com/ziozzang/sacho/releases/download/$VERSION
ASSETS=$BASE/sacho-$VERSION-deploy-docs.zip
curl -fsSLO "$BASE/sacho-$TARGET"
curl -fsSLO "$BASE/SHA256SUMS"
curl -fsSLO "$ASSETS"
sha256sum --check --ignore-missing SHA256SUMS
unzip -q "sacho-$VERSION-deploy-docs.zip" -d sacho-docs
sudo install -m 0755 "sacho-$TARGET" /usr/local/bin/sacho
sacho --version
```

**Pass:** the checksum line says `OK`, and the version matches. The documents archive is not
listed in `SHA256SUMS`, which only covers the program.

```sh
sudo useradd --system --home-dir /var/lib/sacho --shell /usr/sbin/nologin sacho
sudo install -d -m 0700 -o sacho -g sacho /var/lib/sacho /var/lib/sacho/secrets
sudo install -d -m 0750 -o root -g sacho /etc/sacho
sudo -u sacho sacho secrets init --vault /var/lib/sacho/secrets/secrets.enc \
  --key-file /var/lib/sacho/secrets/unlock.key
sudo install -m 0644 sacho-docs/examples/deploy/systemd/sacho.system.service /etc/systemd/system/sacho.service
```

## Step 3: Write the configuration

Start from `sacho-docs/examples/deploy/sacho-production.toml`. Top-level settings stay above
the first `[section]`. Remove `[smtp]` if there is no relay yet, and set `logout_url` to your
service's sign-out path or remove it.

```sh
sudo install -m 0640 -o root -g sacho sacho-docs/examples/deploy/sacho-production.toml /etc/sacho/sacho.toml
sudo sed -i \
  -e 's#^public_url = .*#public_url = "https://status.example.com"#' \
  -e 's#^admins = .*#admins = ["admin@example.com"]#' \
  /etc/sacho/sacho.toml
```

Store each secret by name. Your reader command stands in for the operator's secret source:

```sh
your-secret-reader | sudo -u sacho sacho secrets set SACHO_SMTP_PASSWORD \
  --vault /var/lib/sacho/secrets/secrets.enc --key-file /var/lib/sacho/secrets/unlock.key
```

The example lists `API_AUTHORIZATION` in `probe_env`. Replace it with the names your checks
use; a name without a value is harmless until a check reads it.

```sh
sudo systemctl daemon-reload
sudo systemd-run --wait --pipe -q -p User=sacho -p WorkingDirectory=/var/lib/sacho \
  -p EnvironmentFile=-/etc/sacho/secrets.env \
  /usr/local/bin/sacho doctor --config /etc/sacho/sacho.toml
```

**Pass:** `Configuration and database OK`. An error names the problem: see the table in
`DEPLOY.md` section 11.

## Step 4: Start

```sh
sudo systemctl enable --now sacho
curl -fsS --retry 5 --retry-connrefused http://127.0.0.1:8080/healthz
```

**Pass:** `{"status":"ok","version":"..."}`. On failure, read `journalctl -u sacho -n 50`.

## Step 5: Connect the proxy

Copy the matching file from `sacho-docs/examples/deploy/proxy/`, replace the hostname,
certificate paths and addresses, and reload. Other proxies must meet the route table in
`DEPLOY.md` section 3.

Get two test sessions from the operator: one administrator, one other person. Keep their
cookie values private. To test the wiring before a real authentication service exists, run
`mock-verifier.py` on loopback, with `session=admin` and `session=reader` as the cookies,
and stop it afterwards.

```sh
sh sacho-docs/examples/deploy/proxy/check-proxy.sh https://status.example.com \
  admin@example.com 'SESSION_COOKIE_OF_ADMIN' 'SESSION_COOKIE_OF_OTHER_PERSON'
```

Set `IDENTITY_HEADER` if you changed the header name, and `CURL_OPTS="--cacert FILE"` for a
private certificate.

**Pass:** `All checks passed.` and exit status 0. Treat any `FAIL` as a blocker; do not continue.
From a different machine, Sacho's port must not answer.

## Step 6: Add monitors

```sh
cd sacho-docs/examples/deploy/checks
./register-check.py --admin-email admin@example.com your-check.json
```

The script validates, runs once and saves. It is safe to repeat. It updates the monitor with
the same name, and stops on an outage result unless told otherwise. Fields not mentioned take
defaults. To change one field by hand, read the `spec` from `GET /api/admin/items`, edit it, and
`PUT` the whole object to `/api/admin/items/ID`: unknown fields are errors.

Common API calls, on the private address with
`-H 'X-Forwarded-Email: ADMIN' -H 'X-Sacho-Request: 1' -H 'Content-Type: application/json'`:

| Task | Call |
| --- | --- |
| List monitors | `GET /api/admin/items` |
| Check a specification | `POST /api/admin/validate` |
| Run once without saving | `POST /api/admin/test` |
| Create; replace | `POST /api/admin/items`; `PUT /api/admin/items/ID` |
| Run now; see results | `POST /api/admin/items/ID/run`; `GET /api/admin/items/ID/logs` |
| Store a secret input | `PUT /api/admin/inputs/NAME` with `{"value":"...","enabled":true,"encrypted":true}` |
| Issue a push key (shown once) | `POST /api/admin/push/ID/token` |
| Set page layout | `PUT /api/admin/layout` with `{"sections":[{"title":"...","item_ids":["ID"]}]}` |
| Set page title | `PUT /api/admin/settings` with `{"title":"...","description":"..."}` |
| Set reader groups | `PUT /api/admin/access-groups` with `[{"name":"ops","emails":["a@example.com"]}]` |
| Add a webhook | `POST /api/admin/webhooks` with `{"name":"...","url":"https://..."}` |
| Alert timing | `PUT /api/admin/notification-policy` |
| Enroll a worker | `POST /api/admin/worker-bootstrap` with `{"id":"edge-west","ttl_seconds":3600,"allow_scripts":false}` |

Send a body containing a secret through standard input (`curl --data @-`), never as an argument.

**Pass:** each monitor has a fresh result: `GET https://status.example.com/api/status` lists public
ones with `status` other than `unknown`, after one interval. A queued run is not a success.

## Step 7: Test restricted visibility

Do this if any monitor is not `public`. Set `"visibility":"groups"` and `"access_groups":["ops"]`
on a harmless test monitor, then check `/api/status` through the public address:

| Request | The monitor must be |
| --- | --- |
| No session | Absent |
| No session, with a forged identity header | Absent |
| Session of a person outside the group | Absent |
| Session of a person in the group | Present |

## Step 8: Mail, if configured

Subscribe a designated test mailbox:

```sh
curl -s -X POST https://status.example.com/api/subscribe -H 'Content-Type: application/json' \
  -H 'X-Sacho-Request: 1' -H 'Origin: https://status.example.com' -d '{"email":"test@example.com"}'
```

**Pass:** a confirmation message arrives. `GET /api/admin/notifications` shows delivery results.

## Step 9: Backup

Schedule the commands in `DEPLOY.md` section 8, with a timer of your choice, then restore one to
a temporary path with `sacho database import` and run `sacho database verify`.

**Pass:** both succeed, and the configuration, vault and key are also covered by your backup system.

## Step 10: Report

Say what was done, without any secret value:

- Version, install type, paths of the unit and configuration
- Public address, and the result of `check-proxy.sh`
- Monitor names, IDs, freshest results and visibility
- Input names (no values), and where each comes from
- Mail and automatic update state
- Backup location and the restore test result
- Anything unfinished, and exactly what you need from the operator

## Changing an existing installation

1. `sacho --version`; `systemctl cat sacho`; read the configuration with secret values masked.
2. Take a snapshot (`DEPLOY.md` section 8) before anything risky.
3. Edit, run `doctor`, then restart. `doctor` may migrate the database, so run it with the
   intended version and path. It does not conflict with a running server.
4. Re-run `check-proxy.sh` after any proxy or authentication change.
5. For an upgrade, follow `DEPLOY.md` section 9, including workers.

## Notification subscription rules / 알림 선택 규칙

See [SUBSCRIPTIONS.md](SUBSCRIPTIONS.md) for the rule key table, event kinds and matching semantics.
Use `sacho subscriptions schema` for offline JSON discovery; `subscriptions list|get|set` manages the same rules as `GET|PUT /api/admin/subscriptions/{email|webhook}/{ID}/rules`.
Empty rules preserve all notifications. A disabled final rule stays muted. Recovery uses the previous incident severity.
