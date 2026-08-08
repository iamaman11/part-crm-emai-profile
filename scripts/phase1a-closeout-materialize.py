#!/usr/bin/env python3
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]


def replace_once(path: str, old: str, new: str) -> None:
    target = ROOT / path
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one replacement target, found {count}: {old!r}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8")


PLAN = "docs/DEVELOPMENT_PLAN.md"
MATRIX = "docs/DEVELOPER_CAPABILITY_MATRIX.md"

replace_once(
    PLAN,
    "**Tracking:** Phase 0N accepted via #110/#111; Phase 0 complete; next planned Phase 1A; plan consolidation history #96",
    "**Tracking:** Phase 1A accepted via #114/#115; Phase 0 complete; next sequential slice Phase 2A; Phase 1B eligible dependency-independently; plan consolidation history #96",
)
replace_once(
    PLAN,
    "Repository Steps 0–10 and the accepted post-composition slices through **Phase 0N** provide",
    "Repository Steps 0–10 and the accepted post-composition slices through **Phase 1A** provide",
)
replace_once(
    PLAN,
    "architecture inventory and exact-head cross-component acceptance.",
    "architecture inventory, a versioned durable integration-event/outbox substrate with replay-safe\nnotification persistence and exact-head cross-component acceptance.",
)
replace_once(
    PLAN,
    """**Phase 0 is complete on accepted `main`.** No sequential Phase 0 implementation slice remains.
The **next planned slice is Phase 1A — durable event/outbox foundation**, which must start from
the accepted Phase 0N `main` in its own bounded issue/branch/PR. Phase 1B remains later delivery
hardening and must not be folded into the initial 1A foundation slice.""",
    """Phase 1A was accepted through issue **#114** / PR **#115** with guarded squash merge
`0186b780f7fed4b7c5e7f212c2fe437cbc46a5e5` from exact proven source head
`21b4bc65cd1bb117504c0a0cfe18c8c11e411f25`. The accepted foundation versions the integration
event envelope, evolves the existing durable outbox, persists metadata-only notification events,
adds tenant/consumer/outbox idempotency, dispatches through the Queue adapter and keeps Queue/
scheduled ingress application-thin. Canonical-source guards reject forged event metadata/payload,
prohibited PII/secrets/mail bodies fail closed before persistence, and duplicate accepted delivery
has no duplicate logical effect. Phase 1B retry/backoff/DLQ/catch-up/retention remains unimplemented.

**Phase 0 remains complete on accepted `main`, and Phase 1A is accepted.** The **next planned
sequential slice is Phase 2A — client aggregate and contact crypto foundation**, starting from the
accepted Phase 1A `main` in its own bounded issue/branch/PR. Phase 1B is eligible to proceed
only dependency-independently and must finish before real asynchronous provider/device execution
in Phases 4–5 and before Phase 6 realtime.""",
)
replace_once(
    PLAN,
    "### Phase 1A — Durable event/outbox foundation — NEXT",
    "### Phase 1A — Durable event/outbox foundation — ACCEPTED",
)
replace_once(PLAN, "Build and accept first:", "Accepted implementation:")
replace_once(
    PLAN,
    """- consumer processing is replay-safe for the accepted event set.

After Phase 1A is accepted, **Phase 2 may begin** because Client Registry expansion only needs
the durable event/outbox contract. Phase 1B may proceed in parallel when it does not overlap
with the active Phase 2 files/contracts.""",
    """- consumer processing is replay-safe for the accepted event set.

Acceptance used exact source head `21b4bc65cd1bb117504c0a0cfe18c8c11e411f25`, 12/12 permanent
workflows green, `behind_by=0`, zero blocking reviews/threads and guarded squash merge #115
`0186b780f7fed4b7c5e7f212c2fe437cbc46a5e5`.

With Phase 1A accepted, **Phase 2 may begin** because Client Registry expansion only needs the
durable event/outbox contract. Phase 1B may proceed in parallel when it does not overlap with
the active Phase 2 files/contracts; it remains mandatory before real asynchronous Phases 4–5
and before Phase 6 realtime.""",
)
replace_once(
    PLAN,
    """## 6. Phase 2 — Client Registry 2.0 And Assignment Model

**Goal:** complete the standalone business client model before search and CRM integration.
""",
    """## 6. Phase 2 — Client Registry 2.0 And Assignment Model

**Goal:** complete the standalone business client model before search and CRM integration.

### Phase 2A — Client aggregate and contact crypto foundation — NEXT

Start Phase 2 with the first bounded registry slice rather than the whole phase:

- provider-neutral client aggregate/value model for `PERSON|ORGANIZATION`, lifecycle status and
  versioned metadata;
- encrypted-at-rest contact display values and tenant-keyed HMAC exact-lookup tokens;
- no plaintext contact scan and no name/contact-derived technical identifiers;
- application-owned create/update/archive intent behind ports before transport wiring;
- additive D1 schema/adapter work only after inward native/WASM proof;
- assignment/merge lifecycle and wider API/UI projections remain later Phase 2 slices unless the
  bounded 2A issue proves they are required for the same invariant.

Phase 2A acceptance must retain Phase 1A durable mutation/audit/outbox semantics and all existing
authorization-before-projection, PII and generated-contract boundaries.
""",
)
replace_once(
    PLAN,
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
    """Start **Phase 2A — client aggregate and contact crypto foundation** from the accepted Phase 1A
`main`. Create a fresh bounded issue/branch/PR before implementation; do not fold Phase 2A into the
completed #114/#115 history. Phase 1B delivery hardening may proceed only dependency-independently
and must not be mixed into the first Client Registry slice.

Primary Phase 2A acceptance target:

```text
provider-neutral client aggregate + lifecycle values
  -> encrypted contact display values
  -> tenant-keyed HMAC exact-contact lookup tokens
  -> no plaintext contact scan or PII-derived technical identifiers
  -> application-owned create/update/archive intent behind ports
  -> additive D1 adapter/schema only after inward proof
  -> canonical mutation + audit/outbox remains atomic
  -> exact-head permanent CI + guarded merge
```

Keep all accepted Phase 0 and Phase 1A boundaries, generated-contract/feature-boundary rules and
architecture inventory checks intact. Continue long-lead External gate preparation in parallel
without changing `production_ready=false`; real provider/physical-host evidence remains External.""",
)

replace_once(
    MATRIX,
    "| Integration events / durable notifications | Target | Architecture/plan contracts defined. | Phase 1 implementation not accepted. |",
    "| Integration events / durable notifications | Composed / Synthetic | Accepted Phase 1A versioned envelope, evolved D1 outbox, metadata-only `notification_events`, tenant/consumer/outbox idempotency, sanitized Queue dispatch, thin scheduled/Queue ingress, canonical-source guards and deterministic duplicate/failure-order evidence. | Phase 1B retry/backoff/max-attempt/DLQ/catch-up/retention and real provider/device/realtime delivery remain Target/External. |",
)
replace_once(
    MATRIX,
    """- accepted Phase 0N splits route matching into capability-owned classifiers behind one composed fail-closed entrypoint, prevents unknown `/api/*`, `/auth/*` and `/bridge/*` variants from reaching SPA assets, and permanently verifies deterministic `architecture/inventory.json` plus selected documentation consistency claims;
- the current execution plan, not this matrix, determines subsequent order; Phase 1A is next planned while integration events/durable notifications remain Target until that implementation is accepted.""",
    """- accepted Phase 0N splits route matching into capability-owned classifiers behind one composed fail-closed entrypoint, prevents unknown `/api/*`, `/auth/*` and `/bridge/*` variants from reaching SPA assets, and permanently verifies deterministic `architecture/inventory.json` plus selected documentation consistency claims;
- accepted Phase 1A composes the versioned integration-event envelope, evolved durable outbox, metadata-only notification persistence, Queue dispatcher/consumer and durable consumer idempotency behind provider-neutral ports, with canonical-source and payload-sanitization evidence on native/WASM paths;
- the current execution plan, not this matrix, determines subsequent order; Phase 2A is the next sequential slice, while Phase 1B is eligible dependency-independently and remains required before real async provider/device and realtime execution.""",
)
replace_once(
    MATRIX,
    "crates/application-ports\n  capability-owned interfaces required by application workflows",
    "crates/application-ports\n  capability-owned interfaces required by application workflows, including accepted integration-event outbox/publisher/notification/idempotency ports",
)
replace_once(
    MATRIX,
    "crates/use-cases\n  accepted shared application crate for remaining contexts; identity modules are compatibility re-exports only",
    "crates/use-cases\n  accepted shared application crate for remaining contexts; identity modules are compatibility re-exports only; Phase 1A dispatcher and foundation consumer semantics are application-owned here",
)
replace_once(
    MATRIX,
    "crates/cloudflare-adapters\n  D1/Access/DO/R2/Queue/provider implementations that depend inward",
    "crates/cloudflare-adapters\n  D1/Access/DO/R2/Queue/provider implementations that depend inward, including the accepted Phase 1A D1 integration-event repository and Queue publisher adapter",
)
replace_once(
    MATRIX,
    "apps/control-plane-worker\n  thin Worker/DO composition and transport; coordinator ingress is application-thin on accepted main",
    "apps/control-plane-worker\n  thin Worker/DO/Queue/Scheduled composition and transport; coordinator and accepted Phase 1A event ingress remain application-thin on accepted main",
)

plan = (ROOT / PLAN).read_text(encoding="utf-8")
next_sections = re.findall(r"^### (Phase [^\n]+?) — NEXT\s*$", plan, re.MULTILINE)
if next_sections != ["Phase 2A — Client aggregate and contact crypto foundation"]:
    raise SystemExit(f"unexpected NEXT sections after closeout: {next_sections}")
if "Start **Phase 2A" not in plan.split("## 19. Immediate Next Action", 1)[1]:
    raise SystemExit("Immediate Next Action did not advance to Phase 2A")
if "`production_ready=false`" not in plan:
    raise SystemExit("production readiness claim was lost")

print("Phase 1A docs closeout materialized; Phase 2A is the unique sequential NEXT.")
