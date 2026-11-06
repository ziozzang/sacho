# Deployment examples

Ready-to-edit files for [docs/DEPLOY.md](../../docs/DEPLOY.md) and
[docs/AGENT-GUIDE.md](../../docs/AGENT-GUIDE.md). Every name, address and password
is a placeholder. Replace `status.example.com`, `admin@example.com` and `replace-me`.

| File | Purpose |
| --- | --- |
| `sacho-minimal.toml` | Local trial without sign-in |
| `sacho-production.toml` | Behind a proxy, with mail and encrypted secrets |
| `secrets.env.example` | Private environment file for the service |
| `worker.toml` | Remote worker |
| `systemd/sacho.system.service` | System service with a dedicated account |
| `systemd/sacho.user.service` | Service for one user, no root |
| `systemd/sacho-worker.service` | Remote worker service |
| `proxy/nginx-sacho.conf`, `proxy/Caddyfile`, `proxy/traefik-dynamic.yml` | Reverse proxy with forward authentication |
| `proxy/check-proxy.sh` | Tests the identity boundary from outside; changes nothing |
| `proxy/mock-verifier.py` | **Test only** stand-in for an authentication service |
| `checks/*.json` | Monitor definitions: HTTP, Lua, Python, push |
| `checks/lua-scenario.lua`, `checks/python-disk.py` | Scripts used by those definitions |
| `checks/register-check.py` | Validates, tests, then creates or updates a monitor |
| `checks/push-report.sh` | Sends a result from a job to a push monitor |
