# Select notification items and events

Copyright Jioh L. Jung <jung@jioh.net>.

An email subscriber or webhook is a subscription. Click **Subscribe** on a monitor or incident row to select a subscription, an item and its events. Anonymous visitors create an email subscription and must confirm it by email. Knowing an existing email address never authorizes changing its rules: existing subscribers use their trusted proxy identity or the **Manage subscription rules** link in a notification email. Administrators manage email and webhook rules under **Notifications → Subscription rules**. Existing verified subscribers also get a management button on the public page.

A subscription has at most 64 rules. A rule matches when its target, event and minimum severity all match. Individual item IDs and check group names are alternatives (OR), so a matching group includes new items added to that group. Both target lists empty means all targets. Group names are the monitor's `group`, not read-access groups; there is no separate host or tag catalogue in Sacho. Hosts, services and metrics are all monitored items. Use the existing check group to collect them.

Rules are alternatives (OR). Matching several rules sends a single event once to a destination. Empty rules preserve the original all-events behavior. Existing database rows gain empty rules automatically when the database opens; no manual migration is needed. Disabled rules stay in the list and do not match, so disabling the last rule mutes the subscription. Deleting every rule intentionally restores all events. Exclusions always take precedence and mute just one item/event pair.

## Events and severity

| Event | Meaning | Severity used for filtering |
| --- | --- | --- |
| `incident.started` | A check crosses its consecutive-failure threshold and opens an incident. Collection/probe failures use this same path. | `degraded` → warning, `outage` → critical |
| `incident.resolved` | A check crosses its consecutive-recovery threshold and resolves an incident. | Last incident severity, even though current status is operational |

`info` accepts all severities; `warning` accepts warning and critical; `critical` accepts only critical. Empty event lists accept both events. Confirmation mail (`subscription.verify`) is independent of these rules. Sacho does not currently emit separate collection-failure, tag, or arbitrary state-change notifications. The event log panel displays retained logs and does not create extra notification event kinds.

A recovery-only subscription receives recovery even without a previous problem email. Existing global maintenance, delay and cooldown policies still apply; a recovery paired with a suppressed problem remains suppressed for subscriptions that selected both events. The worker rechecks rules and data visibility immediately before each attempt. A rule edit or unsubscribe suppresses pending matching jobs. Public email never contains restricted monitors; operational webhooks retain their existing administrator-managed access.

## Rule keys and API

These keys are stored per subscription in the database; they are **not server TOML settings**. The same JSON is used by API and CLI. Settings storage is included in SQLite and PostgreSQL portable backups. Treat backups as private: subscriber bearer tokens and the link-signing key are stored there too.

| Scope | Key | Type | Default | Meaning |
| --- | --- | --- | --- | --- |
| selection | `rules` | array | `[]` | OR of selection rules; empty means all events |
| selection | `exclusions` | array | `[]` | Up to 256 `{item_id,event}` pairs muted by legacy all-events mail links |
| selection | `revision` | string | server managed | Any manual replacement invalidates prior rule-mute links |
| rule | `id` | string | required | Unique within the subscription; up to 64 letters, digits, `_` or `-` |
| rule | `item_ids` | string array | `[]` | Up to 128 individual monitored item IDs |
| rule | `groups` | string array | `[]` | Up to 128 check group names, OR with item IDs |
| rule | `events` | string array | `[]` | `incident.started`, `incident.resolved`; empty means both |
| rule | `min_severity` | string | `info` | `info`, `warning`, or `critical` |
| rule | `enabled` | boolean | `true` | Disable without deleting or falling back to all events |

```json
{"rules":[{"id":"lab-recovery","item_ids":["ITEM_ID"],"groups":[],"events":["incident.resolved"],"min_severity":"critical","enabled":true}],"exclusions":[]}
```

- Administrator: `GET|PUT /api/admin/subscriptions/{email|webhook}/{ID}/rules`. IDs come from `GET /api/admin/subscribers` or `/api/admin/webhooks`. PUT replaces the whole selection; `{"rules":[]}` clears it. Existing administrator authentication, same-origin validation and `X-Sacho-Request: 1` apply.
- Subscriber: `GET|PUT /api/account/rules`, with the trusted proxy email of a verified subscriber, or `Authorization: Bearer TOKEN` from that subscriber's email link. PUT requires `X-Sacho-Request: 1` and an allowed Origin. Subscriber targets must be public items/groups. Tokens are never returned in administrator list APIs.
- New email: `POST /api/subscribe` with `{"email":"reader@example.com","rules":[...]}`. Empty or omitted rules retain all events. Changing an existing address by this unauthenticated endpoint is intentionally ignored; use its management link instead. Existing verification, request-header and rate limits still apply.

## CLI

```sh
sacho subscriptions schema
sacho subscriptions --database sacho.db list
sacho subscriptions --database sacho.db get --channel email --id SUBSCRIBER_ID
sacho subscriptions --database sacho.db set --channel email --id SUBSCRIBER_ID --input rules.json
sacho subscriptions --database sacho.db set --channel webhook --id WEBHOOK_ID --input rules.json
```

The CLI requires an existing SQLite database and uses the same validation/storage as the API. It does not require network authentication or send mail. Protect filesystem access to the database. To remove one rule, get the JSON, remove that entry, and set the file back. To disable a rule, set `enabled:false`. `subscriptions schema` describes keys and events offline without opening a database.

## Mute one rule from email

Notification mail always appends a **Mute this rule / 이 알림만 끄기** link for each matching rule, even with custom mail templates. The link carries an HMAC-SHA256 signature, subscription ID, rule ID, selection revision, delivery nonce and a 30-day expiry. Opening the link only displays a confirmation form; POST consumes it once. Forged, expired, reused and obsolete-revision links are rejected. Each other rule's link in the same email remains usable. The rule is disabled, not deleted. For old all-events subscriptions the link adds an exclusion for exactly that item/event instead. Other subscriptions and the full unsubscribe link remain independent.

The permanent management/full-unsubscribe tokens already used by Sacho remain sensitive capabilities. The management page removes its token from browser history after reading it and does not save it in local storage. All subscription responses retain the application's `no-store` and `no-referrer` headers.
