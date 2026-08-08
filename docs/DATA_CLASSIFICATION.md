# Data Classification

**Статус:** normative baseline  
**Дата:** 2026-08-08

## 1. Классы

| Класс | Примеры | Минимальные правила |
|---|---|---|
| `PUBLIC` | README, public schemas, sanitized architecture | допускается Git и публичная документация |
| `INTERNAL` | non-sensitive build metadata, aggregate metrics, feature flags | authenticated access, no unnecessary public export |
| `CONFIDENTIAL` | client card, contact points, assignment history, device metadata, mailbox message metadata/body | tenant authorization, encryption in transit/at rest where persisted, bounded plaintext exposure, no ordinary logs/audit |
| `SECRET` | proxy/mailbox secret handles, key identifiers, security configuration | least privilege, no logs/screenshots, rotation and access audit |
| `CREDENTIAL_EQUIVALENT` | cookies, `key4.db`, `logins.db`, localStorage, OAuth tokens, DEK/KEK/root keys | application-layer encryption, bounded plaintext, no ordinary export, incident handling as credential exposure |

Browser profile payload is always `CREDENTIAL_EQUIVALENT`, even when it does not
contain an obvious plaintext password.

Mailbox message content is product data and is accessible to authorized users. It is
classified as `CONFIDENTIAL` by default, but a message body may itself contain OTPs,
password-reset links, credentials or other higher-sensitivity values. Therefore mailbox
content uses stricter handling than ordinary display metadata: no ordinary logging,
audit/event payloads, telemetry or browser-storage persistence.

## 2. Storage Matrix

| Data | D1 | Durable Object | R2 | Bridge filesystem | Logs/audit |
|---|---|---|---|---|---|
| IDs/status/version | yes | minimal coordination only | manifest reference | cache allowed | safe identifiers/status |
| client contact display value | encrypted only | no | no | only when needed | never raw |
| contact lookup token | tenant-keyed HMAC | no | no | no | never |
| profile archive | no | no | encrypted immutable only | encrypted-at-rest workspace; plaintext only during active use | never |
| root/KEK/DEK plaintext | no | no | no | bounded memory only when authorized | never |
| wrapped keys/key IDs | governed metadata | hash/reference only | authenticated manifest metadata | cache only if policy permits | key ID/version only |
| mailbox/proxy credential | secret handle only | no | no | OS-protected secret adapter | never |
| mailbox message headers/subject/snippet | bounded governed projection only if required by product/query design | no | no by default | provider/local cache only under explicit policy | never raw unless a dedicated governed export requires it |
| mailbox message body | no canonical copy by default; future encrypted cache/index requires separate design approval | no | no canonical copy by default | transient/provider cache or encrypted local cache only when required by adapter policy | never |
| audit event | sanitized structured record | no | optional evidence object | local outbox before delivery | itself, without secret detail |

## 3. Identifier Rules

- Email, mailbox login, client name, message subject and directory name are never technical IDs.
- Public/resource IDs are opaque and validated as safe path segments.
- Provider message references exposed through the API are opaque/provider-scoped and cannot be used to bypass client/mailbox authorization.
- Tenant-owned keys and uniqueness include `tenant_id`.
- Raw PII is prohibited in URLs, R2 keys, filenames, metric labels and correlation IDs.

## 4. Logging And Evidence

Allowed:

- opaque IDs;
- status/problem code;
- correlation/idempotency reference;
- runtime/key version identifiers;
- sizes, durations and bounded counters;
- sanitized evidence digest.

Prohibited:

- cookies, authorization headers and JWT bodies;
- mailbox/proxy passwords or tokens;
- private/device/root/KEK/DEK material;
- full message body or attachment;
- raw message subject/sender/recipient in ordinary infrastructure logs unless a dedicated governed support/export flow explicitly permits it;
- raw client email/phone unless a dedicated governed export requires it;
- screenshots containing uncontrolled PII.

A user-visible message body is **not** an audit/log payload. Authorized product delivery
over HTTPS is allowed; copying that body into logs, audit, metrics, realtime/integration
events or support bundles is prohibited.

## 5. Mailbox Message Search And Body Access

The product must support client-scoped mailbox search and full message-body viewing for
authorized users.

Rules:

- authorization is evaluated before provider search or message-body retrieval;
- search is scoped to mailbox bindings eligible for the requested client/resource context;
- the initial implementation may use provider-native search/fetch or a Bridge/provider adapter; a central D1 blind/full-text index is not mandatory;
- if an adapter must perform bounded fetch/search internally, that remains behind the provider-neutral query port;
- message body may exist transiently in authorized Worker/Bridge process memory and in the HTTPS response needed by the UI;
- React must not persist message bodies in `localStorage` or `sessionStorage`;
- HTML mail is sanitized/sandboxed before rendering and remote images/external active content are disabled by default;
- ordinary audit/events record only safe action/resource metadata, never the subject/body itself;
- any future encrypted body cache, full-text index or blind index requires a separate storage/security decision and retention policy.

Attachments are outside the first mailbox-search slice and require their own access,
malware/content-handling and retention rules before product exposure.

## 6. Local Development

- Legacy originals remain read-only by policy and are never opened by browser or
  SQLite tooling in place.
- Tests use synthetic identifiers, generated secrets and synthetic mailbox content.
- Real credentials are supplied only through approved external secret storage.
- Fixtures and snapshots must pass no-secret/no-PII review before commit.
- Support bundles are allowlist-based, not redact-after-collection archives.
- Real mailbox bodies must not be committed as fixtures or copied into issue/CI logs.

## 7. Incident Rule

Exposure of any browser profile archive, cookies, login database, OAuth token,
mailbox secret, proxy credential or encryption key is treated as a credential
incident. Deleting a Git line is not remediation; revoke/rotate, access review,
impact inventory and incident evidence are required.

Exposure of mailbox message content is handled as a confidentiality/PII incident unless
the exposed content also contains credentials or credential-equivalent material, in which
case the stronger credential incident procedure applies.