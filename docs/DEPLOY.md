# Deploying and configuring Sacho

[한국어](DEPLOY.ko.md) · [Agent runbook](AGENT-GUIDE.md) · [Example files](../examples/deploy/README.md)

This guide takes you from nothing to a public status page with a protected admin
console. It applies to Sacho v0.2.0. All names and addresses are examples: replace
`status.example.com`, `admin@example.com` and `127.0.0.1` ports with your own.

Longer reference guides are built into the program and work offline:
`sacho help setup`, `sacho help monitoring`, `sacho help workers`, `sacho help storage`.
Add `--language ko` for Korean.

## How the pieces fit

```text
visitors ──HTTPS──> reverse proxy ──> Sacho on 127.0.0.1:8080 ──> SQLite file
                        │                      ▲
                        ▼                      │ outbound only
               authentication service     remote workers (optional)
```

- **Sacho is one executable.** Its web pages, Lua, SQLite and encryption are inside.
  Python and shell checks use the `python3` and `/bin/sh` of the machine running them.
- **Sacho does not sign anyone in.** A reverse proxy verifies the person and passes
  their email address in a header. Sacho believes that header, so **nothing except
  the proxy may reach Sacho's port.** This is the most important rule in this guide.
- One controller serves the page. Running two controllers on one database is not supported.

## 1. Install

### Download and verify

Release v0.2.0 publishes Linux x86_64 only. Prefer the `musl` build: it is fully
static and runs on any Linux distribution. For another platform, build from source
with Rust 1.96+ and a C compiler: `cargo build --locked --release`.

```sh
VERSION=v0.2.0
TARGET=x86_64-unknown-linux-musl
BASE=https://github.com/ziozzang/sacho/releases/download/$VERSION
curl -fsSLO "$BASE/sacho-$TARGET"
curl -fsSLO "$BASE/SHA256SUMS"
sha256sum --check --ignore-missing SHA256SUMS
sudo install -m 0755 "sacho-$TARGET" /usr/local/bin/sacho
sacho --version
```

### Try it locally (one minute)

```sh
cp examples/deploy/sacho-minimal.toml sacho.toml
sacho doctor --config sacho.toml
sacho serve --config sacho.toml
```

Open `http://127.0.0.1:8080/` and `/admin`. This mode has no sign-in, and Sacho
refuses to start it on anything but a loopback address. Do not forward it to a network.

### System service (recommended)

A dedicated account, with data under `/var/lib/sacho` and configuration under `/etc/sacho`:

```sh
sudo useradd --system --home-dir /var/lib/sacho --shell /usr/sbin/nologin sacho
sudo install -d -m 0700 -o sacho -g sacho /var/lib/sacho /var/lib/sacho/secrets
sudo install -d -m 0750 -o root -g sacho /etc/sacho

# Create the encrypted store for secrets (skip if you will not use [secrets]).
sudo -u sacho sacho secrets init \
  --vault /var/lib/sacho/secrets/secrets.enc \
  --key-file /var/lib/sacho/secrets/unlock.key

# Configuration: edit every example.com value afterwards.
sudo install -m 0640 -o root -g sacho examples/deploy/sacho-production.toml /etc/sacho/sacho.toml
sudo install -m 0600 examples/deploy/secrets.env.example /etc/sacho/secrets.env
sudo editor /etc/sacho/sacho.toml /etc/sacho/secrets.env

sudo install -m 0644 examples/deploy/systemd/sacho.system.service /etc/systemd/system/sacho.service
sudo systemctl daemon-reload

# Validate using the service's own account and environment.
sudo systemd-run --wait --pipe -q -p User=sacho -p WorkingDirectory=/var/lib/sacho \
  -p EnvironmentFile=-/etc/sacho/secrets.env \
  /usr/local/bin/sacho doctor --config /etc/sacho/sacho.toml

sudo systemctl enable --now sacho
curl -fsS http://127.0.0.1:8080/healthz    # {"status":"ok","version":"0.2.0"}
```

The example file enables `[smtp]`, which needs a working password reference. Remove
that section at first if you have no mail relay yet. `doctor` creates or migrates the
database named in the configuration, so point it only at the intended file.

### User service (root needed only for the last line)

```sh
mkdir -p ~/.local/bin ~/.config/sacho ~/.config/systemd/user ~/.local/share/sacho
install -m 0755 "sacho-$TARGET" ~/.local/bin/sacho
cp examples/deploy/sacho-production.toml ~/.config/sacho/sacho.toml   # use absolute paths inside
cp examples/deploy/systemd/sacho.user.service ~/.config/systemd/user/sacho.service
chmod 700 ~/.config/sacho
systemctl --user daemon-reload
systemctl --user enable --now sacho
sudo loginctl enable-linger "$USER"     # keep running after you log out
```

## 2. Configuration file

Sacho reads one TOML file, given by `--config`, default `sacho.toml`. Every setting
below is optional unless marked required. Unknown names are errors, which catches
typing mistakes. **Top-level settings must come before the first `[section]`;** one
placed after `[auth]` fails with ``unknown field `probe_env` ``. Changes take effect
after a restart. Title, description, monitors, layout, webhooks, groups and inputs
are edited in the admin console and stored in the database instead.

### Top level

| Setting | Default | Meaning |
| --- | --- | --- |
| `bind` | `127.0.0.1:8080` | Address and port to listen on. Keep it private. |
| `database` | `sacho.db` | SQLite file. Use an absolute path. Keep it on a local disk, never a network share. |
| `public_url` | `http://127.0.0.1:8080` | Canonical public origin, without a path. Used in email links and Origin checks. |
| `public_origins` | `[]` | Extra exact origins allowed to write, at most 16, no wildcards. Example: `["https://monitor.example.com"]`. |
| `title` | `Sacho` | Initial page title. |
| `description` | `Service health, recorded with care.` | Initial page description. |
| `retention_days` | `100` | How long observations are kept, 90 to 3650. |
| `max_concurrent_checks` | `16` | Checks running at once, 1 to 128. |
| `probe_env` | `[]` | Environment names that checks may read, at most 32. Not `PATH`, `LANG` or `SACHO_PROBE_*`. |

### `[auth]`

| Setting | Default | Meaning |
| --- | --- | --- |
| `mode` | `no-auth` | `no-auth` (loopback only) or `auth-proxy`. |
| `email_header` | `X-Forwarded-Email` | Header carrying the verified email address. |
| `admins` | `[]` | Administrator addresses. Required with `auth-proxy`. Exact match, ignoring case; no wildcards or domains. |
| `logout_url` | none | Local path or `https://` URL of your proxy's sign-out page. Without it no sign-out link appears. |

### `[smtp]` (optional; without it, email subscriptions are visibly disabled)

| Setting | Default | Meaning |
| --- | --- | --- |
| `host` | required | Mail relay. |
| `port` | `587` | Relay port. |
| `from` | required | Sender, such as `Status <status@example.com>`. |
| `username` | none | Sign-in name for the relay. |
| `password_env` | none | **Name** of the environment variable or vault entry with the password. Sacho will not start if it is missing. |
| `security` | `starttls` | `starttls`, `tls` (usually port 465), or `plain` (only for loopback, `localhost` or a private IPv4 address). |

### `[updates]`

| Setting | Default | Meaning |
| --- | --- | --- |
| `enabled` | `false` | Install signed releases automatically and restart the worker process. |
| `interval_seconds` | `21600` | How often to look, at least 60. |
| `public_key` | built in | Your own base64 Ed25519 key, if you sign your own builds. |
| `manifest_url` | GitHub | Your own HTTPS location for a signed manifest. |
| `ca_file` | none | Extra private certificate authority for `manifest_url`. |

### `[secrets]` (optional; both required when present)

| Setting | Meaning |
| --- | --- |
| `vault` | Encrypted file, created by `sacho secrets init`. The admin console writes to it, so its directory must be writable by the service account. |
| `key_file` | Random unlock key. Must be owned by the service account with no group or other access. |

Environment values take priority over vault entries with the same name.

### `[[remote_workers]]` (older, manual worker registration)

Prefer one-time enrollment (section 7), which needs no configuration here. One block per
worker, at most 128: `id` (required), `token_env` (required: name holding a unique secret
of 32 to 512 printable characters, not listed in `probe_env`), `allow_scripts` (default `false`).

### Worker configuration

| Setting | Default | Meaning |
| --- | --- | --- |
| `control_url` | required | Controller address. HTTPS, or plain HTTP only for loopback. |
| `node_id` | required | This worker's name, matching its enrollment key. |
| `token_env` | required | Name under which its credential is stored. Not in `probe_env`. |
| `label` | empty | Friendly name shown in admin. |
| `probe_env` | `[]` | Inputs available to checks on this worker. The controller's are not shared. |
| `allow_scripts` | `false` | Accept Lua, shell and Python here. The controller must also grant it. |
| `ca_file` | none | Certificate authority for a private controller certificate. |
| `allow_insecure_http` | `false` | Plain HTTP to a non-loopback controller. For isolated tests only. |
| `controller_id`, `[secrets]` | written by `worker-join` | Do not fill these in by hand. |

### Logging

Logs go to standard output, so `journalctl -u sacho` shows them. The `RUST_LOG`
environment variable changes the detail, such as `RUST_LOG=sacho=debug`.

## 3. Reverse proxy

Tested working configurations: [nginx](../examples/deploy/proxy/nginx-sacho.conf),
[Caddy](../examples/deploy/proxy/Caddyfile),
[Traefik](../examples/deploy/proxy/traefik-dynamic.yml). They expect an authentication
service following the oauth2-proxy contract: `GET /oauth2/auth` answers 2xx with
`X-Auth-Request-Email` for a valid session, and 401 otherwise.

### What any proxy must do

| Path | Access | Identity header sent to Sacho |
| --- | --- | --- |
| `/admin` | Session required; send browsers to sign in | Verified email |
| `/api/admin/` | Session required; answer 401, not a redirect | Verified email |
| `/api/account` | Session required; answer 401, not a redirect | Verified email |
| `/api/status`, `/api/incidents`, `/api/history/` | Public, with optional identity | Email if signed in, otherwise none |
| `/api/push/`, `/api/workers/` | Public; they carry their own bearer credentials | None |
| everything else (`/`, `/assets/`, `/healthz`, `/api/subscribe`, `/subscription/`) | Public | None |

And on **every** route:

1. **Remove the identity header arriving from the client.** On public routes too:
   status data is filtered per person, so a forged header there would reveal private monitors.
2. Set the header at most once. Two values count as no identity.
3. Do not forward your sign-in cookie. Sacho uses no cookies.
4. Redirect HTTP to HTTPS. Set `Strict-Transport-Security` at the proxy.
5. Do not send workers or push reporters to a sign-in page. Workers treat a redirect as an error.

Sign-in and sign-out pages of your authentication service must also be routed on this
hostname, as `/oauth2/` is in the examples, if it hands the session back on the application host.

If you keep every monitor public, you can drop the "optional identity" row: those paths
become plain public routes. Signed-in visitors then see the same page as everyone else.

### nginx

```sh
sudo cp examples/deploy/proxy/nginx-sacho.conf /etc/nginx/conf.d/sacho.conf
sudo editor /etc/nginx/conf.d/sacho.conf
sudo nginx -t && sudo systemctl reload nginx
```

The file sets the identity once, at server level, from a variable that only verified
locations fill in. In nginx, a `proxy_set_header` inside a location discards all
inherited ones, so do not add such a line to a location without repeating the others.

### Caddy

```sh
sudo cp examples/deploy/proxy/Caddyfile /etc/caddy/Caddyfile
sudo editor /etc/caddy/Caddyfile
sudo caddy validate --config /etc/caddy/Caddyfile && sudo systemctl reload caddy
```

### Traefik

Load [traefik-dynamic.yml](../examples/deploy/proxy/traefik-dynamic.yml) through the file
provider. Traefik cannot rename headers, so set `email_header = "X-Auth-Request-Email"` in
Sacho. For signed-in visitors to see their restricted monitors, your verifier needs an
endpoint that always answers 200 and adds the email only for a valid session. The comments
in the file explain how to leave that router out if yours has none.

### Authentication service

Any service that can answer a forward-auth check works. For oauth2-proxy, these options
give the contract above; the provider settings depend on your identity provider:

```sh
oauth2-proxy --http-address=127.0.0.1:4180 --reverse-proxy=true --set-xauthrequest=true \
  --upstream=static://202 --redirect-url=https://status.example.com/oauth2/callback \
  --email-domain='*'   # plus your --provider, client ID and cookie secret settings
```

`--email-domain='*'` lets any account of your provider sign in. That is fine for Sacho's
own decision, because only the addresses in `admins` get admin rights, but anyone signed
in can see monitors with `authenticated` visibility. Narrow it if that matters.

The [test verifier](../examples/deploy/proxy/mock-verifier.py) stands in for a real service
while you test the wiring. It has no real sign-in: never expose it.

### Check it

[check-proxy.sh](../examples/deploy/proxy/check-proxy.sh) tests the identity boundary from
the outside and changes nothing on the server:

```sh
sh examples/deploy/proxy/check-proxy.sh https://status.example.com admin@example.com \
   'your_session_cookie=ADMIN_VALUE' 'your_session_cookie=READER_VALUE'
```

Also confirm from another machine that Sacho's own port does not answer:
`curl --max-time 5 http://YOUR_SERVER:8080/healthz` must fail.

These three configurations and the script were run against Sacho v0.2.0 with nginx
(Debian 13), Caddy 2.11 and Traefik 3.7 using the test verifier. A real identity provider
was not part of that test.

## 4. Authentication and access

- **Who is an administrator:** whoever arrives with a header value listed in `admins`.
  Edit the list and restart to change it. Administrators can run commands on the server
  through script checks, so treat this like shell access.
- **What counts as identity:** exactly one header value, a valid email address, at most 254
  characters. It is compared in lowercase. Anything else means anonymous.
- **Reading monitors:** each monitor has `visibility`: `public` (default), `authenticated`
  (anyone signed in) or `groups` (members of a named group, set in admin under
  **Settings → Data access groups**). Checks happen on every request, so a removed member
  loses access at once. Hidden monitors stay out of the page, the overall health and public emails.
- **Writing:** every change requires the header `X-Sacho-Request: 1`, and a browser's
  `Origin` must match `public_url` or `public_origins`. This blocks forged cross-site requests.
- **Extra hostnames:** add the name to your proxy and your authentication service, and to
  `public_origins`. `public_url` stays the single name used in emails.

## 5. Email and webhooks

```toml
[smtp]
host = "smtp.example.com"
port = 587
from = "Example Status <status@example.com>"
username = "status@example.com"
password_env = "SACHO_SMTP_PASSWORD"
security = "starttls"
```

Store the password in either place, then restart:

```sh
# Encrypted store. Read the value from your password manager, not from the command line.
your-secret-reader | sudo -u sacho sacho secrets set SACHO_SMTP_PASSWORD \
  --vault /var/lib/sacho/secrets/secrets.enc --key-file /var/lib/sacho/secrets/unlock.key
```

Or put `SACHO_SMTP_PASSWORD=...` in the private `/etc/sacho/secrets.env`. Do not add this
name to `probe_env`. Test by subscribing a test mailbox on the status page: a confirmation
message should arrive, and its link is valid for 24 hours. Requests are limited to 100
confirmation messages per hour overall and one per hour per address.

Monitors with restricted visibility never email public subscribers. In **Notifications**
you can also register webhooks, edit message templates with a preview, delay alerts
(`settle_seconds`), limit repeats (`cooldown_seconds`) and silence planned maintenance.
Webhook delivery can repeat, so receivers should ignore a repeated `Idempotency-Key`.

## 6. Adding monitors

Use the console at `/admin`, or the JSON API, which an automated setup should prefer.
Through the public address the API needs a real session. On the Sacho machine itself,
the private address accepts the identity header directly:

```sh
curl -s http://127.0.0.1:8080/api/admin/items -H 'X-Forwarded-Email: admin@example.com'
```

[register-check.py](../examples/deploy/checks/register-check.py) validates a check, tries it
once, then creates it or updates the one with the same name, keeping its history:

```sh
cd examples/deploy/checks
./register-check.py --admin-email admin@example.com http-json-value.json
./register-check.py --admin-email admin@example.com lua.json
./register-check.py --admin-email admin@example.com python.json
./register-check.py --admin-email admin@example.com push.json
```

| Example | Shows |
| --- | --- |
| [http.json](../examples/deploy/checks/http.json) | Website with a slow-response warning, tolerating brief failures |
| [http-json-value.json](../examples/deploy/checks/http-json-value.json) | Showing one JSON value publicly |
| [lua.json](../examples/deploy/checks/lua.json), [lua-scenario.lua](../examples/deploy/checks/lua-scenario.lua) | Several requests with content checks; nothing to install |
| [python.json](../examples/deploy/checks/python.json), [python-disk.py](../examples/deploy/checks/python-disk.py) | Local script with warning and critical levels and a chart |
| [push.json](../examples/deploy/checks/push.json), [push-report.sh](../examples/deploy/checks/push-report.sh) | A job that reports in; silence means failure |

Sacho also has built-in DNS, TCP, ping, SNMP and Prometheus checks: see `sacho help monitoring`
and `sacho template --help`.

**Secrets in checks.** List the name in `probe_env` (or add it in admin under **Environment &
secrets**), and read it with `sacho.env("NAME")` in Lua, `os.environ["NAME"]` in Python or
`"$NAME"` in shell. HTTP checks use `"headers_env": {"Authorization": "NAME"}`, where the
value contains the whole header, including `Bearer `.

**Push reports.** Save the monitor, then issue its key, which is shown only once:

```sh
curl -s -X POST http://127.0.0.1:8080/api/admin/push/MONITOR_ID/token \
  -H 'X-Forwarded-Email: admin@example.com' -H 'X-Sacho-Request: 1'
```

Save `token` in a private file on the reporting machine, and call
`push-report.sh https://status.example.com MONITOR_ID /path/to/token 12.5` from the job.
`interval_seconds`, at most 86400, is the longest allowed silence. For a daily job,
allow for run-time variation with `failure_threshold` or by reporting more often.

## 7. Remote workers

A worker tests from another network. It only makes outgoing connections and needs no
database or open port. Use the same binary version as the controller.

1. In admin under **Agents**, choose a node name, a lifetime and whether scripts are allowed,
   and create a one-time key. By API: `POST /api/admin/worker-bootstrap` with
   `{"id":"edge-west","ttl_seconds":3600,"allow_scripts":false}`.
2. On the worker machine, install the binary and create the account and `/var/lib/sacho` as in
   section 1. Copy [worker.toml](../examples/deploy/worker.toml) to `/var/lib/sacho/worker.toml`,
   owned by that account with mode 0600. Set `control_url`, `node_id` and `label`.
3. Join, feeding the key through standard input so it stays out of shell history:

   ```sh
   read -rs key                        # paste, then Enter
   printf '%s' "$key" | sudo -u sacho sacho worker-join --config /var/lib/sacho/worker.toml
   unset key
   ```

   The directory must be writable by that account: joining creates an encrypted credential
   beside the configuration and rewrites the file.
4. Run [sacho-worker.service](../examples/deploy/systemd/sacho-worker.service).
5. In a monitor, choose the worker under **Execution location** (`"worker": "edge-west"` by API).

A private certificate authority goes in `ca_file`. It must be a real authority that issued the
server certificate: a self-signed server certificate fails with `CaUsedAsEndEntity`. Workers
do not update themselves. Upgrade them with the controller. To disconnect one, revoke it in **Agents**.

## 8. Backup and restore

Back up these together, and keep the database snapshot and vault apart from the unlock key:

| What | Where (as in this guide) | Why |
| --- | --- | --- |
| Configuration | `/etc/sacho/sacho.toml`, `/etc/sacho/secrets.env` | Not in the database |
| Database snapshot | below | Monitors, scripts, history, subscribers |
| Encrypted store and key | `/var/lib/sacho/secrets/` | Encrypted inputs are unreadable without them |

Snapshot, safe while running:

```sh
sudo install -d -m 0700 -o sacho -g sacho /var/backups/sacho
FILE=/var/backups/sacho/sacho-$(date -u +%Y%m%dT%H%M%SZ).json
sudo -u sacho sacho database export --database /var/lib/sacho/sacho.db --output "$FILE"
sudo -u sacho sacho database verify --input "$FILE"
```

Snapshots are plain text, including scripts, logs and subscriber addresses, so protect them.
Existing files are never overwritten. A snapshot may be at most 512 MiB. A larger database
needs a file copy instead: `sqlite3 sacho.db ".backup /path/copy.db"`, or stop the service and
copy the file. Never copy a running database with plain `cp`.

Restore, which refuses to overwrite an existing database:

```sh
sudo systemctl stop sacho
sudo -u sacho mv /var/lib/sacho/sacho.db /var/lib/sacho/sacho.db.before-restore
sudo -u sacho sacho database import --input "$FILE" --database /var/lib/sacho/sacho.db
sudo systemctl start sacho
sudo -u sacho sacho doctor --config /etc/sacho/sacho.toml
```

If the old database has `-wal` and `-shm` files beside it, move them aside as well. Keep
the old copy until the page looks right. Do not run the old and new servers side by side,
or subscribers can get duplicate messages.

## 9. Upgrade and roll back

Take a snapshot first (section 8). Read the release notes.

**Manual replacement**, simplest and suited to `/usr/local/bin`:

```sh
sudo install -m 0755 /usr/local/bin/sacho /usr/local/bin/sacho.previous
# Download and verify the new version as in section 1, then:
sudo install -m 0755 "sacho-$TARGET" /usr/local/bin/sacho
sudo systemctl restart sacho
curl -fsS http://127.0.0.1:8080/healthz
```

**`sacho update`** verifies the signature, checksum and target for you. It needs the service
stopped, because it holds the database lock, and an executable in a directory that the
account can write:

```sh
sacho update --check     # report only; no configuration needed
sudo systemctl stop sacho
sudo -u sacho sacho update --config /etc/sacho/sacho.toml
sudo systemctl start sacho
```

It keeps the old program beside the new one as `.sacho-rollback-XXXXXX`, and tells you the name.

**Automatic updates** (`[updates] enabled = true`) also need a writable executable directory,
so install under `/var/lib/sacho/bin` and adjust `ExecStart`. If the new version fails to start
or exits within 30 seconds, the old one is restored. A power cut during those 30 seconds can
leave the new version running and the rollback file in place.

**Rolling back.** Stop the service, put the old executable back, and start it. If the old
version refuses to open the database because its format is newer, restore the snapshot taken
before upgrading (section 8) with the old binary; observations since then are lost.
Upgrade workers along with the controller, as older ones may reject newer monitor settings.

## 10. Security checklist

- [ ] Sacho's port is reachable only by the proxy. **On a machine shared with other users,
      any local process can reach a loopback port and claim to be an administrator:** run Sacho
      in its own container or network namespace there, or use a firewall owner rule.
- [ ] The proxy removes the client's identity header on every route. `check-proxy.sh` passes.
- [ ] If proxy and Sacho are on different machines, the link between them is private or encrypted.
- [ ] `no-auth` is used only for local evaluation.
- [ ] Sacho runs under its own account with no sudo rights. Script checks run with that
      account's permissions, and only trusted administrators can define them.
- [ ] The Docker socket is not granted unless needed. Reading it gives control of the host.
- [ ] `sacho.toml`, `secrets.env`, the vault, the key and snapshots are private and outside Git.
- [ ] Passwords are never placed in TOML, check scripts, URLs or command-line arguments.
- [ ] New monitors exposing internal names or targets use `authenticated` or `groups`:
      `public` is the default. Only values you choose to publish are shown.
- [ ] `allow_scripts` is off on workers unless you trust the controller's administrators
      to run code there.
- [ ] Backups are tested by restoring them.

## 11. Troubleshooting

| Symptom | Cause and fix |
| --- | --- |
| ``unknown field `...` `` at start | Spelling, or a top-level setting placed after a `[section]`. Move it up. |
| `no-auth must bind to loopback` | Use `auth-proxy` for any other address. |
| `auth-proxy requires admin email addresses` | Fill in `[auth].admins`. |
| `SMTP password environment variable is missing` | The name in `password_env` is in neither the service environment nor the vault. Your own shell's variables do not count. |
| `plain SMTP is only allowed on loopback or private IPv4 networks` | Use `starttls` or `tls`, or a private relay given as an IP address. |
| `File exists` from `sacho init` | It never overwrites. Use the existing file or another path. |
| `another Sacho server or update is using this database` | Stop the service first, or check for a second copy. |
| Vault refuses to load | Files must be owned by the service account, with no group or other access. |
| Saving a secret in the console fails | The vault's directory must be writable by the service: check `ReadWritePaths` in the unit. |
| Admin API 401 | No valid identity: session expired, the address is not in `admins` (exact match), or the proxy did not pass the header. Check `/api/account` with the session: it shows the email Sacho received. |
| Admin API 403 | `X-Sacho-Request: 1` missing, or `Origin` not in `public_url`/`public_origins`. |
| `{"error":"X-Sacho-Request: 1 required."}` on subscribe | The same header is required on `/api/subscribe`. |
| API 400 | Invalid fields. Common: `timeout_seconds` above `interval_seconds`; `interval_seconds` outside 5 to 86400. `POST /api/admin/validate` lists errors. |
| API 429 | Too many test runs at once. Wait and retry. |
| Signed-in people do not see their restricted monitors | The status paths have no optional identity at the proxy. See section 3. |
| Monitor stays `unknown` | No result yet. For a push monitor, none reported yet; for a worker monitor, the worker is not connected. |
| Check cannot read a value | The name must be in `probe_env` and in the service environment on the machine that runs the check. |
| Lua warning `NAME: not in probe_env` for a name you do not use | Validation also reads `sacho.env("...")` inside comments. Reword the comment. |
| Worker `named input is not available` at start | It has not joined yet. Run `worker-join` first. |
| `worker-join`: `read a one-time bootstrap key from stdin` | Registration has not happened, so a key is needed. A connection failure does not use up the key. If it has expired, issue a new one. |
| Worker `CaUsedAsEndEntity` | `ca_file` is a self-signed server certificate. Use a certificate authority plus a server certificate issued by it. |
| Worker fails with a redirect error | The proxy sent `/api/workers/` to sign-in. Make that path public. |
| Ping checks fail but the host is up | The machine needs Linux ping sockets (`net.ipv4.ping_group_range`) or the raw-socket capability. |
| Database keeps growing | Lower `retention_days` (at least 90) or use longer check intervals. Pruning runs hourly. |
