# Settings and the natural language assistant

Copyright Jioh L. Jung <jung@jioh.net>.

[한국어](SETTINGS.ko.md)

Open **Admin → Settings → Server settings** to edit configuration-file fields.
Fields, types, defaults and descriptions come from the same configuration schema
as `sacho --help --json`. Enter strings directly and numbers, booleans and arrays
as JSON. Empty optional fields remove the value. Fill required children to enable
an optional table; use **Propose removal** to disable it.

Select **Validate changes**, review the previous/new values, and explicitly choose
**Approve and apply reviewed changes**. The server uses the same validation as
`sacho config check`. Proposals expire after one hour and become invalid if the
configuration file changes. The model cannot apply changes itself.

The service account needs write access to the configuration file and its directory.
Saving atomically replaces the file, preserves its permissions, and rewrites TOML
formatting and comments. Database relocation requires an offline migration.
The supervisor (`sacho serve`) detects file changes and restarts its worker. Expect
a short interruption; reconnect using the new address/identity after address or
authentication changes. Direct `serve --worker` processes require an operator restart.
If the new worker cannot become ready, the supervisor restores the previous file.
Compare current settings with history after such a recovery and check port use.
The public-page title/description controls above take precedence over file defaults.

History records the approving administrator, time, old/new settings and monitor
creation. **Roll back this change** restores the previous settings or removes the
created monitor. Later edits prevent overwriting: prepare a new proposal instead.

## Connect an LLM

Fill `llm.base_url`, `llm.model` and exactly one of `llm.api_key_env` or
`llm.api_key_file` in Server settings and approve the proposal:

```toml
[llm]
base_url = "https://api.example.com/v1"
model = "your-model"
api_key_env = "SACHO_LLM_API_KEY"
```

Supply the key through the service's environment or use
`api_key_file = "/etc/sacho/llm.key"` instead of `api_key_env`. Key files must be
private to their owner (for example, mode 600). The UI shows configured/absent and
references, never key values. Do not allow the LLM key as a probe input.
Use HTTPS; loopback HTTP supports local model servers. API URLs cannot contain
credentials or query parameters. After restart, **Test LLM connection** verifies
access and the model. This contacts the actual provider and may incur charges.
Key values are excluded from API responses, logs and history.

With no `llm` table, the assistant is hidden and the connection test is disabled;
all other features continue working. Remove the table to disable the assistant.

## Use the assistant

Choose Server, Notifications, Agent network check, or Prometheus/Grafana. Describe
the desired change, such as “keep observations for 120 days.” The model proposes
strict JSON, validated by the server and reviewed through the same approval UI.
Validation errors are sent back for at most three attempts.

Register agents in **Agents** first. Supported generated monitors are HTTP, TCP,
DNS, ping and Prometheus checks on the local server or an already registered agent.
For CPU/disk metrics, connect an existing node exporter through Prometheus.
Configure Grafana separately to query that Prometheus server. The assistant does
not edit Grafana itself or generate executable scripts.

Only allowlisted ordinary settings and the request are sent to the provider.
Existing credentials, paths, secret references and check code are omitted; known
secret values are redacted. Never include credentials or sensitive data in requests.
Model changes to authentication, credentials/references, executable scripts,
actions, or unknown keys are rejected. Embedded prompt version 1 includes the
current embedded agent guide and shared configuration schema at build time.

Hermetic tests use a fake provider: `python3 tests/settings_wizard.py`; add
`--browser` for Chromium screenshots. Actual-provider checks are optional: use
**Test LLM connection**, then generate and discard a small proposal. No application
is needed to verify a connection.
