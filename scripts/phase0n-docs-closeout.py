#!/usr/bin/env python3
from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected exactly one replacement target, found {count}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8", newline="\n")


plan = "docs/DEVELOPMENT_PLAN.md"
replace_once(
    plan,
    "**Tracking:** Phase 0M accepted via #106/#107; next planned Phase 0N; plan consolidation history #96  ",
    "**Tracking:** Phase 0N accepted via #110/#111; Phase 0 complete; next planned Phase 1A; plan consolidation history #96",
)
replace_once(
    plan,
    """Repository Steps 0–10 and the accepted post-composition slices through **Phase 0M** provide
the current code baseline: typed domain/application boundaries, governed D1 writes,
profile generations, application-thin coordinator ingress, the first real application Cargo
boundary (`use-cases-identity`), synthetic Bridge/runtime lanes, mailbox metadata/jobs, React
composition, deterministic generated public frontend contracts, enforced frontend feature
boundaries and exact-head cross-component acceptance.""",
    """Repository Steps 0–10 and the accepted post-composition slices through **Phase 0N** provide
the current code baseline: typed domain/application boundaries, governed D1 writes,
profile generations, application-thin coordinator ingress, the first real application Cargo
boundary (`use-cases-identity`), synthetic Bridge/runtime lanes, mailbox metadata/jobs, React
composition, deterministic generated public frontend contracts, enforced frontend feature
boundaries, capability-owned fail-closed route classification, a deterministic machine-readable
architecture inventory and exact-head cross-component acceptance.""",
)
replace_once(
    plan,
    """No sequential Phase 0 implementation slice is active in this closeout. The **next planned
slice is Phase 0N — route classifier, architecture inventory and documentation consistency**,
which must start from the accepted Phase 0M `main` in its own bounded issue/branch/PR.""",
    """Phase 0N was accepted through issue **#110** / PR **#111** with guarded squash merge
`851a3b928fcd7b806f32cc32e2684ca5307d0114` from exact proven source head
`a2a5892daa5a8625e125e619c1f2d9944f567ebe`. Public `RouteClass` and Worker dispatch remained
stable while route matching moved into capability-owned classifiers behind one composed
fail-closed entrypoint. Unknown `/api/*`, `/auth/*` and `/bridge/*` variants cannot reach SPA
assets. `architecture/inventory.json` is deterministically derived/checkable for workspace
members, contiguous D1 migrations, route/classifier ownership, generated public contracts and
documentation authority; stale/tampered/missing inventory and selected documentation drift are
permanently rejected by preflight and CI.

**Phase 0 is complete on accepted `main`.** No sequential Phase 0 implementation slice remains.
The **next planned slice is Phase 1A — durable event/outbox foundation**, which must start from
the accepted Phase 0N `main` in its own bounded issue/branch/PR. Phase 1B remains later delivery
hardening and must not be folded into the initial 1A foundation slice.""",
)
replace_once(
    plan,
    """### Phase 0N — Route classifier, architecture inventory and documentation consistency — NEXT

Finish the remaining DX/convergence items:

- split fail-closed route classification by capability while retaining one composed
  classifier;
- unknown `/api/*`, `/auth/*`, `/bridge/*` versions/methods never fall through to SPA;
- machine-readable inventory for workspace crates, migrations, public routes and generated
  contracts;
- documentation consistency checks for machine-verifiable claims;
- keep `docs/INDEX.md` current and avoid parallel roadmaps.""",
    """### Phase 0N — Route classifier, architecture inventory and documentation consistency — ACCEPTED

Accepted implementation:

- public `RouteClass` and Worker dispatch remain stable while route matching is split into
  capability-owned `foundation`, `identity`, `clients`, `profiles`, `generations` and `mailboxes`
  classifier modules behind one composed `classify_route` entrypoint;
- composition remains fail closed: unknown versions/routes/wrong methods under `/api/*` and
  `/auth/*` resolve to dynamic-not-found, while `/bridge` and `/bridge/*` remain denied by default;
  these namespaces cannot fall through to static SPA assets;
- `architecture/inventory.json` is committed deterministic machine-readable evidence for Cargo
  workspace members, contiguous D1 migrations, public route/classifier ownership, generated public
  contracts and documentation authority;
- `scripts/generate-architecture-inventory.py --check` derives/checks repository truth and rejects
  missing paths, route ownership drift, multiple/misaligned `NEXT` documentation claims and
  production-readiness claim drift;
- a real negative harness proves stale, tampered and missing inventory are rejected;
- fast preflight plus permanent Quality and Repository Quality gates enforce inventory/docs
  consistency, and `docs/INDEX.md` indexes the machine-readable inventory without adding a roadmap;
- acceptance used exact source head `a2a5892daa5a8625e125e619c1f2d9944f567ebe`, 12/12 permanent
  workflows green, `behind_by=0`, zero blocking reviews/threads and guarded squash merge #111.""",
)
replace_once(
    plan,
    "### Phase 1A — Durable event/outbox foundation\n",
    "### Phase 1A — Durable event/outbox foundation — NEXT\n",
)
replace_once(
    plan,
    """Start **Phase 0N — route classifier, architecture inventory and documentation consistency** from
the accepted Phase 0M `main`. Create a fresh bounded issue/branch/PR before implementation; do
not fold 0N work into the completed #106/#107 history.

Primary 0N acceptance target:

```text
capability-owned route classifiers
  -> one composed fail-closed classifier
  -> unknown /api/*, /auth/* and /bridge/* versions/methods never reach SPA
  -> machine-readable workspace/migration/route/generated-contract inventory
  -> CI consistency checks for machine-verifiable documentation claims
  -> docs/INDEX.md remains current; no parallel normative roadmap
  -> exact-head permanent CI + guarded merge
```

Keep the accepted `use-cases-identity` and Phase 0M generated-contract/feature-boundary rules
intact, add no speculative Cargo splitting without real dependency/growth pressure, and continue
long-lead External gate preparation without changing `production_ready=false`.""",
    """Start **Phase 1A — durable event/outbox foundation** from the accepted Phase 0N `main`.
Create a fresh bounded issue/branch/PR before implementation; do not fold Phase 1A work into the
completed #110/#111 history or mix Phase 1B delivery hardening into the foundation slice.

Primary Phase 1A acceptance target:

```text
versioned integration event envelope
  -> evolved durable outbox + minimal notification-event persistence
  -> canonical mutation + audit/outbox atomicity within the D1 boundary
  -> outbox dispatcher + Queue adapter
  -> idempotent consumer registry / consumer_idempotency
  -> payload sanitizer rejects prohibited PII, secrets and mailbox bodies
  -> duplicate delivery has no duplicate logical effect
  -> replay-safe accepted consumer set
  -> exact-head permanent CI + guarded merge
```

Keep all accepted Phase 0 boundaries, generated-contract/feature-boundary rules and architecture
inventory checks intact. Continue long-lead External gate preparation in parallel without changing
`production_ready=false`; real provider/physical-host evidence remains External.""",
)

matrix = "docs/DEVELOPER_CAPABILITY_MATRIX.md"
replace_once(
    matrix,
    """| Cross-component standalone acceptance | Composed / Synthetic | Metadata-only deterministic manifest/validator covering governed D1, generation integrity, Worker/adapters native+WASM, synthetic Bridge and frontend tests/build. Permanent lanes enforce thin identity/client/profile/coordinator Worker boundaries while retaining assignment-as-ACL negative evidence. | Real deployment/provider/device evidence is External. |""",
    """| Cross-component standalone acceptance | Composed / Synthetic | Metadata-only deterministic manifest/validator covering governed D1, generation integrity, Worker/adapters native+WASM, synthetic Bridge and frontend tests/build. Permanent lanes enforce thin identity/client/profile/coordinator Worker boundaries, assignment-as-ACL negative evidence, capability-owned fail-closed route composition and deterministic architecture-inventory consistency. | Real deployment/provider/device evidence is External. |""",
)
replace_once(
    matrix,
    """Accepted Phase 0 slices through **0M** establish application ownership for client create/query/grant,
profile create/query/assignment/grant, mailbox binding/job, generation, identity governance/
ceremonies and coordinator ingress, plus the first real compile-time application Cargo boundary
and generated frontend contract/feature-boundary enforcement.""",
    """Accepted Phase 0 slices through **0N** establish application ownership for client create/query/grant,
profile create/query/assignment/grant, mailbox binding/job, generation, identity governance/
ceremonies and coordinator ingress, plus the first real compile-time application Cargo boundary,
generated frontend contract/feature-boundary enforcement, modular fail-closed route ownership and
a deterministic machine-readable architecture/docs inventory. Phase 0 is complete on accepted
`main`.""",
)
replace_once(
    matrix,
    """- accepted Phase 0M uses `control-plane-contract` as the canonical migrated public Rust transport source, commits deterministic OpenAPI/TypeScript output, consumes generated types on real frontend API surfaces, and permanently rejects sibling-feature internals plus resolver-alias bypasses;
- the current execution plan, not this matrix, determines subsequent order; Phase 0N is next planned.""",
    """- accepted Phase 0M uses `control-plane-contract` as the canonical migrated public Rust transport source, commits deterministic OpenAPI/TypeScript output, consumes generated types on real frontend API surfaces, and permanently rejects sibling-feature internals plus resolver-alias bypasses;
- accepted Phase 0N splits route matching into capability-owned classifiers behind one composed fail-closed entrypoint, prevents unknown `/api/*`, `/auth/*` and `/bridge/*` variants from reaching SPA assets, and permanently verifies deterministic `architecture/inventory.json` plus selected documentation consistency claims;
- the current execution plan, not this matrix, determines subsequent order; Phase 1A is next planned while integration events/durable notifications remain Target until that implementation is accepted.""",
)
replace_once(
    matrix,
    """crates/control-plane-contract
  accepted canonical public control-plane transport contract and deterministic OpenAPI export""",
    """crates/control-plane-contract
  accepted canonical public control-plane transport contract, deterministic OpenAPI export and capability-owned fail-closed route classifiers behind one composed entrypoint""",
)

print("Phase 0N accepted-main documentation closeout materialized")
