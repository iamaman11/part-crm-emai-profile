#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise SystemExit(f"expected exactly one anchor in {path}: {old[:120]!r}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


plan = ROOT / "docs" / "DEVELOPMENT_PLAN.md"
architecture = ROOT / "docs" / "ARCHITECTURE.md"
checker = ROOT / "scripts" / "generate-architecture-inventory.py"

replace_once(
    plan,
    """7. Integrate Profile Bridge materialization freshness check before writer launch.\n   - materialize the exact accepted generation into an isolated clone/workspace; never snapshot a live browser directory;\n   - treat browser identity/fingerprint configuration as versioned generation/profile-lineage metadata: launches reuse the accepted configuration, while changes require an explicit migration/re-certification path rather than implicit regeneration;\n   - pass proxy/network identity as an outer runtime policy with provider-neutral metadata; never encode an assumption that per-session IP rotation is universally safe;\n   - treat browser lock files as evidence of possible writer ownership: never delete `.parentlock`, `lock` or equivalent blindly; prove ownership/recovery state or return `PROFILE_BUSY`.\n8. Implement the exact Phase 2D search/get-message contract through the Bridge/browser adapter.\n9. Reject stale result after claim turnover, generation change or fencing advancement.\n10. Persist successful dirty browser state only through a new immutable encrypted generation: upload, verify, then fenced/CAS activation of the D1 active-generation pointer; never mutate the active R2 object or depend on cherry-picked provider cookie names. On network/R2 failure preserve dirty local state and route recovery through existing generation rules.\n""",
    """7. Integrate Profile Bridge materialization freshness and runtime-identity preflight before writer launch.\n   - materialize the exact accepted generation into an isolated clone/workspace; never snapshot a live browser directory;\n   - define a provider-neutral `BrowserIdentityManifest` that binds the accepted runtime bundle version/digest to the fingerprint source/configuration and compatibility policy; launches reuse that accepted manifest, while runtime/fingerprint changes require an explicit candidate-generation migration and re-certification path rather than implicit regeneration;\n   - do not freeze individual low-level signals such as User-Agent or transport/header details across an incompatible browser-runtime upgrade; compatibility is proven for the manifest/runtime pair, not assumed from copied values;\n   - define `NetworkIdentityPolicy` + `NetworkIdentityObservation` around the actual proxy egress used for the browser job: bounded country/region and timezone compatibility, required network class where applicable, optional allowlisted ASN/carrier constraints, and session stickiness only when the selected policy requires it; never assume per-session IP rotation is universally safe;\n   - classify network mismatch explicitly (for example retryable route churn versus operator-remediated policy mismatch) and fail closed before Camoufox launch when the observation does not satisfy the accepted policy;\n   - treat browser/workspace lock evidence through a fail-closed writer-recovery decision: combine the local workspace lease token/epoch, supervised native process identity and current coordinator lease/fencing evidence; PID alone is never sufficient ownership proof; any active or uncertain writer state returns `PROFILE_BUSY` or `RECOVERY_REQUIRED`;\n   - only after all ownership evidence is proven stale may recovery materialize a fresh isolated clone; never mutate the source generation or blindly delete `.parentlock`, `lock` or equivalent runtime lock files.\n8. Implement the exact Phase 2D search/get-message contract through the Bridge/browser adapter.\n9. Reject stale result after claim turnover, generation change or fencing advancement.\n10. Persist successful dirty browser state only through a new immutable encrypted generation: fully stop/supervise the writer, validate the candidate with bounded restore/inventory and policy-selected read-only store probes where useful, upload, verify, then fenced/CAS activation of the D1 active-generation pointer. A blanket `PRAGMA integrity_check` over every Firefox SQLite file is not a universal health/authority signal. Never mutate the active R2 object or depend on cherry-picked provider cookie names. On corruption or failed validation quarantine the candidate; rollback may target only a previously verified/policy-compatible generation. On network/R2 failure preserve dirty local state and route recovery through existing generation rules.\n""",
)

replace_once(
    plan,
    """- browser identity/fingerprint configuration cannot change implicitly between launches;\n- browser lock files cannot be blindly deleted to acquire writer ownership;\n- dirty browser mutations are not reported as persisted until immutable generation upload, verification and fenced/CAS activation succeed; failure preserves recoverable dirty state;\n""",
    """- browser identity/fingerprint configuration cannot change implicitly between launches; runtime upgrades use an explicit `BrowserIdentityManifest` compatibility/migration + re-certification path;\n- browser launch is blocked when `NetworkIdentityObservation` does not satisfy the accepted `NetworkIdentityPolicy`; policy may require bounded geo/timezone/network-class/ASN constraints without assuming universal mobile-IP rotation behavior;\n- writer recovery is fail closed: local lease token/epoch + supervised native process identity + coordinator fencing are reconciled, PID alone is insufficient, uncertain ownership is `PROFILE_BUSY`/`RECOVERY_REQUIRED`, and browser lock files are never blindly deleted;\n- recovery validation is bounded and policy-driven rather than treating blanket Firefox SQLite `PRAGMA integrity_check` as canonical health proof; invalid candidates are quarantined and rollback uses only verified compatible generations;\n- dirty browser mutations are not reported as persisted until immutable generation upload, verification and fenced/CAS activation succeed; failure preserves recoverable dirty state;\n""",
)

replace_once(
    architecture,
    """## 12. Key Hierarchy\n""",
    """### 11.1 Browser Runtime Identity, Network Policy And Writer Recovery\n\nBrowser execution is a Phase 2F outer/runtime concern, but its safety invariants are stable:\n\n- an accepted browser launch uses a versioned `BrowserIdentityManifest` binding the runtime bundle\n  version/digest to fingerprint source/configuration and compatibility policy; identity never changes\n  implicitly between launches, while runtime upgrades use an explicit candidate-generation migration\n  and re-certification path rather than blindly preserving or regenerating individual low-level values;\n- proxy/network behavior is represented by provider-neutral `NetworkIdentityPolicy` and observed through\n  the actual egress route used for the browser job. Policy may constrain country/region, timezone\n  compatibility, network class and optional ASN/carrier allowlists, and may require session stickiness;\n  no architecture rule assumes that per-session mobile-IP rotation is universally safe;\n- a browser writer is considered unavailable whenever local workspace lease/token/epoch, supervised\n  native process identity or coordinator lease/fencing evidence proves an active writer, or when those\n  signals cannot be reconciled confidently. PID alone is not ownership proof. Ambiguity fails closed as\n  `PROFILE_BUSY`/`RECOVERY_REQUIRED`; runtime lock files are never deleted merely to acquire ownership;\n- stale-writer recovery never mutates the source generation. It materializes a fresh isolated clone and\n  applies bounded restore/inventory checks plus policy-selected read-only store probes where useful. A\n  blanket Firefox SQLite `PRAGMA integrity_check` is not canonical profile-health authority; invalid\n  candidates are quarantined and rollback may target only a previously verified compatible generation.\n\nThese rules complement the immutable-generation saga: dirty browser state becomes durable only after\nwriter shutdown, candidate validation, immutable encrypted upload, verification and fenced/CAS\nactivation of the D1 active-generation pointer.\n\n## 12. Key Hierarchy\n""",
)

replace_once(
    checker,
    """    plan = (ROOT / \"docs\" / \"DEVELOPMENT_PLAN.md\").read_text(encoding=\"utf-8\")\n    matrix = (ROOT / \"docs\" / \"DEVELOPER_CAPABILITY_MATRIX.md\").read_text(encoding=\"utf-8\")\n""",
    """    plan = (ROOT / \"docs\" / \"DEVELOPMENT_PLAN.md\").read_text(encoding=\"utf-8\")\n    architecture = (ROOT / \"docs\" / \"ARCHITECTURE.md\").read_text(encoding=\"utf-8\")\n    matrix = (ROOT / \"docs\" / \"DEVELOPER_CAPABILITY_MATRIX.md\").read_text(encoding=\"utf-8\")\n""",
)

replace_once(
    checker,
    """        \"| 6.4 | Authorization-before-projection | **Accepted through Phase 2D query/read-model scope.**\",\n    )\n""",
    """        \"| 6.4 | Authorization-before-projection | **Accepted through Phase 2D query/read-model scope.**\",\n        \"`BrowserIdentityManifest`\",\n        \"`NetworkIdentityPolicy` + `NetworkIdentityObservation`\",\n        \"PID alone is never sufficient ownership proof\",\n        \"blanket `PRAGMA integrity_check`\",\n    )\n""",
)

replace_once(
    checker,
    """    stale_plan_markers = (\n""",
    """    required_architecture_markers = (\n        \"### 11.1 Browser Runtime Identity, Network Policy And Writer Recovery\",\n        \"`BrowserIdentityManifest`\",\n        \"`NetworkIdentityPolicy`\",\n        \"PID alone is not ownership proof\",\n        \"blanket Firefox SQLite `PRAGMA integrity_check` is not canonical profile-health authority\",\n    )\n    stale_plan_markers = (\n""",
)

replace_once(
    checker,
    """    for marker in stale_plan_markers:\n        if marker in plan:\n            raise SystemExit(f\"DEVELOPMENT_PLAN.md contains stale accepted-phase marker: {marker}\")\n""",
    """    for marker in stale_plan_markers:\n        if marker in plan:\n            raise SystemExit(f\"DEVELOPMENT_PLAN.md contains stale accepted-phase marker: {marker}\")\n    for marker in required_architecture_markers:\n        if marker not in architecture:\n            raise SystemExit(f\"ARCHITECTURE.md is missing browser-runtime safety marker: {marker}\")\n""",
)

print("Phase 2F runtime identity/recovery refinement materialized.")
