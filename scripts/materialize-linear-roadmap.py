#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PLAN = ROOT / "docs" / "DEVELOPMENT_PLAN.md"
MATRIX = ROOT / "docs" / "DEVELOPER_CAPABILITY_MATRIX.md"
INDEX = ROOT / "docs" / "INDEX.md"
FUTURE = ROOT / "docs" / "FUTURE_DEVELOPMENT.md"


def replace_between(text: str, start: str, end: str, replacement: str) -> str:
    if start not in text or end not in text:
        raise SystemExit(f"missing roadmap marker: {start!r} / {end!r}")
    before, rest = text.split(start, 1)
    _, after = rest.split(end, 1)
    return before + replacement + end + after


def replace_once(text: str, old: str, new: str) -> str:
    if text.count(old) != 1:
        raise SystemExit(f"expected one occurrence, found {text.count(old)}: {old[:100]!r}")
    return text.replace(old, new, 1)


plan = PLAN.read_text(encoding="utf-8")
plan = replace_once(
    plan,
    "**Tracking:** Phase 1A accepted via #114/#115; Phase 0 complete; next sequential slice Phase 2A; Phase 1B eligible dependency-independently; plan consolidation history #96",
    "**Tracking:** Phase 1A accepted via #114/#115; Phase 0 complete; Phase 1B #120 is the unique NEXT; Phase 2 starts only after Phase 1B acceptance; external CRM is future development only; plan consolidation history #96",
)

old_baseline = """**Phase 0 remains complete on accepted `main`, and Phase 1A is accepted.** The **next planned
sequential slice is Phase 2A — client aggregate and contact crypto foundation**, starting from the
accepted Phase 1A `main` in its own bounded issue/branch/PR. Phase 1B is eligible to proceed
only dependency-independently and must finish before real asynchronous provider/device execution
in Phases 4–5 and before Phase 6 realtime.
"""
new_baseline = """**Phase 0 remains complete on accepted `main`, and Phase 1A is accepted.** The active
implementation path is now deliberately linear: **Phase 1B is the unique NEXT** and Phase 2 does
not advance until Phase 1B is accepted and closed out on `main`. The already-created Phase 2A
issue #118/branch is queued only; it is not an active acceptance lane while Phase 1B is open.
External CRM integration is not an active phase and cannot block the standalone product path.
"""
plan = replace_once(plan, old_baseline, new_baseline)

old_policy = """- one sequential architecture/governance slice is active at a time when slices touch the same
  Worker/governed-write/application boundary;
- dependency-independent work may proceed in parallel only when it cannot create competing
  edits or invalidate the active slice baseline;
"""
new_policy = """- exactly one active implementation slice is on the product critical path at a time;
- the next implementation slice starts only after the previous slice is accepted, merged and its
  normative closeout advances the next marker;
- no dependency-independent product branch is used to bypass this sequence; operational collection
  of External evidence may continue outside the implementation path but never changes `NEXT`;
"""
plan = replace_once(plan, old_policy, new_policy)

phase_block = r'''## 5. Phase 1 — Durable Integration And Delivery Foundation

**Goal:** finish the durable asynchronous substrate completely before product expansion depends on it.
Phase 1 is infrastructure/application foundation. Phase 2 does not begin until Phase 1 is complete.

### Phase 1A — Durable event/outbox foundation — ACCEPTED

Accepted implementation:

- versioned integration event envelope;
- evolved `outbox_events` and metadata-only notification-event persistence;
- outbox dispatcher and Queue adapter;
- tenant/consumer/outbox idempotency;
- payload sanitizer enforcing the existing PII/secret/content policy;
- duplicate-delivery neutrality and canonical-source validation.

Acceptance used exact source head `21b4bc65cd1bb117504c0a0cfe18c8c11e411f25`, 12/12 permanent
workflows green, `behind_by=0`, zero blocking reviews/threads and guarded squash merge #115
`0186b780f7fed4b7c5e7f212c2fe437cbc46a5e5`.

### Phase 1B — Delivery hardening, catch-up and operations — NEXT

**Goal:** make the Phase 1A substrate operationally safe before any richer mailbox/device/realtime
product behavior is allowed to depend on it.

Implement in this order inside the bounded Phase 1B slice family:

1. durable `notification_deliveries` and `user_event_cursors` needed for delivery/catch-up state;
2. deterministic attempt accounting and next-attempt scheduling;
3. exponential backoff with bounded jitter and a configured maximum automatic attempt count;
4. terminal failure/DLQ lane with sanitized failure metadata only;
5. operator-safe remediation/replay with explicit idempotency and audit evidence;
6. authorized catch-up that applies tenant/live-membership/grant checks before event history exposure;
7. bounded retention/compaction for delivery/cursor state without deleting canonical business state;
8. sanitized operational visibility/alerts that never expose prohibited PII, secrets or mailbox bodies.

Phase 1B acceptance requires all of the following on one unchanged final head:

- poison messages reach terminal/DLQ state after the configured bound;
- retries cannot hot-loop and retry timing is deterministic under the accepted jitter contract;
- replay after remediation cannot duplicate the logical business effect;
- unauthorized/revoked actors cannot query catch-up/event history;
- disconnect/reconnect catch-up is durable rather than process-memory-only;
- Phase 1A event sanitation, canonical-source, transaction and duplicate-neutrality evidence remains green;
- permanent 12/12 workflows succeed, `behind_by=0`, diff is bounded, reviews/threads are zero,
  and guarded squash merge uses the exact accepted source SHA.

**Phase 1 completion gate:** Phase 1 is complete only after Phase 1B is accepted and its docs closeout
advances Phase 2A to `NEXT`. No Phase 2 implementation PR may merge before that gate.

## 6. Phase 2 — Expert Standalone Product Completion

**Goal:** turn the accepted platform foundation into a complete, secure, operator-usable standalone
product. Phase 2 is the entire remaining active product program. Every slice below is mandatory and
strictly sequential: **2A → 2B → 2C → 2D → 2E → 2F → 2G → 2H → 2I → 2J**.

A Phase 2 slice starts only after the immediately preceding slice is accepted and its normative
closeout advances the next marker. No later Phase 2 slice is an alternative or parallel lane.

### Phase 2A — Client aggregate and contact crypto foundation

Build the inward client/contact foundation first:

- extend the provider-neutral client aggregate/value model for `PERSON|ORGANIZATION`, lifecycle
  status and versioned metadata;
- typed contact-point identity/type/value contracts without using PII as technical identifiers;
- encrypted-at-rest contact display representation;
- tenant-keyed HMAC exact-lookup token contract;
- application-owned create/update/archive intent behind ports before D1/transport wiring;
- native + Workers-WASM proof before outer adapters.

Acceptance:

- plaintext contact values cannot cross the persistence port by type;
- no plaintext contact scan is required for exact lookup;
- no client name/email/phone/URL is used in IDs, paths, keys, metric labels or correlation IDs;
- Phase 1 durable mutation/audit/outbox invariants remain green.

### Phase 2B — Client persistence and lifecycle command path

Wire the 2A model into authoritative storage and application commands:

- additive D1 schema for encrypted contact values, key/version metadata and tenant-keyed lookup tokens;
- application-owned client update/archive lifecycle with checked aggregate versions;
- atomic canonical client mutation + idempotency + audit + outbox;
- contact encryption/token adapter behind the inward crypto port;
- tenant-scoped exact-contact lookup without plaintext scan;
- migration and negative tests proving raw contact persistence is rejected.

Acceptance:

- D1 never stores raw contact display values;
- wrong-tenant tokens cannot resolve another tenant's contact;
- failed crypto/persistence leaves no partial canonical mutation/audit/outbox state;
- create/update/archive replay is duplicate-neutral.

### Phase 2C — Client merge, assignment, grant-safe projections and Client Registry UI

Complete the business registry semantics before broad search:

- explicit `MERGED` lifecycle and deterministic source/target merge rules;
- historical `ProfileClientAssignment` reassignment: close previous active assignment, create next,
  emit audit/outbox, never grant access;
- one active primary client assignment per profile and multiple profiles per client;
- grant-filtered client/profile/assignment/activity projections;
- generated public contracts for the accepted new client surfaces;
- usable Client Registry UI for create/update/archive/merge/contact/assignment/grant projections.

Acceptance:

- merge cannot create identity/tenant ambiguity or resurrect archived state;
- assignment remains non-authorizing in application, D1 and UI evidence;
- revoked members lose projections without count/existence leakage;
- ordinary registry workflows are usable without CLI.

### Phase 2D — Read models, global search and client-mail query contract

Build explicit query boundaries after registry semantics are stable:

- CQRS-lite read-model ports/services for lists/search/detail projections;
- tenant/grant filtering before projection construction;
- bounded indexed global search for clients, profiles, permitted members and mailbox metadata;
- provider-neutral `SearchClientMailboxMessages` and `GetClientMailboxMessage` application contracts;
- deterministic fake provider/Bridge query adapters;
- incremental Client -> Mail UI against those contracts.

Mandatory query order:

```text
authenticate actor
  -> tenant + live membership/grants
  -> authorize client/mailbox context
  -> resolve eligible mailbox bindings
  -> provider/Bridge query adapter
  -> bounded result/body projection
```

Acceptance:

- no cross-tenant/result-count leakage;
- provider/body fetch is never called before authorization succeeds;
- foreign message references cannot bypass client/mailbox authorization;
- full synthetic body can be displayed without entering audit/events/logs/telemetry/browser storage.

### Phase 2E — Cloud mailbox provider lane

Implement the real cloud-capable provider path only after Phase 1 and the query contract are complete:

- cloud provider adapter contract implementations (Gmail API/IMAP as selected by product support);
- scheduled Queue execution using the accepted Phase 1 retry/DLQ/idempotency substrate;
- mailbox state/observation mutation + audit/outbox;
- provider-native subject/sender/recipient/body search where supported;
- bounded result mapping and selected full-body fetch;
- deterministic auth-required/suspended/failure transitions.

Acceptance:

- duplicate Queue delivery cannot duplicate logical results/counters/events;
- revoked/suspended bindings cannot execute;
- message content never enters ordinary audit/outbox/realtime payloads;
- provider integration evidence is clearly separated into repository-local versus External claims.

### Phase 2F — Durable device jobs and browser mailbox lane

Add providers that require an authorized browser profile:

- durable device job/request state and authenticated claim/result protocol;
- offline device -> explicit `PENDING_DEVICE`;
- profile contention -> explicit `PROFILE_BUSY`;
- claim/result idempotency, lease and fencing checks;
- current generation/certification validation before browser execution;
- Profile Bridge implementation of the same Phase 2D message query/body contract;
- stale device result rejection after claim turnover.

Acceptance:

- Bridge cannot claim another tenant/device job;
- offline/contended states are never reported as false success/empty result;
- stale results cannot overwrite a newer claim/generation;
- cloud and browser lanes satisfy one provider-neutral application contract.

### Phase 2G — Durable realtime notification hub

Build realtime only after durable retry/catch-up and business/query semantics exist:

```text
canonical state + outbox
  -> Queue dispatcher
  -> per-user UserNotificationHub Durable Object
  -> Hibernatable WebSocket
  -> React invalidation
  -> HTTPS refetch of canonical projection
```

Requirements:

- multiple tabs/devices;
- durable cursor catch-up after reconnect;
- bounded reauthorization and membership-revoke disconnect;
- no authoritative business state stored only in the WebSocket/DO process memory;
- no prohibited PII/secrets/message bodies in realtime envelopes;
- React treats realtime as invalidation/change signal and refetches canonical projections.

### Phase 2H — Complete standalone UI and administration UX

Finish all ordinary operator workflows without CLI:

1. Clients / Profiles;
2. Users & Access;
3. Client detail -> Mail search/result/body;
4. Mailboxes provider/binding/job administration;
5. Devices and sessions infrastructure administration;
6. Audit/settings/error/recovery surfaces.

Required route family remains:

```text
/
/clients
/clients/:clientId
/profiles
/profiles/:profileId
/users
/mailboxes
/sessions
/devices
/audit
/settings
```

UI acceptance:

- generated public contracts only for migrated public DTOs;
- strict sibling-feature boundaries;
- no optimistic success for governed mutations;
- explicit pending/offline/auth-required/profile-busy/retry/terminal states;
- sanitized/sandboxed HTML mail with remote active content disabled by default;
- no mailbox body or credential persistence in Web Storage/telemetry.

### Phase 2I — Standalone E2E, security, recovery and operational hardening

Prove the complete standalone product before production promotion:

- end-to-end owner/member/client/profile/mailbox/device/realtime workflows;
- grant/IDOR/revocation negative matrix;
- duplicate/replay/terminal failure/recovery scenarios;
- generation freshness/fencing/R2 failure and device turnover scenarios;
- backup/restore and disaster/recovery runbooks for D1/R2/DO/Bridge-owned state;
- bounded load/cost/performance tests for search, Queue, notification catch-up and UI-critical APIs;
- support/evidence bundles remain allowlist/sanitized;
- no uncontrolled plaintext PII/secret/message body in logs, audit, events or artifacts.

Acceptance requires one exact-head standalone release-candidate evidence set with all permanent
workflows green and no unresolved architecture/security gaps inside repository-owned scope.

### Phase 2J — Standalone production-readiness evidence and rollout

Close the final release gates for the standalone product. This is the only active phase slice that
can change `production_ready=false` after all required evidence is accepted.

Required evidence includes:

- isolated production Cloudflare resources, budgets and remote D1/R2/DO/Queue behavior;
- trusted Windows signing/update path;
- primary and secondary physical Windows evidence;
- production device-key protection/unwrap and recovery procedure;
- key escrow/restore drill;
- privacy/retention approval;
- product/license decisions;
- real provider/fingerprint certification for supported production lanes;
- remote backup/recovery/failure-order evidence;
- independent security/cryptographic review for applicable production cryptography;
- rollout/rollback/monitoring/runbook acceptance.

Phase 2J acceptance changes `production_ready` only when every mandatory External gate is backed by
real reviewable evidence. Missing evidence keeps `production_ready=false`; there is no code-only
shortcut.

## 7. Active Development Completion Gate

The active development roadmap is complete only when Phase 2J is accepted. At that point the
standalone application must be expert-grade and independently usable without any external CRM:

- Client Registry 2.0 and assignment/grant semantics complete;
- secure encrypted contact handling and exact lookup complete;
- grant-safe global search and client mailbox search/body viewing complete;
- cloud and browser mailbox lanes complete for supported providers;
- durable retries/DLQ/replay/catch-up complete;
- realtime durable-event-backed and non-authoritative;
- complete operator/admin UI usable without CLI;
- E2E/security/recovery/operational evidence accepted;
- production evidence accepted and `production_ready=true` only at this final gate.

External CRM integration is explicitly **not** a prerequisite for this completion gate.

'''
plan = replace_between(plan, "## 5. Phase 1 — Integration Events, Outbox And Notification Persistence", "## 15. Architecture Gates For Every Future PR", phase_block)

plan = replace_once(
    plan,
    "| CRM mapping | CRM adapter + versioned integration contract |",
    "| future CRM mapping | future-only CRM adapter + versioned integration contract; outside active Phase 1–2 execution |",
)

section17 = r'''## 17. Mandatory Sequential Execution Order

The active product path has no alternative or parallel implementation lane. The repository executes
exactly this order:

```text
Phase 0H -> 0I -> 0J -> 0K -> 0L -> 0M -> 0N        ACCEPTED
Phase 1A durable event/outbox foundation              ACCEPTED
Phase 1B delivery hardening/catch-up/operations       NEXT
Phase 2A client aggregate/contact crypto
Phase 2B client persistence/lifecycle
Phase 2C merge/assignment/grant-safe projections + Client Registry UI
Phase 2D read models/global search/client-mail query contract
Phase 2E cloud mailbox provider lane
Phase 2F durable device/browser mailbox lane
Phase 2G realtime notification hub
Phase 2H complete standalone UI/admin UX
Phase 2I standalone E2E/security/recovery/operations hardening
Phase 2J production-readiness evidence and rollout
```

Rules:

1. only the item marked `NEXT` is implementation-active;
2. every slice gets its own bounded issue/branch/draft PR;
3. the next slice starts only after exact-head acceptance, guarded merge and normative closeout of
   the previous slice;
4. no later Phase 2 branch is merged early because it appears dependency-independent;
5. External evidence collection may happen operationally, but it cannot change execution order or
   promote a repository capability claim before its owning slice;
6. external CRM integration is not part of this sequence and is documented only in
   `FUTURE_DEVELOPMENT.md`.

'''
plan = replace_between(plan, "## 17. Recommended PR Slicing", "## 18. Final Product Definition Of Done", section17)

section18 = r'''## 18. Standalone Product Definition Of Done

The active Phase 1–2 roadmap is complete only when the standalone application itself is complete;
CRM integration is not part of this definition.

Definition of done:

- clean application/adapter boundaries remain enforced in code and CI;
- Client Registry supports create/update/archive/merge, encrypted contacts, exact lookup,
  grants and historical profile assignment;
- client/profile/user/mailbox search is tenant/grant-safe and bounded/index-backed where required;
- authorized users can search a client's eligible mail and open the full body;
- mailbox content remains outside ordinary telemetry/audit/event payloads and browser storage;
- cloud and browser mailbox lanes share provider-neutral application contracts;
- durable jobs/retries/DLQ/replay/catch-up are operationally safe;
- realtime is durable-event-backed, revocation-aware and never authoritative by itself;
- complete UI/admin workflows work without CLI for ordinary operation;
- D1 remains authoritative for catalog/business metadata, DO for session/realtime coordination,
  R2 for immutable encrypted generation objects and Bridge for local runtime/materialization;
- stale devices/sessions cannot overwrite newer generations/claims;
- backup, restore, recovery, rollout and rollback procedures are proven at the required evidence level;
- required real provider/physical-host/security/privacy evidence is accepted;
- all permanent workflows are green on exact accepted heads and unresolved reviews/threads are zero;
- `production_ready=true` is allowed only after Phase 2J accepts every mandatory external gate.

A standalone release must not require an external CRM. CRM/Party authority integration is future
product evolution documented separately and can be evaluated only after this definition of done is met.

'''
plan = replace_between(plan, "## 18. Final Product Definition Of Done", "## 19. Immediate Next Action", section18)

section19 = r'''## 19. Immediate Next Action

Start **Phase 1B — delivery hardening, catch-up and operations** under issue #120 from the accepted
Phase 1A main. Phase 1B is the **only** active implementation slice.

Do not advance Phase 2A issue #118 or the existing `phase2a-client-contact-foundation` branch while
Phase 1B is unaccepted. Retain that branch only as queued work; rebase/review it after the Phase 1B
closeout marks Phase 2A `NEXT`.

Primary Phase 1B acceptance target:

```text
notification_deliveries + user_event_cursors
  -> deterministic attempt accounting
  -> bounded exponential backoff + jitter
  -> maximum automatic attempts
  -> terminal/DLQ failure lane
  -> operator-safe idempotent replay
  -> authorized durable catch-up
  -> bounded retention/compaction
  -> sanitized operational visibility
  -> Phase 1A invariants remain green
  -> exact-head permanent CI + guarded merge
```

After Phase 1B acceptance, proceed exactly through Phase 2A -> 2B -> 2C -> 2D -> 2E -> 2F -> 2G
-> 2H -> 2I -> 2J. No active Phase 3 exists. External CRM integration remains future development
only and must not enter the standalone critical path.

Keep `production_ready=false` until Phase 2J accepts all mandatory real external evidence.
'''
if "## 19. Immediate Next Action" not in plan:
    raise SystemExit("missing section 19")
plan = plan.split("## 19. Immediate Next Action", 1)[0] + section19
PLAN.write_text(plan, encoding="utf-8")

matrix = MATRIX.read_text(encoding="utf-8")
matrix = replace_once(
    matrix,
    "| Integration events / durable notifications | Composed / Synthetic | Accepted Phase 1A versioned envelope, evolved D1 outbox, metadata-only `notification_events`, tenant/consumer/outbox idempotency, sanitized Queue dispatch, thin scheduled/Queue ingress, canonical-source guards and deterministic duplicate/failure-order evidence. | Phase 1B retry/backoff/max-attempt/DLQ/catch-up/retention and real provider/device/realtime delivery remain Target/External. |",
    "| Integration events / durable notifications | Composed / Synthetic | Accepted Phase 1A versioned envelope, evolved D1 outbox, metadata-only `notification_events`, tenant/consumer/outbox idempotency, sanitized Queue dispatch, thin scheduled/Queue ingress, canonical-source guards and deterministic duplicate/failure-order evidence. | Phase 1B delivery hardening/catch-up/operations is the unique NEXT; retry/backoff/max-attempt/DLQ/cursors/retention remain Target until accepted. |",
)
matrix = replace_once(
    matrix,
    "| Client Registry 2.0 | Target | Target model/constraints defined in current plan. | Phase 2 implementation not accepted. |",
    "| Client Registry 2.0 | Target | Existing client create/query/grant/assignment baseline is accepted; the expert standalone registry target is defined by strict Phase 2A–2C sequencing. | Phase 2 is blocked until Phase 1B acceptance; encrypted contacts, lifecycle/merge and complete registry UI remain Target. |",
)
matrix = replace_once(
    matrix,
    "| Read models/global search | Target | CQRS-lite/query boundary and search targets defined. | Phase 3 implementation not accepted. |",
    "| Read models/global search | Target | CQRS-lite/query boundary and search targets defined. | Strict Phase 2D implementation is not accepted. |",
)
matrix = replace_once(
    matrix,
    "| Client-scoped mailbox message search/body | Target | Product/query/security contract is normative. | Provider-neutral query implementation and real provider/browser lanes are Phases 3–5. |",
    "| Client-scoped mailbox message search/body | Target | Product/query/security contract is normative. | Phase 2D defines the query contract; Phase 2E cloud and 2F browser execution remain Target. |",
)
matrix = replace_once(
    matrix,
    "| Realtime UserNotificationHub | Target | Durable-event-backed topology is normative. | Phase 6 implementation not accepted. |",
    "| Realtime UserNotificationHub | Target | Durable-event-backed topology is normative. | Strict Phase 2G implementation is not accepted. |",
)
matrix = replace_once(
    matrix,
    "| Complete standalone UI/E2E | Target | UI/acceptance target is normative. | Phases 7–8 implementation not accepted. |",
    "| Complete standalone UI/E2E | Target | UI/acceptance target is normative. | Strict Phase 2H–2I implementation is not accepted; Phase 2J closes production evidence/rollout. |",
)
matrix = replace_once(
    matrix,
    "| CRM integration | Target | Contract/event isolation, Party reference and replaceable-adapter direction are defined. | CRM Party projection/OIDC/PostgreSQL/cutover not accepted. |",
    "| External CRM integration | Target | Future-only contract-isolated Party/adapter direction is documented separately; it is not part of active product completion. | No CRM implementation is active; it can be considered only after standalone Phase 2J completion. |",
)
matrix = replace_once(
    matrix,
    "| Production readiness | External | Evidence intake/readiness interlocks exist. | Required external evidence remains incomplete; `production_ready=false`. |",
    "| Production readiness | External | Evidence intake/readiness interlocks exist; Phase 2J is the final standalone evidence/rollout gate. | Required external evidence remains incomplete; `production_ready=false` until Phase 2J acceptance. |",
)
old_order = "- the current execution plan, not this matrix, determines subsequent order; Phase 2A is the next sequential slice, while Phase 1B is eligible dependency-independently and remains required before real async provider/device and realtime execution."
new_order = "- the current execution plan, not this matrix, determines subsequent order; Phase 1B is the unique NEXT, Phase 2A–2J follow strictly in order after Phase 1B, and external CRM is future development only."
matrix = replace_once(matrix, old_order, new_order)
MATRIX.write_text(matrix, encoding="utf-8")

index = INDEX.read_text(encoding="utf-8")
future_section = """## Future Development — Not Active Execution\n\n- [`FUTURE_DEVELOPMENT.md`](./FUTURE_DEVELOPMENT.md) — post-standalone evolution only. External CRM/Party integration is explicitly outside the active Phase 1–2 critical path and can be considered only after Phase 2J production-readiness closure.\n\n"""
if "## Future Development — Not Active Execution" not in index:
    index = index.replace("## Historical / Design Baseline\n", future_section + "## Historical / Design Baseline\n")
index = replace_once(
    index,
    "4. **Historical rationale:** old roadmaps/plans; they never override current execution order.",
    "4. **Future development:** `FUTURE_DEVELOPMENT.md` records post-standalone ideas only; it never supplies `NEXT` and never overrides the active Phase 1–2 order.\n5. **Historical rationale:** old roadmaps/plans; they never override current execution order.",
)
INDEX.write_text(index, encoding="utf-8")

FUTURE.write_text(r'''# Future Development — External CRM Integration

**Status:** future product evolution; not active execution  
**Date:** 2026-08-08  
**Prerequisite:** standalone Phase 2J accepted with production-readiness evidence

## 1. Purpose

This document records product evolution that is deliberately **outside** the active Phase 1–2
execution plan. It is not a roadmap competing with `DEVELOPMENT_PLAN.md`, cannot contain `NEXT`, and
must not block completion or release of the standalone application.

The standalone Browser Profile Platform must reach its full product definition of done before any
external CRM becomes authoritative.

## 2. External CRM / Party Integration

External CRM integration is a future capability, not an active Phase 3.

Only after standalone Phase 2J acceptance may a new planning decision consider:

1. preserve the standalone opaque `client_id` as the platform resource identity;
2. introduce/verify an opaque `external_party_ref` rather than deriving IDs from CRM names/contacts;
3. consume versioned CRM Party/Customer projections/events through an isolated adapter;
4. reconcile/link standalone Client and CRM Party without changing profile/generation/session IDs;
5. prove parity before transferring authority for canonical name/contact/status fields;
6. keep profile assignments, browser profiles, generations, sessions, certification, mailbox runtime
   and Profile Bridge lifecycle owned by this platform;
7. after authority transfer, block conflicting local edits or translate explicit commands through the
   CRM adapter;
8. evaluate any PostgreSQL/SQLx + RLS replacement as a separate architecture migration;
9. evaluate any CRM OIDC identity replacement as a separate security/identity migration;
10. preserve R2 encrypted-generation and Profile Bridge lifecycle boundaries without CRM coupling.

## 3. Future Acceptance Principles

Any later CRM initiative must still satisfy:

- versioned contract/event isolation; no CRM SDK/table/entity imports in core domain/application code;
- async durable projections for synchronization, with synchronous HTTP only where a user command
  legitimately needs an immediate acknowledgement/result;
- tenant/authorization checks before projection/fetch;
- no raw PII in technical identifiers, logs, events or support evidence;
- no standalone feature regression and no requirement to migrate R2 generation objects merely to
  integrate CRM;
- its own issue/ADR/branch/PR/evidence plan created **after** Phase 2J, based on the then-current
  product requirements.

## 4. Explicit Non-Goals Of The Active Roadmap

The active Phase 1–2 roadmap does not implement:

- CRM Party authority/cutover;
- CRM OIDC migration;
- CRM-backed PostgreSQL migration;
- CRM-specific UI/workflows;
- any dependency that prevents the standalone application from operating independently.

Until a future post-Phase-2 decision is accepted, external CRM integration remains `Target`/future
only and has no effect on `production_ready` for the standalone product.
''', encoding="utf-8")

print("Linear Phase 1 -> Phase 2 roadmap materialized; Phase 1B is the unique NEXT; CRM is future-only.")
