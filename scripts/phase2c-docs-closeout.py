#!/usr/bin/env python3
from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one match, found {count}")
    return text.replace(old, new, 1)


plan = Path("docs/DEVELOPMENT_PLAN.md")
text = plan.read_text(encoding="utf-8")

changes = [
    (
        "tracking",
        "**Tracking:** Phase 1 complete; Phase 2A and Phase 2B accepted via #118/#137 and #138/#140; Phase 2C is the unique NEXT; expert-plan refinement #133; external CRM is future development only  ",
        "**Tracking:** Phase 1 complete; Phase 2A, Phase 2B and Phase 2C accepted via #118/#137, #138/#140 and #142/#143; Phase 2D is the unique NEXT; expert-plan refinement #133; external CRM is future development only  ",
    ),
    (
        "accepted baseline",
        "- Phase 2B authoritative protected client/contact persistence in D1, separate versioned\n  encryption and exact-lookup key domains, application-owned checked-version lifecycle/contact\n  commands, atomic mutation + idempotency + audit + outbox, and tenant-first indexed HMAC lookup.\n",
        "- Phase 2B authoritative protected client/contact persistence in D1, separate versioned\n  encryption and exact-lookup key domains, application-owned checked-version lifecycle/contact\n  commands, atomic mutation + idempotency + audit + outbox, and tenant-first indexed HMAC lookup.\n- Phase 2C deterministic one-way client merge, historical non-authorizing primary assignments,\n  grant-safe Client Registry projections, feature-owned SPA route composition, generated Client\n  Registry contracts and ordinary registry UI workflows without CLI.\n",
    ),
    (
        "accepted evidence list",
        "Phase 2B was accepted through issue #138 / PR #140 from exact proven source head\n`895594e35b77ddd86395300b1644e9df6a712123` and guarded squash merge\n`298062ea443c31c69212cb03b3988265b6bbcd48`.\n",
        "Phase 2B was accepted through issue #138 / PR #140 from exact proven source head\n`895594e35b77ddd86395300b1644e9df6a712123` and guarded squash merge\n`298062ea443c31c69212cb03b3988265b6bbcd48`.\n\nPhase 2C was accepted through issue #142 / PR #143 from exact proven source head\n`d3ad2e774a98ad5fed2565ba410ba9923062d170` and guarded squash merge\n`042d0dc72fa37e99f971d61d21544609a69c6e31`.\n",
    ),
    (
        "critical path marker",
        "Exactly one implementation slice is active at a time. Phase 2C is the unique NEXT after\nthe accepted Phase 2B closeout; later Phase 2 slices remain blocked by the same linear rule.",
        "Exactly one implementation slice is active at a time. Phase 2D is the unique NEXT after\nthe accepted Phase 2C closeout; later Phase 2 slices remain blocked by the same linear rule.",
    ),
    (
        "A4",
        "| A4 | Rust/OpenAPI/TypeScript generation | **Foundation accepted, coverage incomplete.** Migrated public slice is generated and CI-enforced; Phase 2B added no public transport DTO. | Every new 2C–2H public DTO/event expands canonical generated coverage before UI use. |",
        "| A4 | Rust/OpenAPI/TypeScript generation | **Expanded through Phase 2C.** Client Registry Rust/OpenAPI/TypeScript contracts are canonical, generated and drift-checked; malformed/duplicate extension fails closed. | Every new 2D–2H public DTO/event expands canonical generated coverage before UI use. |",
    ),
    (
        "A5",
        "| A5 | Feature-owned SPA route composition | **Open.** Current root router still owns direct feature imports/routes. | Mandatory first frontend architecture step in 2C before route-family growth. |",
        "| A5 | Feature-owned SPA route composition | **Accepted in Phase 2C.** Feature route definitions live behind public feature route modules; root composition imports feature route APIs rather than workspace internals; negative CI rejects bypasses. | Preserve for every later route family; no return to root-owned feature internals. |",
    ),
    (
        "6.4",
        "| 6.4 | Authorization-before-projection | **Composed for Phase 1B event catch-up.** Live membership/grants are applied before event projection; broader query/provider coverage remains open. | 2D read/search/provider query; 2G realtime subscriptions. |",
        "| 6.4 | Authorization-before-projection | **Accepted for Phase 1B catch-up and Phase 2C Client Registry projections.** Live membership/client grants are applied before client projection; member assignment history additionally requires profile grant; broader provider/global query coverage remains open. | 2D read/search/provider query; 2G realtime subscriptions. |",
    ),
    (
        "Phase 0 traceability",
        "Phase 0 completion does **not** mean A3, A5 or A8 were implemented. Those obligations are explicitly\nscheduled in Phase 2 at their required growth points.",
        "Phase 0 completion did **not** itself close A3, A5 or A8. The client half of A3 was accepted\nin Phase 2A and A5 in Phase 2C; A8 remains owned by Phase 2D and the mailbox half of A3 by Phase 2E.",
    ),
    (
        "Phase 1 status",
        "**Status:** ACCEPTED. Phase 1 is complete; Phase 2A and Phase 2B are accepted; Phase 2C is the unique next implementation slice.",
        "**Status:** ACCEPTED. Phase 1 is complete; Phase 2A, Phase 2B and Phase 2C are accepted; Phase 2D is the unique next implementation slice.",
    ),
    (
        "Phase 1 completion marker",
        "Phase 2A and Phase 2B are also accepted; Phase 2C is the unique NEXT and later Phase 2 slices remain\nblocked by the same linear rule.",
        "Phase 2A, Phase 2B and Phase 2C are also accepted; Phase 2D is the unique NEXT and later Phase 2 slices\nremain blocked by the same linear rule.",
    ),
    (
        "2C heading",
        "### Phase 2C — Client merge, assignment, grant-safe projections and modular Client Registry UI — NEXT",
        "### Phase 2C — Client merge, assignment, grant-safe projections and modular Client Registry UI — ACCEPTED",
    ),
    (
        "2C evidence",
        "**Purpose:** finish Client Registry business semantics and establish scalable frontend route\ncomposition before the SPA grows further.\n\n#### 2C execution order",
        "**Purpose:** finish Client Registry business semantics and establish scalable frontend route\ncomposition before the SPA grows further.\n\n**Accepted evidence:** issue #142 / PR #143; exact proven source head\n`d3ad2e774a98ad5fed2565ba410ba9923062d170`; guarded squash merge\n`042d0dc72fa37e99f971d61d21544609a69c6e31`; 12/12 permanent workflows green on the unchanged\nsource head; `behind_by=0`; reviews=0; unresolved threads=0. `production_ready=false` remains unchanged.\n\n#### 2C execution order",
    ),
    (
        "2D heading",
        "### Phase 2D — CQRS read models, global search and client-mail query contract",
        "### Phase 2D — CQRS read models, global search and client-mail query contract — NEXT",
    ),
]
for label, old, new in changes:
    text = replace_once(text, old, new, label)
if "Phase 2C is the unique NEXT" in text:
    raise SystemExit("stale Phase 2C NEXT marker remains")
plan.write_text(text, encoding="utf-8")

architecture = Path("docs/ARCHITECTURE.md")
text = architecture.read_text(encoding="utf-8")
changes = [
    (
        "projection ports",
        "Phase 2B accepts authoritative protected client-contact D1 persistence and a separate exact-lookup\nquery port. Exact lookup accepts tenant scope + contact kind + normalization version + versioned HMAC\ntoken only; D1 lookup adapters do not receive plaintext and do not decrypt/scan all contact rows.\n",
        "Phase 2B accepts authoritative protected client-contact D1 persistence and a separate exact-lookup\nquery port. Exact lookup accepts tenant scope + contact kind + normalization version + versioned HMAC\ntoken only; D1 lookup adapters do not receive plaintext and do not decrypt/scan all contact rows.\n\nPhase 2C accepts client-merge and Client Registry projection ports. Assignment is business/history\nlinkage only and never an authorization source; registry projections apply live membership/client\ngrants before construction, and member-visible assignment history additionally requires profile grant.\n",
    ),
    (
        "data ownership",
        "| ClientContactPoint | Client Registry | D1 ciphertext + nonce/key-version metadata + tenant-first HMAC lookup index; audit/outbox remain metadata-only |\n| Profile/Assignment | Profile Catalog | D1 + audit/outbox |",
        "| ClientContactPoint | Client Registry | D1 ciphertext + nonce/key-version metadata + tenant-first HMAC lookup index; audit/outbox remain metadata-only |\n| ClientMerge history | Client Registry | immutable governed D1 merge record + audit/outbox; source grants are removed and never transferred to the target |\n| Profile/Assignment | Profile Catalog | D1 + audit/outbox; assignment history is non-authorizing |",
    ),
    (
        "authorization",
        "Frontend filtering is never an authorization mechanism. Foreign/missing resources use the\naccepted neutral-disclosure behavior.",
        "Frontend filtering is never an authorization mechanism. Foreign/missing resources use the\naccepted neutral-disclosure behavior. Phase 2C Client Registry list/history projections enforce this\nbefore projection construction; profile-client assignment alone cannot make a client or profile visible.",
    ),
    (
        "frontend routes",
        "- sibling feature internals are not imported directly;\n- cross-feature composition uses shared/entities/app/routes or explicit feature public APIs;",
        "- sibling feature internals are not imported directly;\n- root route composition imports public feature route factories/APIs, never feature workspace internals;\n- cross-feature composition uses shared/entities/app/routes or explicit feature public APIs;",
    ),
    (
        "frontend acceptance",
        "Generated-contract drift and sibling-feature violations are permanent CI targets in Phase 0.",
        "Generated-contract drift and sibling-feature violations are permanent CI targets. Phase 2C accepts\nfeature-owned route composition and expands canonical generated contracts through the Client Registry surface.",
    ),
    (
        "CI enforcement",
        "- protected client-contact persistence and tenant-scoped exact-HMAC lookup positive/negative fixtures;\n- generation freshness/fencing;",
        "- protected client-contact persistence and tenant-scoped exact-HMAC lookup positive/negative fixtures;\n- client merge one-way/non-grant semantics, assignment-as-non-ACL and grant-safe projection fixtures;\n- feature-owned route composition and generated Client Registry contract drift;\n- generation freshness/fencing;",
    ),
]
for label, old, new in changes:
    text = replace_once(text, old, new, label)
architecture.write_text(text, encoding="utf-8")
