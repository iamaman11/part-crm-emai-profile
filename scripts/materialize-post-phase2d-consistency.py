#!/usr/bin/env python3
from __future__ import annotations

import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected exactly one match in {path}: found {count}\n--- OLD ---\n{old}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


plan = ROOT / "docs" / "DEVELOPMENT_PLAN.md"
replace_once(
    plan,
    "| A8 | Query-side/CQRS read-model boundary | **Open.** No accepted global read/search application context. | Phase 2D, before global search and provider message-body query execution. |",
    "| A8 | Query-side/CQRS read-model boundary | **Accepted in Phase 2D.** `use-cases-query`, capability-owned read-model ports/projections, bounded typed global search, grant-safe exact-contact lookup and provider-neutral Client Mail sequencing are permanently enforced. | Preserve in Phase 2E/2F provider implementations and later query surfaces; do not move query policy into adapters/UI. |",
)
replace_once(
    plan,
    "| 6.4 | Authorization-before-projection | **Composed for Phase 1B event catch-up.** Live membership/grants are applied before event projection; broader query/provider coverage remains open. | 2D read/search/provider query; 2G realtime subscriptions. |",
    "| 6.4 | Authorization-before-projection | **Accepted through Phase 2D query/read-model scope.** Live membership/grants precede list/search/detail projection, exact-contact lookup and mailbox eligibility/provider invocation; grant-sensitive D1 predicates recheck visibility where applicable. | Preserve in 2E/2F real provider lanes; 2G extends the same rule to realtime subscriptions. |",
)
replace_once(
    plan,
    "Phase 0 completion does **not** mean A3, A5 or A8 were implemented. Those obligations are explicitly\nscheduled in Phase 2 at their required growth points.",
    "Phase 0 completion did **not** by itself satisfy A3, A5 or A8. Their fixed growth-point work is now\ntracked by accepted evidence: the client half of A3 was accepted in 2A, A5 in 2C and A8 in 2D; only\nthe mailbox half of A3 remains open and is owned by 2E.",
)
replace_once(
    plan,
    "**Status:** ACCEPTED. Phase 1 is complete; Phase 2A, Phase 2B and Phase 2C are accepted; Phase 2D issue #144 is the unique next implementation slice.",
    "**Status:** ACCEPTED. Phase 1 is complete; Phase 2A, Phase 2B, Phase 2C and Phase 2D are accepted; Phase 2E issue #148 is the unique next implementation slice.",
)
replace_once(
    plan,
    "- cloud query implementation conforms to the exact 2D application contract.\n\n### Phase 2F — Durable device jobs, browser mailbox lane and materialization integration",
    "- cloud query implementation conforms to the exact 2D application contract.\n\nBrowser/Camoufox execution, device jobs, fingerprint/profile identity, proxy/network runtime policy,\nbrowser workspace locks and browser-driven generation evolution are explicit Phase 2F concerns and\nmust not leak into the 2E cloud lane.\n\n### Phase 2F — Durable device jobs, browser mailbox lane and materialization integration",
)
replace_once(
    plan,
    "7. Integrate Profile Bridge materialization freshness check before writer launch.\n8. Implement the exact Phase 2D search/get-message contract through the Bridge/browser adapter.\n9. Reject stale result after claim turnover, generation change or fencing advancement.\n10. Preserve dirty local state on network/R2 failure and route recovery through existing generation rules.\n11. Add multi-device/offline/contention/replay/recovery synthetic E2E evidence.",
    "7. Integrate Profile Bridge materialization freshness check before writer launch.\n   - materialize the exact accepted generation into an isolated clone/workspace; never snapshot a live browser directory;\n   - treat browser identity/fingerprint configuration as versioned generation/profile-lineage metadata: launches reuse the accepted configuration, while changes require an explicit migration/re-certification path rather than implicit regeneration;\n   - pass proxy/network identity as an outer runtime policy with provider-neutral metadata; never encode an assumption that per-session IP rotation is universally safe;\n   - treat browser lock files as evidence of possible writer ownership: never delete `.parentlock`, `lock` or equivalent blindly; prove ownership/recovery state or return `PROFILE_BUSY`.\n8. Implement the exact Phase 2D search/get-message contract through the Bridge/browser adapter.\n9. Reject stale result after claim turnover, generation change or fencing advancement.\n10. Persist successful dirty browser state only through a new immutable encrypted generation: upload, verify, then fenced/CAS activation of the D1 active-generation pointer; never mutate the active R2 object or depend on cherry-picked provider cookie names. On network/R2 failure preserve dirty local state and route recovery through existing generation rules.\n11. Add multi-device/offline/contention/replay/recovery synthetic E2E evidence.",
)
replace_once(
    plan,
    "- cloud and browser lanes satisfy one application query/job contract;\n- local materialization remains cache/workspace, not authority.",
    "- cloud and browser lanes satisfy one application query/job contract;\n- browser identity/fingerprint configuration cannot change implicitly between launches;\n- browser lock files cannot be blindly deleted to acquire writer ownership;\n- dirty browser mutations are not reported as persisted until immutable generation upload, verification and fenced/CAS activation succeed; failure preserves recoverable dirty state;\n- local materialization remains cache/workspace, not authority.",
)

matrix = ROOT / "docs" / "DEVELOPER_CAPABILITY_MATRIX.md"
replacements = [
    (
        "| Client Registry baseline | Composed | Current client create/query/grant/assignment metadata paths and D1 schema. Client create/query and grants are application-owned; assignment is non-authorizing. | Domain decomposition, encrypted contacts, richer lifecycle/merge and complete Registry UI are Phase 2A–2C Target. |",
        "| Client Registry baseline | Composed | Phase 2A–2C accepted decomposed `client-domain`, independent `use-cases-clients`, protected contacts, checked lifecycle/merge, historical non-authorizing assignment, grant-safe projections and modular Client Registry UI. | Phase 2H completes broader operator/admin UX; external CRM remains future-only. |",
    ),
    (
        "| Mailbox operations baseline | Composed / Synthetic | Provider-neutral binding/job domain, D1 persistence, secret-handle-only DTOs, idempotency/audit/outbox, Worker metadata/job paths and synthetic provider decisions. | Mailbox-domain decomposition, real cloud/browser execution and message search/body retrieval are Phase 2D–2F Target/External. |",
        "| Mailbox operations baseline | Composed / Synthetic | Provider-neutral binding/job domain, D1 persistence, secret-handle-only DTOs, idempotency/audit/outbox and Worker metadata/job paths are composed. Phase 2D adds the accepted provider-neutral Client Mail search/body application contract with synthetic cloud/Bridge adapters. | Mailbox-domain decomposition + real cloud lane are Phase 2E; durable browser/device lane is Phase 2F; real provider evidence remains External. |",
    ),
    (
        "| React web UI baseline | Composed / Synthetic | React/Vite/TS shell and current session/client/profile/ACL/assignment/generation/coordinator/mailbox/user surfaces. Migrated session/client/problem/mutation contracts are generated. Sibling-feature internal/alias imports are fail-closed. | Feature-owned route composition (A5), complete routes, full generated public DTO coverage, Client Mail and complete admin UX remain Phase 2C/2H Target. |",
        "| React web UI baseline | Composed / Synthetic | React/Vite/TS shell, Phase 2C feature-owned route composition and modular Client Registry UI are accepted; migrated public DTOs are generated, and Phase 2D adds generated Client Mail contracts plus incremental Client -> Mail UI. Sibling-feature internal/alias imports are fail-closed. | Phase 2H completes routes, full operator/admin UX, safe full-body mail rendering and remaining generated public coverage. |",
    ),
    (
        "| Client contact protection | Target | Data-classification and architecture contracts require encrypted display values and tenant-keyed exact lookup. | Actual client contact encryption/HMAC/key-version persistence is **not** implemented; Phase 2A–2B owns it. |",
        "| Client contact protection | Composed | Phase 2A/2B accepted versioned normalization, separate encryption/HMAC key domains, ciphertext-only authoritative D1 persistence, key-version-aware protection and tenant-first indexed exact lookup; Phase 2D reuses the HMAC index behind live authorization/grants. | Production key operations/restore remain External; fuzzy/prefix PII search remains prohibited without a separate ADR. |",
    ),
    (
        "| Client Registry 2.0 | Target | Expert standalone registry target and strict 2A–2C sequence are normative. | `client-domain` split, `use-cases-clients`, contacts, lifecycle/merge, projections and UI remain Target. |",
        "| Client Registry 2.0 | Composed | Phase 2A–2C accepted client-domain split, `use-cases-clients`, protected contacts, lifecycle/merge, grant-safe projections, historical assignment and ordinary Registry UI workflows. | Phase 2H completes cross-capability operator/admin polish; future CRM cutover remains outside active Phase 2. |",
    ),
    (
        "| Read models/global search | Target | CQRS/query/security target is normative. | `use-cases-query`, read-model ports/projections and global search are Phase 2D. |",
        "| Read models/global search | Library / Synthetic | Phase 2D accepted independent `use-cases-query`, capability-owned read-model ports/projections, bounded opaque-ID global search, grant-safe D1 predicates, cursor/cost bounds and query-plan evidence with permanent native/WASM CI. | Real provider-backed mailbox reads arrive through Phase 2E/2F; broader UX is Phase 2H. |",
    ),
    (
        "| Client-scoped mailbox message search/body | Target | Product/query/security contract is normative. | 2D defines query contract; 2E cloud and 2F browser implementations remain Target. |",
        "| Client-scoped mailbox message search/body | Library / Synthetic | Phase 2D accepted provider-neutral search/get-message contracts, authorization -> mailbox eligibility -> provider sequencing, foreign-reference protection, deterministic fake cloud/Bridge full-body adapters, generated DTOs and incremental UI. | Phase 2E implements the real cloud lane; Phase 2F implements the real browser/Bridge lane; provider/physical evidence remains External. |",
    ),
    (
        "| A3 | Domain aggregate splitting | **Open** — `client-domain` and `mailbox-domain` remain monolithic today. | Client half 2A; mailbox half 2E. |",
        "| A3 | Domain aggregate splitting | **Client half accepted in Phase 2A** — `client-domain` is decomposed behind a thin facade; `mailbox-domain` remains the open half. | Mailbox half is Phase 2E. |",
    ),
    (
        "| A5 | Feature-sliced SPA route composition | **Open** — root router still directly assembles feature workspaces. | Phase 2C before route-family expansion. |",
        "| A5 | Feature-sliced SPA route composition | **Accepted in Phase 2C** — feature-owned public route APIs compose into the root router and sibling internals are permanently rejected. | Preserve during later route-family expansion. |",
    ),
    (
        "| A8 | CQRS/read-model boundary | **Open**. | Phase 2D via independent `use-cases-query`. |",
        "| A8 | CQRS/read-model boundary | **Accepted in Phase 2D** — independent `use-cases-query`, capability-owned read projections and authorization-before-projection/provider sequencing are permanently enforced. | Preserve in 2E/2F real provider lanes and later query surfaces. |",
    ),
    (
        "| 6.4 | Authorization-before-projection | **Composed for Phase 1B catch-up** with live membership/grants before metadata projection. | 2D query/provider fetch; 2G realtime. |",
        "| 6.4 | Authorization-before-projection | **Accepted through Phase 2D query scope** — live membership/grants precede projection, exact-contact lookup, mailbox eligibility and provider/body invocation. | Preserve in 2E/2F real lanes; 2G extends the rule to realtime. |",
    ),
    (
        "| 6.5 | PII contact protection | **Open for client contacts**. Phase 1A sanitizer does not satisfy this requirement. | 2A protected-value/crypto boundary; 2B D1 encryption/HMAC/key rotation. |",
        "| 6.5 | PII contact protection | **Accepted through Phase 2B/2D** — protected D1 contacts, separate versioned encryption/HMAC domains and tenant-first exact lookup are accepted; query reuse remains grant-safe. | Preserve continuously; fuzzy/prefix PII indexing still requires a separate accepted ADR. |",
    ),
    (
        "crates/use-cases-notifications\n  independent notification dispatch, retry, replay, catch-up, retention and operations application context\n\ncrates/use-cases\n  remaining shared application contexts; notification ownership is fully extracted",
        "crates/use-cases-notifications\n  independent notification dispatch, retry, replay, catch-up, retention and operations application context\n\ncrates/use-cases-clients\n  independent Client Registry command/application context accepted in Phase 2A–2C\n\ncrates/use-cases-query\n  independent cross-capability read/search application context accepted in Phase 2D\n\ncrates/use-cases\n  remaining shared application contexts; notification/client/query ownership does not return to this compatibility surface",
    ),
    (
        "The notification extraction point was accepted in Phase 1B. Remaining fixed extraction points are\nnormative in `DEVELOPMENT_PLAN.md`: clients in 2A, query in 2D, mailboxes in 2E and devices in 2F.",
        "The notification, client and query extraction points were accepted in Phase 1B, Phase 2A and Phase 2D.\nRemaining fixed extraction points are normative in `DEVELOPMENT_PLAN.md`: mailboxes in 2E and devices\nin 2F.",
    ),
    (
        "Current accepted mailbox capability is metadata/job oriented. Full message search/body view remains\nTarget. The planned contract is client-scoped and authorizes before provider fetch; message content\nis permitted only in the authorized response/UI and prohibited from ordinary technical channels.",
        "Current accepted mailbox capability combines composed metadata/job paths with the Phase 2D Library/Synthetic\nclient-scoped message search/body contract. Authorization and mailbox eligibility precede provider/body\ninvocation; message content is permitted only in the authorized response/UI and prohibited from ordinary\ntechnical channels. Real cloud execution remains Phase 2E and real browser/Bridge execution Phase 2F.",
    ),
]
for old, new in replacements:
    replace_once(matrix, old, new)

inventory_generator = ROOT / "scripts" / "generate-architecture-inventory.py"
replace_once(
    inventory_generator,
    '''    {
        "name": "client-registry-api",
        "canonical_source": "crates/control-plane-contract/src/client_registry_api.rs",
        "openapi": "openapi/v1/fragments/client-registry.json",
        "typescript": "frontend/src/shared/api/generated/client-registry.ts",
        "generator": "scripts/generate-frontend-contracts.py",
    },
]''',
    '''    {
        "name": "client-registry-api",
        "canonical_source": "crates/control-plane-contract/src/client_registry_api.rs",
        "openapi": "openapi/v1/fragments/client-registry.json",
        "typescript": "frontend/src/shared/api/generated/client-registry.ts",
        "generator": "scripts/generate-frontend-contracts.py",
    },
    {
        "name": "query-mail-api",
        "canonical_source": "crates/control-plane-contract/src/bin/export_query_mail.rs",
        "openapi": "openapi/v1/fragments/query-mail.json",
        "typescript": "frontend/src/shared/api/generated/query-mail.ts",
        "generator": "scripts/generate-frontend-contracts.py",
    },
]''',
)
replace_once(
    inventory_generator,
    '''def validate_docs() -> None:
    index = (ROOT / "docs" / "INDEX.md").read_text(encoding="utf-8")
    missing_links = [value for value in REQUIRED_INDEX_LINKS if value not in index]
    if missing_links:
        raise SystemExit(f"docs/INDEX.md is missing authority links: {missing_links}")

    plan = (ROOT / "docs" / "DEVELOPMENT_PLAN.md").read_text(encoding="utf-8")
    next_sections = re.findall(r"^### (Phase [^\\n]+?) — NEXT\\s*$", plan, re.MULTILINE)
    if len(next_sections) != 1:
        raise SystemExit(f"DEVELOPMENT_PLAN.md must have exactly one Phase ... — NEXT section: {next_sections}")
    immediate = plan.split("## 19. Immediate Next Action", 1)
    if len(immediate) != 2 or next_sections[0].split(" — ", 1)[0] not in immediate[1]:
        raise SystemExit("Immediate Next Action is inconsistent with the unique NEXT phase")

    status = json.loads((ROOT / "docs" / "status.json").read_text(encoding="utf-8"))
    if status.get("production_ready") is not False:
        raise SystemExit("docs/status.json must remain production_ready=false until external gates pass")
    if "`production_ready=false`" not in plan:
        raise SystemExit("DEVELOPMENT_PLAN.md must preserve the production_ready=false claim")
''',
    '''def validate_docs() -> None:
    index = (ROOT / "docs" / "INDEX.md").read_text(encoding="utf-8")
    missing_links = [value for value in REQUIRED_INDEX_LINKS if value not in index]
    if missing_links:
        raise SystemExit(f"docs/INDEX.md is missing authority links: {missing_links}")

    plan = (ROOT / "docs" / "DEVELOPMENT_PLAN.md").read_text(encoding="utf-8")
    matrix = (ROOT / "docs" / "DEVELOPER_CAPABILITY_MATRIX.md").read_text(encoding="utf-8")
    next_sections = re.findall(r"^### (Phase [^\\n]+?) — NEXT\\s*$", plan, re.MULTILINE)
    if len(next_sections) != 1:
        raise SystemExit(f"DEVELOPMENT_PLAN.md must have exactly one Phase ... — NEXT section: {next_sections}")
    immediate = plan.split("## 19. Immediate Next Action", 1)
    if len(immediate) != 2 or next_sections[0].split(" — ", 1)[0] not in immediate[1]:
        raise SystemExit("Immediate Next Action is inconsistent with the unique NEXT phase")

    required_plan_markers = (
        "Phase 2D — CQRS read models, global search and client-mail query contract — ACCEPTED",
        "Phase 2E — Mailbox domain decomposition and real cloud mailbox lane — NEXT",
        "Phase 2E issue #148 is the unique NEXT",
        "| A8 | Query-side/CQRS read-model boundary | **Accepted in Phase 2D.**",
        "| 6.4 | Authorization-before-projection | **Accepted through Phase 2D query/read-model scope.**",
    )
    stale_plan_markers = (
        "Phase 2D issue #144 is the unique next implementation slice",
        "| A8 | Query-side/CQRS read-model boundary | **Open.**",
        "2D read/search/provider query; 2G realtime subscriptions",
    )
    required_matrix_markers = (
        "| Client contact protection | Composed |",
        "| Client Registry 2.0 | Composed |",
        "| Read models/global search | Library / Synthetic |",
        "| Client-scoped mailbox message search/body | Library / Synthetic |",
        "| A3 | Domain aggregate splitting | **Client half accepted in Phase 2A**",
        "| A5 | Feature-sliced SPA route composition | **Accepted in Phase 2C**",
        "| A8 | CQRS/read-model boundary | **Accepted in Phase 2D**",
        "| 6.5 | PII contact protection | **Accepted through Phase 2B/2D**",
        "crates/use-cases-query",
    )
    stale_matrix_markers = (
        "| Client contact protection | Target |",
        "| Client Registry 2.0 | Target |",
        "| Read models/global search | Target |",
        "| A5 | Feature-sliced SPA route composition | **Open**",
        "| A8 | CQRS/read-model boundary | **Open**",
        "| 6.5 | PII contact protection | **Open for client contacts**",
    )
    for marker in required_plan_markers:
        if marker not in plan:
            raise SystemExit(f"DEVELOPMENT_PLAN.md is missing accepted-phase semantic marker: {marker}")
    for marker in stale_plan_markers:
        if marker in plan:
            raise SystemExit(f"DEVELOPMENT_PLAN.md contains stale accepted-phase marker: {marker}")
    for marker in required_matrix_markers:
        if marker not in matrix:
            raise SystemExit(f"DEVELOPER_CAPABILITY_MATRIX.md is missing accepted capability marker: {marker}")
    for marker in stale_matrix_markers:
        if marker in matrix:
            raise SystemExit(f"DEVELOPER_CAPABILITY_MATRIX.md contains stale capability marker: {marker}")

    for contract in GENERATED_CONTRACTS:
        for key in ("canonical_source", "openapi", "typescript", "generator"):
            relative_path = contract[key]
            if not (ROOT / relative_path).is_file():
                raise SystemExit(f"generated contract {contract['name']} references missing {key}: {relative_path}")

    status = json.loads((ROOT / "docs" / "status.json").read_text(encoding="utf-8"))
    if status.get("production_ready") is not False:
        raise SystemExit("docs/status.json must remain production_ready=false until external gates pass")
    if "`production_ready=false`" not in plan:
        raise SystemExit("DEVELOPMENT_PLAN.md must preserve the production_ready=false claim")
''',
)

subprocess.run(
    ["python", "scripts/generate-architecture-inventory.py", "--write"],
    cwd=ROOT,
    check=True,
)
