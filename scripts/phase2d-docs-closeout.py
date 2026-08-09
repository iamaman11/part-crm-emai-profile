#!/usr/bin/env python3
from pathlib import Path


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one exact anchor, found {count}: {old!r}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


plan = Path("docs/DEVELOPMENT_PLAN.md")
replace_once(
    plan,
    "**Tracking:** Phase 1 complete; Phase 2A/2B/2C accepted via #118/#137, #138/#140 and #142/#143; Phase 2D issue #144 is the unique NEXT; expert-plan refinement #133; external CRM is future development only",
    "**Tracking:** Phase 1 complete; Phase 2A/2B/2C/2D accepted via #118/#137, #138/#140, #142/#143 and #144/#147; Phase 2E issue #148 is the unique NEXT; expert-plan refinement #133; external CRM is future development only",
)
replace_once(
    plan,
    """- Phase 2B authoritative protected client/contact persistence in D1, separate versioned
  encryption and exact-lookup key domains, application-owned checked-version lifecycle/contact
  commands, atomic mutation + idempotency + audit + outbox, and tenant-first indexed HMAC lookup.

Phase 1A was accepted""",
    """- Phase 2B authoritative protected client/contact persistence in D1, separate versioned
  encryption and exact-lookup key domains, application-owned checked-version lifecycle/contact
  commands, atomic mutation + idempotency + audit + outbox, and tenant-first indexed HMAC lookup;
- Phase 2C deterministic client merge, historical non-authorizing primary assignment, grant-safe
  Client Registry projections, feature-owned route composition and generated Client Registry contracts;
- Phase 2D independent `use-cases-query`, capability-owned read projections, bounded typed global
  search, grant-safe exact-contact HMAC lookup, provider-neutral Client Mail query contracts,
  deterministic cloud/Bridge query adapters and permanent query/privacy enforcement.

Phase 1A was accepted""",
)
replace_once(
    plan,
    """Phase 2B was accepted through issue #138 / PR #140 from exact proven source head
`895594e35b77ddd86395300b1644e9df6a712123` and guarded squash merge
`298062ea443c31c69212cb03b3988265b6bbcd48`.

The critical path is deliberately linear:""",
    """Phase 2B was accepted through issue #138 / PR #140 from exact proven source head
`895594e35b77ddd86395300b1644e9df6a712123` and guarded squash merge
`298062ea443c31c69212cb03b3988265b6bbcd48`.

Phase 2C was accepted through issue #142 / PR #143 from exact proven source head
`d3ad2e774a98ad5fed2565ba410ba9923062d170` and guarded squash merge
`042d0dc72fa37e99f971d61d21544609a69c6e31`.

Phase 2D was accepted through issue #144 / PR #147 from exact proven source head
`ad491e2f0c9ba9f79130923fdde6fe1407af4dc5` and guarded squash merge
`26f8fa82bdad02a5a0867b0d36748b915579ef1c`.

The critical path is deliberately linear:""",
)
replace_once(
    plan,
    """Exactly one implementation slice is active at a time. Phase 2D issue #144 is the unique NEXT after
the accepted Phase 2C closeout; later Phase 2 slices remain blocked by the same linear rule.""",
    """Exactly one implementation slice is active at a time. Phase 2E issue #148 is the unique NEXT after
the accepted Phase 2D closeout; later Phase 2 slices remain blocked by the same linear rule.""",
)
replace_once(
    plan,
    "### Phase 2D — CQRS read models, global search and client-mail query contract — NEXT",
    "### Phase 2D — CQRS read models, global search and client-mail query contract — ACCEPTED",
)
replace_once(
    plan,
    """- synthetic full message body never enters logs/audit/events/telemetry/Web Storage;
- fuzzy/prefix PII search remains absent unless a separate ADR is accepted.

### Phase 2E — Mailbox domain decomposition and real cloud mailbox lane""",
    """- synthetic full message body never enters logs/audit/events/telemetry/Web Storage;
- fuzzy/prefix PII search remains absent unless a separate ADR is accepted.

#### Phase 2D acceptance evidence

Phase 2D was accepted through issue #144 / implementation PR #147 from exact proven source head
`ad491e2f0c9ba9f79130923fdde6fe1407af4dc5` and guarded squash merge
`26f8fa82bdad02a5a0867b0d36748b915579ef1c`. The unchanged source head passed exactly 12/12
permanent workflows with `behind_by=0`, reviews=0 and unresolved review threads=0. Accepted scope
includes independent `use-cases-query`, capability-owned grant-safe read projections, bounded typed
opaque-ID global search, Phase 2B HMAC-index exact-contact lookup with live grants, provider-neutral
Client Mail search/body contracts, authorization-before-eligibility/provider sequencing, deterministic
fake cloud/Bridge full-body adapters, indexed query-plan evidence, Rust-derived generated mail
contracts, incremental Client -> Mail UI and permanent Phase 2D privacy/authorization enforcement.
`production_ready=false` remains intentional.

### Phase 2E — Mailbox domain decomposition and real cloud mailbox lane — NEXT""",
)
replace_once(
    plan,
    """Phase 2B encrypted contact persistence + client lifecycle commands                 NEXT
Phase 2C merge/assignment/projections + feature-owned routes + Client Registry UI
Phase 2D use-cases-query + CQRS read models + global/client-mail query contracts
Phase 2E mailbox-domain split + use-cases-mailboxes + cloud provider lane""",
    """Phase 2B encrypted contact persistence + client lifecycle commands                 ACCEPTED
Phase 2C merge/assignment/projections + feature-owned routes + Client Registry UI                   ACCEPTED
Phase 2D use-cases-query + CQRS read models + global/client-mail query contracts                     ACCEPTED
Phase 2E mailbox-domain split + use-cases-mailboxes + cloud provider lane                            NEXT""",
)
text = plan.read_text(encoding="utf-8")
heading = "## 19. Immediate Next Action\n"
if text.count(heading) != 1:
    raise SystemExit("DEVELOPMENT_PLAN: Immediate Next Action heading must occur exactly once")
prefix = text.split(heading, 1)[0]
plan.write_text(
    prefix
    + """## 19. Immediate Next Action

Start **Phase 2E — mailbox domain decomposition and real cloud mailbox lane** under issue #148
from the accepted Phase 2D implementation merge `26f8fa82bdad02a5a0867b0d36748b915579ef1c` only after
this docs-only closeout is accepted on `main`.

Execute Phase 2E inward-first in this exact order:

```text
mailbox-domain decomposition (binding/job/runtime_lane/observation)
  -> independent use-cases-mailboxes
  -> provider-neutral cloud job state extensions
  -> real approved Gmail API/IMAP outer adapters
  -> Phase 1B retry/DLQ/idempotency scheduling
  -> metadata-only provider observations + canonical mutation/audit/outbox
  -> accepted Phase 2D search/get-message contract on the cloud lane
  -> credential-handle auth-required/suspended lifecycle
  -> provider failure/rate-limit/backpressure taxonomy + bounded metrics
  -> deterministic repository evidence separated from real External evidence
```

No Phase 2F+ implementation starts before Phase 2E is accepted and closed out. Mailbox content remains
excluded from audit/outbox/realtime/metrics, fuzzy/prefix PII search remains prohibited without a
separate accepted ADR, and `production_ready=false` remains unchanged.
""",
    encoding="utf-8",
)

architecture = Path("docs/ARCHITECTURE.md")
replace_once(
    architecture,
    """Phase 2C accepts deterministic one-way client merge and historical primary assignment semantics,
application-owned merge/reassignment sequencing, governed atomic D1 merge/history, and grant-safe
Client Registry read projections. Assignment remains explicitly non-authorizing; source grants are
removed rather than transferred on merge; revoked access disappears before projection construction.

The current plan may keep `application-ports` as one Cargo crate with capability modules while""",
    """Phase 2C accepts deterministic one-way client merge and historical primary assignment semantics,
application-owned merge/reassignment sequencing, governed atomic D1 merge/history, and grant-safe
Client Registry read projections. Assignment remains explicitly non-authorizing; source grants are
removed rather than transferred on merge; revoked access disappears before projection construction.

Phase 2D accepts capability-owned client/profile/member/mailbox/mail read-model ports, stable read
projections, typed opaque-ID global search, grant-safe exact-contact lookup through the existing Phase
2B HMAC index, and provider-neutral Client Mail search/body contracts. Live membership/grants are
checked before projection construction and again in grant-sensitive D1 predicates where applicable;
mail provider/body access is sequenced only after authorization and mailbox eligibility. Fuzzy/prefix
PII discovery, result-count leakage, secret-handle projection and assignment-derived authorization
remain prohibited.

The current plan may keep `application-ports` as one Cargo crate with capability modules while""",
)
replace_once(
    architecture,
    """Accepted independent application ownership includes `use-cases-identity`,
`use-cases-notifications` and `use-cases-clients`; shared `use-cases` compatibility re-exports do
not regain canonical ownership of those capabilities.

Phase 2D adds `use-cases-query` as the independent cross-capability read/search application context;
mutation aggregates remain owned by their existing capability use cases.""",
    """Accepted independent application ownership includes `use-cases-identity`,
`use-cases-notifications`, `use-cases-clients` and `use-cases-query`; shared `use-cases` compatibility
re-exports do not regain canonical ownership of those capabilities.

Phase 2D accepts `use-cases-query` as the independent cross-capability read/search application context;
mutation aggregates remain owned by their existing capability use cases. Query orchestration owns
authorization-before-projection, bounded exact-contact HMAC derivation/lookup, and authorization ->
mailbox eligibility -> provider/body sequencing without importing provider/runtime implementations.""",
)
replace_once(
    architecture,
    """- Phase 2C client merge/assignment-as-non-ACL/grant-safe projection and feature-route positive/negative fixtures;
- generation freshness/fencing;""",
    """- Phase 2C client merge/assignment-as-non-ACL/grant-safe projection and feature-route positive/negative fixtures;
- Phase 2D query ownership/privacy, exact-HMAC contact/grant safety, bounded/index-backed query-plan,
  synthetic cloud/Bridge Client Mail and native/WASM positive/negative fixtures;
- generation freshness/fencing;""",
)
replace_once(
    architecture,
    """  use-cases-identity/
  use-cases-clients/
  use-cases-profiles/
  use-cases-mailboxes/
  # later: notifications/search/devices/crm projection as justified""",
    """  use-cases-identity/
  use-cases-clients/
  use-cases-query/
  use-cases-profiles/
  use-cases-mailboxes/
  # later: devices/crm projection as justified""",
)
