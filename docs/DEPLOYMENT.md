# Deployment

[한국어](DEPLOYMENT.ko.md)

Use the native single executable on Linux or macOS. Unix is required for atomic
update activation. Native GNU Linux builds require a compatible glibc; optional
musl builds are fully static. SQLite/Lua and web assets are always embedded.

For a local evaluation, `sacho init` and `serve` use loopback with no-auth. Do not
forward that mode to an untrusted network. Public deployment uses auth-proxy:

```toml
bind = "127.0.0.1:8080"
database = "/home/sacho/data/sacho.db"
public_url = "https://sacho.example.com"
[auth]
mode = "auth-proxy"
email_header = "X-Forwarded-Email"
admins = ["operator@example.com"]
```

The upstream should remain private. On Hangang's host network, loopback works;
otherwise bind to a private interface with firewall/network restrictions. The
proxy must strip incoming identity headers and inject the authenticator's verified
email. Sacho trusts that header and compares it with the configured administrator
list; the proxy is the authentication trust boundary. Case-insensitive email match
is exact, not a suffix or domain match.

## Hangang + Hopae

[`deploy/hangang.example.json`](../deploy/hangang.example.json) defines public `/`
and protected `/admin` and `/api/admin/` routes. The protected routes have higher
priority. Adjust the host, listener IDs, backend address and existing authenticator
contract to your Hangang deployment. With Hopae, register the Sacho host in its apps
and use the same verified-email header/contract as your working protected routes.
The example names `HANGANG_HOPAE_KEY`, which belongs to Hangang's environment and
must never be supplied by the client. Sacho needs no copy of that gateway secret.

The public status/history/incident APIs omit scripts, target URLs, raw output,
email lists and notification destinations. Only explicitly enabled scalar display
values are public. The `/admin` HTML contains no private state; its API requests
still require identity even if you choose to leave that static shell public.

## systemd

Copy [`deploy/sacho.service`](../deploy/sacho.service) and adjust paths/user. Run the
binary as a dedicated user, provide a private `EnvironmentFile` for SMTP/check
credentials, and make the binary's directory writable to that user if automatic
updates are enabled. The service must invoke `serve`, not the internal `--worker`.
Use `Restart=on-failure`; Sacho owns updates and graceful worker replacement.

Keep the TOML config and environment file private. Configuration is validated on
startup; auth/SMTP/update changes take effect after restart. Public title/description,
monitors, webhook/subscriber management and layout are editable in the console.
The UI does not reveal or change actual SMTP/check secret values.

```toml
[smtp]
host = "smtp.example.com"
port = 587
from = "Status <status@example.com>"
username = "status@example.com"
password_env = "SACHO_SMTP_PASSWORD"
security = "starttls"
```

`tls` supports implicit TLS (usually 465); `plain` is restricted to literal loopback
or localhost for test relays. Configure `public_url` correctly for email links and
Origin checks. Confirmation links expire after 24h. Subscription requests are
limited globally to 100 confirmation jobs/hour and per-address to one/hour.

## Data and updates

Default retention is 100 days; configurable from 90 to 3650. Checks, delivered or
exhausted notification jobs and unconfirmed subscriptions are pruned hourly.
Incident history and verified subscriptions remain until explicitly managed. Use
SQLite's backup API or stop the service and copy the DB; do not copy an active DB
alone while WAL files are in use. A SIGTERM drains service work and checkpoints WAL.

Automatic update checks begin on startup and recur every `interval_seconds`.
The signed payload pins version, target, artifact URL, SHA-256 and size. GitHub
assets must belong exactly to `ziozzang/sacho` and the canonical version tag;
redirects are restricted to GitHub's release CDN. Custom HTTPS origins cannot
redirect across origins; `ca_file` optionally adds their private CA trust.

Releases are installed atomically, keeping a rollback hard link. A version
preflight runs before downtime; then the supervisor stops its worker, activates
the candidate and restarts it. Startup failure or exit during 30-second probation
restores the old executable. Successful probation removes the rollback. The parent
supervisor remains from the original process until the service itself restarts;
it tracks the worker's version for subsequent update checks. Changes to supervisor
code therefore take effect on a service restart. Power loss during probation can
leave a retained rollback file; automatic rollback is a running supervisor feature,
not a persistent boot recovery journal.

## Release signing

Set a base64 32-byte Ed25519 seed in the GitHub repository secret
`SACHO_RELEASE_SIGNING_KEY`. Commit the matching base64 public key in
`release-public-key.txt`. Never reuse a signing key between projects.
Tag `v<version>` matching Cargo.toml after CI passes. Native assets are built on
Linux/macOS x86_64/ARM64; a Linux x86_64 musl build provides a static executable.
The publish job signs each precise manifest with the native Sacho signer and
uploads executables, manifests and SHA256SUMS. Unsigned/missing releases may be
reported in metadata checks but cannot be installed by the server.
