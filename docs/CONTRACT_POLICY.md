# Contract Compatibility Policy

**Статус:** normative v1 policy  
**Дата:** 2026-08-12

## 1. Version Roots

- Web API root: `openapi/v1/openapi.json`;
- additive Web API fragments: `openapi/v1/fragments/*.json`;
- profile/CRM protobuf: `proto/profile/v1/`;
- Bridge protobuf: `proto/bridge/v1/`;
- Rust contract constants: `crates/contracts`.

The directory/package major version is authoritative. A new incompatible surface
uses a new major root; it is not hidden behind an in-place v1 mutation.

The canonical current Web API document is the deterministic merge of the v1 root
and lexically sorted additive fragments. The root owns `openapi`, `info`, shared
security schemes and common components. A fragment may add only:

- new paths;
- new schemas;
- new parameters;
- new responses;
- new request bodies;
- new headers;
- new security schemes.

A fragment cannot replace any path or component already defined by the root or an
earlier fragment. Unknown top-level/component sections and collisions fail closed.
Use `python scripts/render-openapi.py` when a single materialized document is
required for client generation or inspection.

## 2. Stable Problem Taxonomy

Public errors use stable machine codes from `contracts::ProblemCode`:

- `not_found`;
- `forbidden`;
- `invalid_request`;
- `invalid_state`;
- `version_conflict`;
- `lease_conflict`;
- `replay_rejected`;
- `dependency_unavailable`;
- `integrity_failure`;
- `internal_failure`.

The legacy generic `conflict` code remains supported only where it is already part
of the current v1 surface. New endpoints should prefer the most specific stable
code that does not disclose a foreign or unauthorized resource.

Foreign and absent resources may intentionally map to the same `not_found`
response. Internal adapter/SDK errors never become public SDK-specific codes.
Human-readable detail is non-authoritative and must not contain secrets or PII.
An unknown internal code maps to the internal-failure problem type rather than a
misleading not-found type.

## 3. Compatible v1 Changes

Generally compatible:

- add an optional response field;
- add a new endpoint with a unique operation ID;
- add a uniquely named additive fragment path/component;
- add a protobuf message;
- add a protobuf field with a new number;
- add a new problem code when old clients can treat it generically;
- add a new capability that is default-deny.

Compatibility still requires implementation and negative authorization tests.

## 4. Breaking v1 Changes

Forbidden without a new major root or governed migration:

- remove or rename an existing API path/operation ID;
- replace an existing root/fragment path or component through a collision;
- remove an existing schema property required by the baseline;
- remove/rename a protobuf message;
- remove, rename or reuse a protobuf field number;
- change the protobuf package;
- reinterpret assignment as authorization;
- change an opaque ID to email/name/path semantics;
- expose provider/runtime SDK error types as public contracts.

## 5. Baseline And Gate

`contracts/baseline/` contains the accepted v1 compatibility floor. The permanent
quality gate:

1. deterministically merges the current root and additive fragments;
2. rejects malformed fragments, unknown sections and path/component collisions;
3. lints the merged OpenAPI/protobuf roots;
4. proves current contracts retain every baseline path/operation/message/field;
5. runs deliberately breaking fixtures and requires the checker to reject them.

The baseline is not automatically copied from current fragments. Changing the
baseline is a governed compatibility decision, not an automatic side effect of
editing current contracts. The PR must state why old clients are safe or introduce
a new major version/cutover plan.

## 6. Generated Code

Generated web/protobuf clients and rendered OpenAPI JSON are build artifacts and do
not own domain policy. Frontend, Bridge and future CRM adapters consume versioned
contracts; domain crates remain independent of HTTP, protobuf runtime and provider
SDK types.

## 7. Pre-2J B4 one-shot additive v1 authority

The Phase 2I release freeze remains the default during the active pre-2J
product-readiness remediation. Issue #211 is the separately governed versioning
decision required by the canonical remediation plan for Batch B4.

The accepted machine authority is
`architecture/pre2j-b4-contract-authority.json`. It permits **one** future
absent-to-added compatible v1 fragment and nothing else:

`openapi/v1/fragments/mailbox-client-association.json`

This is intentionally not a general v1 thaw and not a new major version. B4 needs
new unique mailbox-association operations; it does not reinterpret or break an
accepted v1 operation. Creating a `/api/v2` island only for this additive capability
would fragment the browser contract without a breaking-change requirement.

Permanent rules:

- the authority must first be accepted on the PR base before the fragment may be
  added; authority establishment and consumption cannot happen in one PR;
- once accepted, the machine-authority file is immutable;
- the allowlisted fragment may be added only while it is absent from the accepted
  base;
- after that fragment exists in accepted `main`, it is immutable under this
  exception and the one-shot authority is consumed;
- `openapi/v1/openapi.json` and every already-accepted v1 fragment remain
  byte-immutable during the exception;
- `contracts/baseline/**` and `proto/**` remain byte-immutable;
- the baseline is never rewritten merely to admit B4;
- ordinary compatibility lint/collision checks still apply to the new fragment;
- any breaking change still requires a new major root or another separately
  governed migration.

The B4 public surface must remain provider-neutral and must not make mailbox
association an ACL. Browser DTOs may expose opaque relationship/client/mailbox
identifiers and relationship version/state needed for bind, rebind and unbind, but
must not expose credentials, provider tokens, SDK objects, assignment-derived
permissions or raw protected mail data.

## 8. Pre-2J C2 one-shot additive v1 Gmail OAuth authority

Issue #217 is the separately governed versioning decision required before Batch C2
may expose a browser-visible Gmail OAuth onboarding ceremony. The accepted C1
provider-neutral lifecycle and existing Gmail execution/query adapters remain
separate authorities; this exception governs only the new browser protocol surface.

The machine authority is
`architecture/pre2j-c2-contract-authority.json`. Once that authority has first been
accepted on the PR base, it may permit exactly one absent-to-added compatible v1
fragment:

`openapi/v1/fragments/mailbox-gmail-oauth.json`

The authority PR itself must not add that fragment. The later C2 implementation PR
may consume the exception once; after accepted consumption the fragment becomes
immutable under the same release-freeze discipline.

C2 contract rules are deliberately narrower than a general OAuth or mailbox API
thaw:

- only authenticated Owner-facing initiation plus the fixed OAuth callback/result
  surface required to complete an exact C1 onboarding/re-authorization ceremony
  may be added;
- browser responses may contain a short-lived authorization URL and bounded
  machine-safe ceremony/result metadata, but never access/refresh tokens,
  authorization codes in response bodies, PKCE verifier material, OAuth client
  secret, raw mailbox credentials, provider SDK objects or secret-store internals;
- `MAILBOX_SECRET_RESOLVER` remains the single credential boundary. Google code
  exchange, refresh-token persistence/rotation and access-token refresh stay behind
  that boundary; C1/D1/domain-readable state receives only an opaque `SecretHandle`;
- callback/state handling must be short-lived, replay-safe and fail closed, and
  transient OAuth material must not enter D1, audit/outbox, browser storage,
  ordinary logs or mailbox association state;
- C2 requests only the Gmail read scope needed by the already accepted read/query
  capability. Future Gmail send consent belongs to C5 and must not be silently
  pre-granted by C2; later send consent is an explicit incremental capability;
- `openapi/v1/openapi.json`, every accepted v1 fragment including the consumed B4
  fragment, `contracts/baseline/**` and `proto/**` remain byte-immutable;
- ordinary compatibility/collision checks remain in force, and a breaking change
  still requires a new major root or another separately governed migration.

The release-freeze gate verifies the already consumed B4 authority/fragment
independently and gives the complete current `openapi/v1` diff to the active C2
one-shot checker. This prevents the historical B4 exception from accidentally
blocking a separately approved later exception without weakening immutability of
any accepted B4 artifact.

## 9. Pre-2J C3 one-shot additive v1 standards IMAP/SMTP authority

Issue #221 is the separately governed versioning decision required before Batch C3
may expose browser-visible standards-based IMAP/SMTP onboarding and credential
rotation. C1 remains the lifecycle/CAS authority and `MAILBOX_SECRET_RESOLVER`
remains the sole credential authority/storage boundary.

The machine authority is
`architecture/pre2j-c3-contract-authority.json`. Once that authority has first been
accepted on the PR base, one later C3 implementation PR may add exactly one absent
compatible v1 fragment:

`openapi/v1/fragments/mailbox-imap-smtp-onboarding.json`

The authority PR itself must not add that fragment. After accepted consumption the
fragment is immutable, just like the consumed B4 and C2 fragments.

C3 contract rules are deliberately capability- and authentication-explicit:

- the public surface must distinguish IMAP read/search readiness from SMTP send
  readiness; actual outbound message execution remains C6;
- supported transport configuration must make encrypted transport explicit and
  fail closed on plaintext. Accepted implementation modes are implicit TLS and
  STARTTLS, with bounded SSRF-safe targets;
- password authentication may be supported for standards servers that still accept
  it, but password mode must never be described as Outlook.com/Microsoft 365
  compatibility;
- Outlook.com/Microsoft 365 support under C3 means standards-protocol IMAP/SMTP with
  Microsoft Entra OAuth2 and SASL XOAUTH2, not Microsoft Graph;
- delegated Microsoft OAuth2 authority is limited to
  `https://outlook.office.com/IMAP.AccessAsUser.All`,
  `https://outlook.office.com/SMTP.Send`, and `offline_access`; Graph `Mail.*`
  permissions and Graph API claims are forbidden;
- raw passwords, OAuth authorization codes, PKCE material, client secrets,
  access/refresh tokens and SASL bearer material may cross only transient transport
  into the resolver boundary. They must not enter D1, audit/outbox, browser storage,
  ordinary logs, mailbox association state or ordinary domain-readable state;
- C3 may activate only exact `PENDING` or `REAUTH_REQUIRED` C1 onboarding versions.
  Successful provisioning returns only an opaque `SecretHandle` to C1; if C1 CAS
  fails after provisioning, the resolver-owned credential must be discarded or
  revoked;
- `openapi/v1/openapi.json`, every accepted v1 fragment including consumed B4/C2,
  `contracts/baseline/**`, and `proto/**` remain byte-immutable;
- ordinary compatibility/collision checks remain in force and any breaking change
  still requires a new major root or another separately governed migration.

The release-freeze gate validates consumed B4 and C2 authorities/fragments in
invariant-only mode and gives the complete current `openapi/v1` diff exclusively to
the active C3 one-shot checker. C3 itself also ships an invariant-only mode so a
future separately governed C4+ exception can preserve C3 immutability without
reusing C3 as a global diff owner.
