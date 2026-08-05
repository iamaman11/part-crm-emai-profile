# Data Classification

**Статус:** normative baseline  
**Дата:** 2026-08-05

## 1. Классы

| Класс | Примеры | Минимальные правила |
|---|---|---|
| `PUBLIC` | README, public schemas, sanitized architecture | допускается Git и публичная документация |
| `INTERNAL` | non-sensitive build metadata, aggregate metrics, feature flags | authenticated access, no unnecessary public export |
| `CONFIDENTIAL` | client card, contact points, assignment history, device metadata | tenant authorization, encryption in transit/at rest, bounded audit |
| `SECRET` | proxy/mailbox secret handles, key identifiers, security configuration | least privilege, no logs/screenshots, rotation and access audit |
| `CREDENTIAL_EQUIVALENT` | cookies, `key4.db`, `logins.db`, localStorage, OAuth tokens, DEK/KEK/root keys | application-layer encryption, bounded plaintext, no ordinary export, incident handling as credential exposure |

Browser profile payload is always `CREDENTIAL_EQUIVALENT`, even when it does not
contain an obvious plaintext password.

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
| audit event | sanitized structured record | no | optional evidence object | local outbox before delivery | itself, without secret detail |

## 3. Identifier Rules

- Email, mailbox login, client name and directory name are never technical IDs.
- Public/resource IDs are opaque and validated as safe path segments.
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
- raw client email/phone unless a dedicated governed export requires it;
- screenshots containing uncontrolled PII.

## 5. Local Development

- Legacy originals remain read-only by policy and are never opened by browser or
  SQLite tooling in place.
- Tests use synthetic identifiers and generated secrets.
- Real credentials are supplied only through approved external secret storage.
- Fixtures and snapshots must pass no-secret/no-PII review before commit.
- Support bundles are allowlist-based, not redact-after-collection archives.

## 6. Incident Rule

Exposure of any browser profile archive, cookies, login database, OAuth token,
mailbox secret, proxy credential or encryption key is treated as a credential
incident. Deleting a Git line is not remediation; revoke/rotate, access review,
impact inventory and incident evidence are required.
