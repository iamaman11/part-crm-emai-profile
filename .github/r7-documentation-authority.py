import json
from pathlib import Path

root = Path('.')


def write(path: str, content: str) -> None:
    target = root / path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(content, encoding='utf-8')


def must_replace(path: str, old: str, new: str, expected: int = 1) -> None:
    target = root / path
    text = target.read_text(encoding='utf-8')
    count = text.count(old)
    if count != expected:
        raise SystemExit(f'{path}: expected {expected} occurrences of {old!r}, found {count}')
    target.write_text(text.replace(old, new, expected), encoding='utf-8')


write('README.md', '''# Browser Profile Platform

Browser Profile Platform is a standalone, provider-neutral control plane for governed browser-profile,
client, mailbox, device and notification workflows. The Rust control plane, Cloudflare adapters,
Windows Profile Bridge and React operator UI are developed as one product with explicit authority and
privacy boundaries.

## Current state

- **Accepted repository-local product phase: Phase 2I.** The immutable acceptance ledger is
  [`architecture/accepted-phases.json`](architecture/accepted-phases.json).
- **Pre-2J remediation: ACTIVE / BLOCKING PHASE 2J.** The active repository-owned closeout is
  [`docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md`](docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md).
- **Phase 2J has not started.** Real production evidence and controlled rollout remain blocked until
  the pre-2J closure rule is satisfied on accepted `main`.
- **Production readiness:** `production_ready=false`. The machine-readable projection is
  [`docs/status.json`](docs/status.json).

Repository Steps 0–10 are historical delivery history, not the current implementation queue. Their
accepted evidence remains in [`docs/DELIVERY_ROADMAP.md`](docs/DELIVERY_ROADMAP.md) and
[`docs/evidence/`](docs/evidence/).

## Current authority

Start with [`docs/INDEX.md`](docs/INDEX.md). The main current sources are:

- [`docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md`](docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md) —
  temporary pre-2J blocker and closure rule while its status is ACTIVE;
- [`docs/DEVELOPMENT_PLAN.md`](docs/DEVELOPMENT_PLAN.md) — normative product phase plan;
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) + accepted ADRs — stable architecture invariants;
- [`docs/DEVELOPER_CAPABILITY_MATRIX.md`](docs/DEVELOPER_CAPABILITY_MATRIX.md) — accepted capability
  and evidence level on `main`;
- [`docs/status.json`](docs/status.json) — machine-readable current/readiness projection;
- [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md) — canonical current repository-local threat model;
- [`architecture/accepted-phases.json`](architecture/accepted-phases.json) — immutable accepted phase
  provenance.

If prose conflicts, follow the hierarchy in `docs/INDEX.md`; do not infer acceptance from an open PR,
historical evidence file or phase-specific closeout document.

## Architecture snapshot

- pure Rust domain/application crates own business rules and provider-neutral use cases;
- capability-specific application ownership is separated (`use-cases-clients`, `use-cases-identity`,
  `use-cases-mailboxes`, `use-cases-devices`, `use-cases-notifications`, `use-cases-query`), while the
  remaining shared `use-cases` crate owns current Profile/Generation/coordinator orchestration;
- Cloudflare D1/R2/Queue/Durable Object and provider mechanics stay in outer adapters/composition;
- the React SPA uses feature-owned routes and feature-owned capability API modules; shared API code is
  transport/generated-contract infrastructure only;
- governed public DTOs are Rust-owned and generated deterministically to OpenAPI/TypeScript;
- authorization, neutral disclosure, idempotency, fencing, privacy and recovery boundaries are backed
  by permanent positive and negative CI evidence.

## Fast local verification

```bash
python scripts/verify-fast.py
python scripts/verify-fast.py --with-compile
```

Full acceptance still requires all permanent GitHub workflows to succeed on one unchanged exact PR
head, zero blocking reviews/unresolved threads, `behind_by=0` and a guarded squash merge.

## Development and security

- Contributor workflow: [`CONTRIBUTING.md`](CONTRIBUTING.md)
- Security policy: [`SECURITY.md`](SECURITY.md)
- Product intent: [`docs/PRODUCT.md`](docs/PRODUCT.md)
- Future CRM boundary: [`docs/FUTURE_DEVELOPMENT.md`](docs/FUTURE_DEVELOPMENT.md)

Phase 2J External evidence cannot be substituted with repository-local/synthetic proof and cannot set
`production_ready=true` before every mandatory external acceptance gate is reviewed and accepted.
''')

write('docs/README.md', '''# Documentation Navigation

This file is a compatibility navigation entrypoint. The canonical documentation governance and
current-authority hierarchy live in [`docs/INDEX.md`](INDEX.md).

## Current state

- **Accepted repository-local product phase: Phase 2I.** Acceptance provenance:
  [`../architecture/accepted-phases.json`](../architecture/accepted-phases.json).
- **Pre-2J remediation: ACTIVE / BLOCKING PHASE 2J.** Active blocker/closure rule:
  [`PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md`](PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md).
- **Phase 2J is not implementation-active while that plan is ACTIVE.**
- **Production readiness:** `production_ready=false`; machine-readable projection:
  [`status.json`](status.json).

Repository Steps 0–10 are historical delivery history. Use [`DELIVERY_ROADMAP.md`](DELIVERY_ROADMAP.md)
and [`evidence/`](evidence/) only for historical provenance, not to decide current implementation order.

## Current normative sources

- [`INDEX.md`](INDEX.md) — documentation authority/governance;
- [`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md) — product phase order and acceptance rules;
- [`PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md`](PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md) — active temporary
  pre-2J blocker while status is ACTIVE;
- [`ARCHITECTURE.md`](ARCHITECTURE.md) and accepted ADRs — architecture invariants;
- [`DATA_CLASSIFICATION.md`](DATA_CLASSIFICATION.md) — data/privacy classes;
- [`UI_ARCHITECTURE.md`](UI_ARCHITECTURE.md) — standalone UI target;
- [`DEVELOPER_CAPABILITY_MATRIX.md`](DEVELOPER_CAPABILITY_MATRIX.md) — accepted capability/evidence level;
- [`status.json`](status.json) — current machine-readable readiness projection;
- [`THREAT_MODEL.md`](THREAT_MODEL.md) — canonical current security threat model;
- [`../architecture/accepted-phases.json`](../architecture/accepted-phases.json) — immutable accepted phase ledger.

Phase-specific threat/release/closeout documents, including
[`PHASE2I_THREAT_MODEL.md`](PHASE2I_THREAT_MODEL.md), are accepted evidence/history. They do not replace
`THREAT_MODEL.md` as the current security authority.

For contributor commands and exact-head acceptance discipline see [`../CONTRIBUTING.md`](../CONTRIBUTING.md).
''')

write('docs/INDEX.md', '''# Documentation Authority Index

This index defines which repository documents are normative **now** and which files are historical or
evidence-only. It is intentionally small so current implementation and security truth do not drift
across multiple roadmaps.

## Current repository state

- Accepted repository-local product phase: **Phase 2I**, proven by
  [`../architecture/accepted-phases.json`](../architecture/accepted-phases.json).
- Phase 2J remains blocked while the pre-2J remediation plan is ACTIVE.
- Machine-readable readiness authority: [`status.json`](status.json), with `production_ready=false`.
- Canonical current repository-local security authority: [`THREAT_MODEL.md`](THREAT_MODEL.md).

## Authority hierarchy

1. While [`PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md`](PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md) is
   **ACTIVE / BLOCKING PHASE 2J**, it is the temporary execution blocker and closure rule. It may stop
   the next product phase but does not redefine product scope.
2. [`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md) defines product phase order, ownership and acceptance.
3. [`ARCHITECTURE.md`](ARCHITECTURE.md), accepted ADRs and [`DATA_CLASSIFICATION.md`](DATA_CLASSIFICATION.md)
   define stable architecture/security/privacy invariants.
4. [`UI_ARCHITECTURE.md`](UI_ARCHITECTURE.md) defines the standalone UI target.
5. [`DEVELOPER_CAPABILITY_MATRIX.md`](DEVELOPER_CAPABILITY_MATRIX.md) records what is accepted on `main`
   and at which evidence level.
6. [`../architecture/accepted-phases.json`](../architecture/accepted-phases.json) is the immutable phase
   provenance ledger; [`status.json`](status.json) is the current machine-readable projection.
7. [`THREAT_MODEL.md`](THREAT_MODEL.md) is the canonical current threat model. Phase-specific threat
   documents are accepted evidence/history only.

If these sources disagree, implementation stops and the authority documents are corrected before work
continues. An open branch/PR never outranks accepted `main`.

## Current architecture and capability references

- [`ARCHITECTURE.md`](ARCHITECTURE.md)
- [`DEVELOPER_CAPABILITY_MATRIX.md`](DEVELOPER_CAPABILITY_MATRIX.md)
- [`REALTIME_NOTIFICATIONS.md`](REALTIME_NOTIFICATIONS.md)
- [`PROFILE_GENERATION_REGISTRY.md`](PROFILE_GENERATION_REGISTRY.md)
- [`UI_ARCHITECTURE.md`](UI_ARCHITECTURE.md)
- [`THREAT_MODEL.md`](THREAT_MODEL.md)
- [`TEST_EVIDENCE_INDEX.md`](TEST_EVIDENCE_INDEX.md)

## Historical and evidence context

- [`DELIVERY_ROADMAP.md`](DELIVERY_ROADMAP.md) — historical Repository Steps 0–10.
- [`evidence/`](evidence/) — immutable/bounded acceptance evidence.
- [`PHASE2I_THREAT_MODEL.md`](PHASE2I_THREAT_MODEL.md) — Historical accepted Phase 2I evidence; current
  threat authority is `THREAT_MODEL.md`.
- Phase-specific governance/closeout/runbook files preserve the evidence and reasoning of their owning
  phase; they do not become a second current roadmap.

Future CRM/Party work remains future-only in [`FUTURE_DEVELOPMENT.md`](FUTURE_DEVELOPMENT.md) until the
standalone product passes Phase 2J.
''')

write('docs/THREAT_MODEL.md', '''# Threat Model

**Status:** Canonical current repository-local threat model; Phase 2I accepted repository-local controls  
**Production readiness:** `production_ready=false`; Phase 2J External residual risks remain unaccepted  
**Method:** trust-boundary and STRIDE-oriented analysis

## 1. Protected assets

- browser profile generations: cookies, login databases, localStorage, IndexedDB and materialized state;
- profile entropy/fingerprint/network identity policy and observations;
- mailbox, proxy and OAuth secret handles;
- root wrapping keys, tenant KEK, generation DEK and device private keys;
- memberships, grants, client/contact records and historical profile assignments;
- launch intents, durable jobs, leases, fencing tokens, sessions and realtime cursors;
- runtime/Bridge installers, signatures, update metadata and immutable generation objects;
- audit, support, operational evidence, backup and recovery artifacts.

## 2. Trust boundaries

1. Browser user -> Cloudflare Access -> verified application actor.
2. Verified actor -> live tenant membership/capability/grant authorization.
3. React SPA -> Rust Control Plane Worker API; UI state is never authorization authority.
4. Worker application orchestration -> D1/R2/Queue/Durable Objects through outer adapters.
5. Mailbox application ports -> cloud provider adapters or browser/device execution lane.
6. Durable device job/claim -> Windows Profile Bridge device identity and fencing context.
7. Bridge -> local workspace/SQLite outbox -> embedded browser runtime process.
8. Immutable encrypted generation -> exact verification -> authoritative catalog activation.
9. Durable notification/realtime event -> current authorization -> metadata-only invalidation -> refetch.
10. Operators -> Cloudflare account, signing material, recovery escrow and production rollout controls.
11. Future CRM -> versioned contracts only; never direct profile/mailbox authority.

## 3. Phase 2I accepted repository-local controls

| Threat | Accepted repository-local control | Permanent evidence class |
|---|---|---|
| Cross-tenant / IDOR access | live membership/capability/grant checks before projection/provider/device/realtime access; neutral denial | identity/query/application boundary gates and cross-component acceptance |
| Result-count / existence disclosure | foreign and absent resources are public-response neutral; denied paths return no foreign projections/counts | query/transport negative fixtures |
| Revoked actor receives realtime data | current authorization before catch-up/live delivery; durable cursor semantics | Phase 2G notification/realtime policy/tests |
| Realtime becomes business authority | metadata-only invalidation followed by authorized refetch; no direct business query mutation | frontend realtime policy/self-tests |
| Duplicate/replayed command | idempotency receipts, replay neutrality and atomic governed mutation envelopes | D1/application/mailbox/device tests |
| Concurrent or stale writer | expected-version CAS, coordinator/device/generation fencing and single-writer ownership | coordinator/device/generation tests |
| Unverified/corrupt generation becomes active | immutable candidate, exact verification, quarantine/fail-closed parsing, then activation | profile-generation/encrypted-generation/R2 gates |
| Failed remote commit destroys recoverable local state | retained dirty/operator-owned state until verified remote commit | Bridge/materialization recovery tests |
| Provider outage/auth expiry reported as success | explicit retry/auth-required/suspended/failed durable states | mailbox application failure tests |
| Offline/busy device reported as success | durable retry/remediation state and bounded claims; no false completion | device domain/application tests |
| Corrupt backup/restore | point-in-time restore plus schema/data/integrity validation | Phase 2I recovery/DR drills |
| Sensitive/high-cardinality telemetry | metadata/class-only dimensions and explicit forbidden identifier/content classes | operational-bounds negative policy |
| Sensitive support evidence | allowlist-only support fields and sanitizer/forbidden-data policy | support-bundle negative policy |
| Dependency/CI source substitution | exact dependency locks, approved sources and SHA-pinned permanent actions | supply-chain/license/runtime policies |
| Malicious archive/path escape | safe paths, streaming/bounded extraction and deterministic inventory | runtime bundle/materialization gates |
| Browser/runtime command abuse | typed bounded IPC/capability allowlists; no generic privileged command channel | Bridge/runtime contract gates |

## 4. Fail-closed rules

- Unknown membership, grant, device, runtime, mailbox or generation state denies access.
- Foreign and absent resources produce indistinguishable public denial behavior.
- Authorization precedes projection, provider, device and realtime access.
- Unverified/corrupt/quarantined generation cannot become authoritative.
- Dirty or recovery-required local state is not silently evicted or overwritten.
- Expired/stale fencing, claim, generation or session state cannot write newer authority.
- Missing key/recovery evidence quarantines data rather than guessing.
- Signature/update verification failure preserves the previous accepted runtime.
- Confidential mail input stays in request bodies; sanitized mail HTML remains sandboxed and non-networked.
- Technical telemetry/support/evidence never carries raw PII, secrets, mailbox content or unbounded IDs.

## 5. Phase 2J External residual risks

Repository-local Phase 2I evidence does **not** prove production Cloudflare behavior, real mailbox-provider
behavior, real Camoufox/fingerprint behavior, physical multi-device recovery, production device-key
protection, trusted Windows signing/update, remote R2/key recovery, offline escrow restore, independent
cryptographic review, production privacy/retention approval or operational rollout/on-call readiness.

Cloudflare account compromise remains high impact; D1 has no PostgreSQL-style RLS defense in depth; an
authorized compromised endpoint/device can observe plaintext while a profile is in active use; provider
and fingerprint behavior changes independently. These risks are accepted only through the applicable
real Phase 2J evidence/review, never by relabelling synthetic tests.

## 6. Security authority and review gates

This file is the canonical current threat model. [`PHASE2I_THREAT_MODEL.md`](PHASE2I_THREAT_MODEL.md)
is Historical accepted Phase 2I evidence and remains useful provenance, but it does not override this
model.

Update this model whenever a trust boundary, cryptographic protocol, identity provider, operating-system
lane, mailbox provider, tenant model or future CRM adapter changes. Production promotion requires the
Phase 2J evidence matrix and immutable reviewed evidence for all mandatory external security/recovery
controls; until then `production_ready=false`.
''')

write('docs/PHASE2I_THREAT_MODEL.md', '''# Phase 2I Repository-Local Threat Model

**Historical accepted Phase 2I evidence.**  
**Canonical current threat model: [THREAT_MODEL.md](THREAT_MODEL.md).**  
Status: ACCEPTED for repository-local Phase 2I scope. Production readiness remains false pending Phase 2J External evidence.

## Trust boundaries

The accepted Phase 2I system separated browser UI, Control Plane Worker, D1 catalog, R2 immutable
generations, Durable Object coordination, mailbox providers, device execution and the local Profile
Bridge. Application/domain layers remained provider-agnostic; adapters owned provider mechanics.

## Threat and control matrix

| Threat | Required control | Repository-local evidence |
|---|---|---|
| Cross-tenant or IDOR access | live membership/capability/grant checks before projection or provider access; neutral denial | identity ACL, query boundary and Phase 2I hardening gates |
| Result-count disclosure | denied paths return no foreign projection/count information | query application and projection tests |
| Revoked actor receives realtime data | current authorization before catch-up/live delivery; durable cursor semantics | Phase 2G realtime policy and notification tests |
| Realtime becomes business authority | metadata-only invalidation followed by authorized refetch | frontend realtime policy and self-tests |
| Duplicate/replayed commands | idempotency/replay receipts and transaction boundaries | D1 command/mailbox tests and cross-component acceptance |
| Concurrent/stale writer | fencing, expected-version compare-and-swap and single-writer coordination | coordinator, device and generation tests |
| Unverified generation activation | immutable candidate, exact verification, then authoritative activation | generation registry, encrypted-generation and R2 boundary tests |
| Failed generation commit destroys recovery state | retained dirty local state until verified remote commit | Profile Bridge retained-operator tests |
| Provider outage or authentication expiry appears successful | explicit retry/auth/suspended/failed states | mailbox application failure tests |
| Offline/busy device appears successful | durable retry/remediation state; no false completion | device application/domain tests |
| Corrupt backup is restored | point-in-time restore plus schema/data/integrity validation | Phase 2I D1 backup/restore drill |
| Sensitive or high-cardinality telemetry | class-only metric dimensions; forbidden identifier/data classes | operational-bounds policy and negative fixtures |
| Sensitive support evidence | allowlist-only class fields and forbidden-data policy | support-bundle policy and negative fixtures |
| Dependency or CI source substitution | exact dependency versions, lock integrity, SHA-pinned actions, approved registries only | Phase 2I supply-chain policy |

## Residual External risks

Phase 2I did not convert repository-local tests into proof of production Cloudflare behavior, real
mailbox-provider behavior, real Camoufox execution, physical multi-device behavior, production device-key
protection, trusted signing, remote R2/key recovery or independent cryptographic review. Those remain
Phase 2J evidence requirements and are tracked in the canonical threat model.

## Closure rule

A repository-owned threat was closed for Phase 2I only when its control was executable in permanent CI
or explicitly represented by a fail-closed policy with negative fixtures. External evidence gaps
continued to keep production readiness false without being misreported as repository-local failures.
''')

status_path = root / 'docs/status.json'
status = json.loads(status_path.read_text(encoding='utf-8'))
if status.get('schema_version') != 1 or status.get('as_of') != '2026-08-07':
    raise SystemExit('unexpected docs/status.json baseline before R7 migration')
repository_step = dict(status['repository_step'])
repository_step['historical'] = True
repository_step['scope'] = 'historical_repository_steps_0_10'
next_step = dict(status['next_repository_step'])
next_step.update({
    'number': None,
    'name': 'Phase 2J external production evidence after pre-2J closure',
    'status': 'blocked_pre2j_closure',
})
implementation = dict(status['implementation'])
implementation.update({
    'repository_local_product_phase': 'accepted_phase_2i',
    'pre2j_architecture_remediation': 'active_blocking_phase_2j',
    'application_ownership': 'r5_accepted_capability_owners_without_compatibility_facades',
    'frontend_api_ownership': 'r6_accepted_feature_owned_capability_api',
    'frontend': 'accepted_phase_2h_plus_r6_feature_owned_api',
    'cross_component_acceptance': 'accepted_phase_2i_integrated_repository_local',
})
current = {
    'accepted_product_phase': 'Phase 2I',
    'accepted_phase_ledger': 'architecture/accepted-phases.json',
    'documentation_index': 'docs/INDEX.md',
    'security_authority': 'docs/THREAT_MODEL.md',
    'pre2j_remediation': {
        'status': 'active_blocking_phase_2j',
        'plan': 'docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md',
    },
    'phase_2j': {
        'status': 'blocked_pre2j_closure',
        'plan': 'docs/DEVELOPMENT_PLAN.md',
        'production_ready_may_change_only_after_acceptance': True,
    },
}
new_status = {
    'schema_version': 2,
    'as_of': '2026-08-11',
    'current': current,
    'repository_step': repository_step,
    'next_repository_step': next_step,
    'accepted_steps': status['accepted_steps'],
    'historical_status_note': (
        'repository_step, accepted_steps and legacy evidence fields preserve historical Repository Steps 0-10 '
        'and earlier repository-local evidence; current phase authority is current + architecture/accepted-phases.json'
    ),
    'product': status['product'],
    'implementation': implementation,
    'evidence': status['evidence'],
    'decisions': status['decisions'],
    'external_gates': status['external_gates'],
    'production_ready': False,
}
status_path.write_text(json.dumps(new_status, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')

# Development Plan: preserve the accepted phase plan but block Phase 2J behind active pre-2J closeout.
must_replace(
    'docs/DEVELOPMENT_PLAN.md',
    '**Status:** normative post-composition execution plan  ',
    '**Status:** normative product phase plan; active pre-2J remediation overlay blocks Phase 2J  ',
)
must_replace(
    'docs/DEVELOPMENT_PLAN.md',
    '**Tracking:** Phase 1 complete; Phase 2A/2B/2C/2D/2E/2F/2G/2H/2I accepted via #118/#137, #138/#140, #142/#143, #144/#147, #148/#152, #154/#155, #159/#160, #163/#164 and #167/#168; Phase 2J is the unique NEXT after this docs closeout; expert-plan refinement #133; external CRM is future development only',
    '**Tracking:** Phase 1 complete; Phase 2A/2B/2C/2D/2E/2F/2G/2H/2I accepted via #118/#137, #138/#140, #142/#143, #144/#147, #148/#152, #154/#155, #159/#160, #163/#164 and #167/#168; Phase 2J is the next product phase but is blocked by the active `PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md` closure; expert-plan refinement #133; external CRM is future development only',
)
must_replace(
    'docs/DEVELOPMENT_PLAN.md',
    'This document is the **single normative source for active implementation order**.\nIt defines the exact critical path, architecture ownership, mandatory prerequisites, bounded slice\nscope and acceptance conditions from the accepted repository baseline to an expert-grade standalone\nproduct.',
    'This document is the **normative product phase plan**. While `PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md`\nis ACTIVE / BLOCKING PHASE 2J, that accepted remediation plan is the temporary execution blocker and\nclosure rule. This document continues to define the product critical path, architecture ownership,\nmandatory prerequisites, bounded phase scope and acceptance conditions.',
)
must_replace(
    'docs/DEVELOPMENT_PLAN.md',
    '- `DEVELOPMENT_PLAN.md` — execution order, slice ownership and acceptance;',
    '- `PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md` — temporary execution blocker/closure rule while ACTIVE;\n- `DEVELOPMENT_PLAN.md` — product phase order, slice ownership and acceptance;',
)
must_replace(
    'docs/DEVELOPMENT_PLAN.md',
    'If another roadmap conflicts with this document on **what to implement next**, this document wins.\nIf this document conflicts with an accepted ADR or security/data invariant, the invariant wins and\nthis plan must be corrected before implementation continues.',
    'While the pre-2J remediation plan is ACTIVE, it may block the next product phase and wins on the\nquestion of whether Phase 2J may start. Outside that temporary blocker, this document controls product\nphase order. Accepted ADR/security/data invariants always win and this plan must be corrected before\nimplementation continues when they conflict.',
)
must_replace(
    'docs/DEVELOPMENT_PLAN.md',
    'Exactly one implementation slice is active at a time. Phase 2J is the unique NEXT only after this\nPhase 2I docs closeout is accepted on `main`; future CRM work remains blocked by the same linear rule.',
    'No product phase is implementation-active while the pre-2J remediation plan is ACTIVE. Phase 2J\nremains the next product phase only after the remediation closure rule is satisfied on accepted `main`;\nfuture CRM work remains blocked by the same linear rule.',
)
must_replace(
    'docs/DEVELOPMENT_PLAN.md',
    'Phase 2A through Phase 2I are also accepted; Phase 2J is the unique NEXT and future CRM work remains\nblocked by the same linear rule.',
    'Phase 2A through Phase 2I are also accepted; Phase 2J is next only after the active pre-2J\nremediation closure is accepted, and future CRM work remains blocked by the same linear rule.',
)
must_replace(
    'docs/DEVELOPMENT_PLAN.md',
    '### Phase 2J — Production-readiness evidence and controlled rollout — NEXT',
    '### Phase 2J — Production-readiness evidence and controlled rollout — BLOCKED / NEXT AFTER PRE-2J',
)
must_replace(
    'docs/DEVELOPMENT_PLAN.md',
    'Phase 2J real production evidence + controlled rollout                                                NEXT',
    'Phase 2J real production evidence + controlled rollout                                                BLOCKED / NEXT AFTER PRE-2J',
)
must_replace(
    'docs/DEVELOPMENT_PLAN.md',
    '1. only the phase section marked `NEXT` is implementation-active;\n2. each phase may use bounded sub-PRs only when they preserve the listed internal order and the phase\n   itself does not close until every listed outcome is accepted;\n3. the next phase starts only after implementation acceptance + guarded merge + normative closeout;',
    '1. no product phase is implementation-active while `PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md` is ACTIVE;\n2. each phase may use bounded sub-PRs only when they preserve the listed internal order and the phase\n   itself does not close until every listed outcome is accepted;\n3. Phase 2J starts only after the pre-2J closure rule plus implementation acceptance/guarded closeout are satisfied;',
)
must_replace(
    'docs/DEVELOPMENT_PLAN.md',
    'After this docs/governance closeout is accepted on `main`, open the bounded implementation/evidence issue\nand start **Phase 2J — production-readiness evidence and controlled rollout** from the exact resulting\npre-2J closeout `main` SHA. Do not start Phase 2J directly from the Phase 2I implementation merge\n`800c634147d6300ea3989ff0cf87ade6e2387ee9`; this governance closeout is part of the linear gate.',
    'Do not start Phase 2J while `PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md` remains ACTIVE. The immediate\nrepository action is to complete its exact accepted closure rule, including R1-R7 closure and explicit\nclassification of any remaining P2 findings. Only after that accepted pre-2J closeout may a bounded\nPhase 2J evidence issue start from the exact resulting `main` SHA.',
)

# Architecture inventory documentation semantics.
must_replace(
    'scripts/generate-architecture-inventory.py',
    '    "REALTIME_NOTIFICATIONS.md",\n]',
    '    "REALTIME_NOTIFICATIONS.md",\n    "PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md",\n    "status.json",\n    "THREAT_MODEL.md",\n]',
)
must_replace(
    'scripts/generate-architecture-inventory.py',
    '''    next_sections = re.findall(r"^### (Phase [^\\n]+?) — NEXT\\s*$", plan, re.MULTILINE)\n    if len(next_sections) != 1:\n        raise SystemExit(f"DEVELOPMENT_PLAN.md must have exactly one Phase ... — NEXT section: {next_sections}")\n    immediate = plan.split("## 19. Immediate Next Action", 1)\n    if len(immediate) != 2 or next_sections[0].split(" — ", 1)[0] not in immediate[1]:\n        raise SystemExit("Immediate Next Action is inconsistent with the unique NEXT phase")\n''',
    '''    next_sections = re.findall(r"^### (Phase [^\\n]+?) — NEXT\\s*$", plan, re.MULTILINE)\n    if next_sections:\n        raise SystemExit(f"no product Phase ... — NEXT section is allowed while pre-2J remediation is active: {next_sections}")\n    blocked_phase2j = "Phase 2J — Production-readiness evidence and controlled rollout — BLOCKED / NEXT AFTER PRE-2J"\n    if blocked_phase2j not in plan:\n        raise SystemExit("DEVELOPMENT_PLAN.md must keep Phase 2J blocked behind pre-2J closure")\n    immediate = plan.split("## 19. Immediate Next Action", 1)\n    if len(immediate) != 2 or "PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md" not in immediate[1] or "Do not start Phase 2J" not in immediate[1]:\n        raise SystemExit("Immediate Next Action must enforce the active pre-2J blocker")\n''',
)
must_replace(
    'scripts/generate-architecture-inventory.py',
    '        "Phase 2J — Production-readiness evidence and controlled rollout — NEXT",',
    '        "Phase 2J — Production-readiness evidence and controlled rollout — BLOCKED / NEXT AFTER PRE-2J",\n        "`PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md`",',
)
must_replace(
    'scripts/generate-architecture-inventory.py',
    '    stale_plan_markers = (\n',
    '    stale_plan_markers = (\n        "Phase 2J — Production-readiness evidence and controlled rollout — NEXT",\n        "Phase 2J is the unique NEXT",\n',
)
must_replace(
    'scripts/generate-architecture-inventory.py',
    '            "index": "docs/INDEX.md",\n',
    '            "index": "docs/INDEX.md",\n            "pre2j_execution_blocker": "docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md",\n            "readiness": "docs/status.json",\n            "security": "docs/THREAT_MODEL.md",\n            "accepted_phase_ledger": "architecture/accepted-phases.json",\n',
)

# Fast preflight must enforce the same current documentation authority.
must_replace(
    'scripts/verify-fast.py',
    '        "check-frontend-feature-boundaries.py",\n',
    '        "check-frontend-feature-boundaries.py",\n        "check-documentation-authority.py",\n',
)

# Permanent Quality Gate: compile and execute the policy + negative proof.
must_replace(
    '.github/workflows/quality-gate.yml',
    '          scripts/generate-architecture-inventory.py\n',
    '          scripts/generate-architecture-inventory.py\n          scripts/check-documentation-authority.py\n',
)
must_replace(
    '.github/workflows/quality-gate.yml',
    '''      - name: Prove architecture inventory drift is detected\n        run: python scripts/generate-architecture-inventory.py --self-test\n\n''',
    '''      - name: Prove architecture inventory drift is detected\n        run: python scripts/generate-architecture-inventory.py --self-test\n\n      - name: Enforce current documentation, readiness and security authority\n        run: python scripts/check-documentation-authority.py\n\n      - name: Prove stale documentation authority fixtures are rejected\n        run: python scripts/check-documentation-authority.py --self-test\n\n''',
)

# Repository Quality Audit gets an independent permanent policy lane.
must_replace(
    '.github/workflows/repository-quality-audit-gate.yml',
    '          python -m py_compile scripts/generate-architecture-inventory.py\n',
    '          python -m py_compile scripts/generate-architecture-inventory.py\n          python -m py_compile scripts/check-documentation-authority.py\n',
)
must_replace(
    '.github/workflows/repository-quality-audit-gate.yml',
    '''      - name: Prove architecture inventory drift is rejected\n        run: python scripts/test-architecture-inventory-negative.py\n\n''',
    '''      - name: Prove architecture inventory drift is rejected\n        run: python scripts/test-architecture-inventory-negative.py\n\n      - name: Enforce current documentation, readiness and security authority\n        run: python scripts/check-documentation-authority.py\n\n      - name: Prove stale documentation authority fixtures are rejected\n        run: python scripts/check-documentation-authority.py --self-test\n\n''',
)

(root / '.github/.r7-user-finalize').write_text(
    'temporary marker; delete with user-authored finalization commit\n', encoding='utf-8'
)
