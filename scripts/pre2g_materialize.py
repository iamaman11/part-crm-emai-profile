#!/usr/bin/env python3
"""One-shot, fail-closed Phase 2F docs closeout materializer.

Every edit is anchored to exact accepted-main text. A missing/duplicated anchor aborts before write.
The temporary workflow deletes this script after successful validation and commit.
"""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PLAN = ROOT / "docs" / "DEVELOPMENT_PLAN.md"
ARCH = ROOT / "docs" / "ARCHITECTURE.md"


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one anchor, observed {count}")
    return text.replace(old, new, 1)


def materialize_plan() -> None:
    text = PLAN.read_text(encoding="utf-8")

    text = replace_once(
        text,
        "**Date:** 2026-08-09  \n**Tracking:** Phase 1 complete; Phase 2A/2B/2C/2D/2E accepted via #118/#137, #138/#140, #142/#143, #144/#147 and #148/#152; Phase 2F is the unique NEXT after this docs closeout; expert-plan refinement #133; external CRM is future development only",
        "**Date:** 2026-08-10  \n**Tracking:** Phase 1 complete; Phase 2A/2B/2C/2D/2E/2F accepted via #118/#137, #138/#140, #142/#143, #144/#147, #148/#152 and #154/#155; Phase 2G is the unique NEXT after this docs closeout; expert-plan refinement #133; external CRM is future development only",
        "plan header",
    )

    text = replace_once(
        text,
        "  `use-cases-identity`, `use-cases-notifications`, `use-cases-clients`, `use-cases-query` and\n  `use-cases-mailboxes`;",
        "  `use-cases-identity`, `use-cases-notifications`, `use-cases-clients`, `use-cases-query`,\n  `use-cases-mailboxes` and `use-cases-devices`;",
        "accepted application crates",
    )

    text = replace_once(
        text,
        "- Phase 2E decomposed `mailbox-domain`, independent `use-cases-mailboxes`, real Gmail API/IMAP outer\n  adapters, durable Queue retry/DLQ/idempotency/fencing, opaque secret-resolution, the accepted Phase\n  2D Client Mail contract on the cloud adapter, and permanent mailbox privacy/runtime enforcement.\n",
        "- Phase 2E decomposed `mailbox-domain`, independent `use-cases-mailboxes`, real Gmail API/IMAP outer\n  adapters, durable Queue retry/DLQ/idempotency/fencing, opaque secret-resolution, the accepted Phase\n  2D Client Mail contract on the cloud adapter, and permanent mailbox privacy/runtime enforcement;\n- Phase 2F independent `device-domain`/`use-cases-devices`, durable D1 device jobs and browser mailbox\n  execution, trusted claim/generation/Coordinator fencing checks, retained Bridge writer ownership through\n  immutable upload + exact verification + fenced/CAS commit, and deterministic rematerialization recovery.\n",
        "Phase 2F baseline",
    )

    text = replace_once(
        text,
        "Phase 2E was accepted through issue #148 / PR #152 from exact proven source head\n`0cefa67abe810db079102462f33ec28fcfc73f69` and guarded squash merge\n`6c6ba4564de88b40d282081e701a2d24f1611cc2`.\n",
        "Phase 2E was accepted through issue #148 / PR #152 from exact proven source head\n`0cefa67abe810db079102462f33ec28fcfc73f69` and guarded squash merge\n`6c6ba4564de88b40d282081e701a2d24f1611cc2`.\n\nPhase 2F was accepted through issue #154 / PR #155 from exact proven source head\n`c36df418f9fa877c5143327e97b60087c33ffd02` and guarded squash merge\n`42b26dc0c78c0c65dcea2bc90bb5ce6a3bd4b02b`.\n",
        "Phase 2F provenance",
    )

    text = replace_once(
        text,
        "Exactly one implementation slice is active at a time. Phase 2F is the unique NEXT only after this\nPhase 2E docs closeout is accepted on `main`; later Phase 2 slices remain blocked by the same linear rule.",
        "Exactly one implementation slice is active at a time. Phase 2G is the unique NEXT only after this\nPhase 2F docs closeout is accepted on `main`; later Phase 2 slices remain blocked by the same linear rule.",
        "unique next baseline",
    )

    text = replace_once(
        text,
        "`application-ports` remains one Cargo crate with capability-owned modules throughout this roadmap.\nThat boundary is already accepted and avoids needless crate-per-interface fragmentation.",
        "`application-ports` remains one Cargo crate with capability-owned modules throughout this roadmap.\nThat boundary is already accepted and avoids needless crate-per-interface fragmentation.\n\n`crates/use-cases` remains the canonical application owner for Profile Catalog and Profile Generation Registry workflows\nuntil a future explicitly scoped extraction is accepted. Identity, client, query, mailbox, notification and\ndevice contexts already extracted into independent crates never move back into the shared surface merely\nfor symmetry; no `use-cases-profiles` extraction is implied without a dedicated owning slice.",
        "profile generation application ownership",
    )

    replacements = (
        (
            "| A3 | Domain aggregate decomposition | **Client half accepted in Phase 2A.** `client-domain` is decomposed behind a thin facade; `mailbox-domain` remains monolithic. | Mailbox split first in 2E. |",
            "| A3 | Domain aggregate decomposition | **Accepted.** Phase 2A decomposed `client-domain`; Phase 2E decomposed `mailbox-domain`; Phase 2F owns durable device state separately in `device-domain`. | Preserve continuously; split further only at an explicit growth point. |",
            "A3 accepted state",
        ),
        (
            "| 6.2 | Durable-before-notify | **Accepted through durable delivery in Phase 1B.** Durable mutation/outbox precedes Queue delivery state and notification surfaces. | Preserve; 2E–2G and 2I extend the same failure ordering to new consumers/realtime. |",
            "| 6.2 | Durable-before-notify | **Accepted through Phase 2F repository-owned consumers.** Phase 1B durable delivery is preserved by Phase 2E mailbox and Phase 2F device/browser execution ordering. | Phase 2G extends the same durable-before-signal rule to realtime; preserve through 2I. |",
            "6.2 accepted state",
        ),
        (
            "| 6.3 | At-least-once consumer idempotency | **Accepted for the current notification consumer through Phase 1B retry/DLQ/replay.** | 2E/2F every new Queue/device consumer must preserve duplicate neutrality. |",
            "| 6.3 | At-least-once consumer idempotency | **Accepted through Phase 2F.** Notification replay, Phase 2E mailbox execution and Phase 2F durable device jobs preserve duplicate-neutral canonical effects. | Preserve for every later consumer/realtime signal. |",
            "6.3 accepted state",
        ),
        (
            "| 6.4 | Authorization-before-projection | **Accepted through Phase 2D query/read-model scope.** Live membership/grants precede list/search/detail projection, exact-contact lookup and mailbox eligibility/provider invocation; grant-sensitive D1 predicates recheck visibility where applicable. | Preserve in 2E/2F real provider lanes; 2G extends the same rule to realtime subscriptions. |",
            "| 6.4 | Authorization-before-projection | **Accepted through Phase 2D query/read-model scope.** Live membership/grants precede list/search/detail projection, exact-contact lookup and mailbox eligibility/provider invocation; the accepted Phase 2E cloud and Phase 2F browser lanes preserve this ordering. | Phase 2G extends the same rule to realtime subscriptions. |",
            "6.4 accepted state",
        ),
        (
            "| 6.6 | Profile materialization contract | **Library/Synthetic foundation exists.** | 2F real device/browser lane integration; 2I recovery/E2E; 2J physical/external evidence. |",
            "| 6.6 | Profile materialization | **Accepted repository-local through Phase 2F.** Retained writer ownership, immutable dirty-generation evolution, exact verification, fenced/CAS commit and deterministic rematerialization are composed/synthetic. | Phase 2I closes broader recovery/E2E; Phase 2J supplies real physical/provider evidence. |",
            "6.6 accepted state",
        ),
    )
    for old, new, label in replacements:
        text = replace_once(text, old, new, label)

    text = replace_once(
        text,
        "tracked by accepted evidence: the client half of A3 was accepted in 2A, A5 in 2C and A8 in 2D; only\nthe mailbox half of A3 remains open and is owned by 2E.",
        "tracked by accepted evidence: A3 is complete through the Phase 2A client and Phase 2E mailbox\ndecompositions plus separate Phase 2F device ownership; A5 was accepted in 2C and A8 in 2D.",
        "Phase 0 obligation summary",
    )

    text = replace_once(
        text,
        "**Status:** ACCEPTED. Phase 1 is complete; Phase 2A, Phase 2B, Phase 2C and Phase 2D are accepted; Phase 2E issue #148 is the unique next implementation slice.",
        "**Status:** ACCEPTED. Phase 1 is complete; Phase 2A through Phase 2F are accepted; Phase 2G is the unique next implementation slice after this closeout.",
        "Phase 1 current status",
    )

    text = replace_once(
        text,
        "### Phase 2F — Durable device jobs, browser mailbox lane and materialization integration — NEXT",
        "### Phase 2F — Durable device jobs, browser mailbox lane and materialization integration — ACCEPTED",
        "Phase 2F heading",
    )

    phase2f_acceptance_end = "- local materialization remains cache/workspace, not authority.\n\n### Phase 2G — Durable realtime notification hub"
    phase2f_evidence = """- local materialization remains cache/workspace, not authority.\n\n#### Phase 2F acceptance evidence\n\nPhase 2F was accepted through issue #154 / PR #155 from exact proven source head\n`c36df418f9fa877c5143327e97b60087c33ffd02` and guarded squash merge\n`42b26dc0c78c0c65dcea2bc90bb5ce6a3bd4b02b`. The unchanged source head passed exactly 12/12\npermanent workflows with `behind_by=0`, reviews=0 and unresolved review threads=0. Accepted scope\nincludes independent provider-neutral device ownership, durable device jobs/claims, browser mailbox\nexecution, generation/fencing freshness, retained writer ownership, immutable generation upload, exact\nverification, authoritative fenced/CAS commit and deterministic post-commit rematerialization recovery.\nReal physical-device, Camoufox, provider, remote R2/key and production-runtime evidence remains External;\n`production_ready=false` remains intentional.\n\n### Phase 2G — Durable realtime notification hub — NEXT"""
    text = replace_once(text, phase2f_acceptance_end, phase2f_evidence, "Phase 2F acceptance evidence")

    text = replace_once(
        text,
        "  use-cases-identity/\n  use-cases-clients/",
        "  use-cases/             # canonical remaining Profile Catalog / Generation Registry workflows\n  use-cases-identity/\n  use-cases-clients/",
        "target shared use-cases owner",
    )

    text = replace_once(
        text,
        "This is a target ownership map, not permission to create empty placeholder crates. Each named new\ncrate is created only in its already-fixed owning phase above.",
        "This is a target ownership map, not permission to create empty placeholder crates. Each named new\ncrate is created only in its already-fixed owning phase above. Shared `use-cases` is the current\ncanonical Profile Catalog / Generation Registry application owner; a future `use-cases-profiles`\nextraction requires an explicit owning slice and is not implied by architectural symmetry.",
        "target map ownership note",
    )

    text = replace_once(
        text,
        "19. **Evidence-scope gate** — synthetic/local evidence never promotes External claims.",
        "19. **Evidence-scope gate** — synthetic/local evidence never promotes External claims.\n20. **Accepted-provenance gate** — `architecture/accepted-phases.json` and historical issue/PR/source-head/merge-SHA claims must agree; tampered provenance fails closed.",
        "provenance architecture gate",
    )

    text = replace_once(
        text,
        "Phase 2F device-domain + use-cases-devices + browser/Bridge mailbox lane                            NEXT\nPhase 2G durable realtime notification hub",
        "Phase 2F device-domain + use-cases-devices + browser/Bridge mailbox lane                        ACCEPTED\nPhase 2G durable realtime notification hub                                                      NEXT",
        "sequential phase status",
    )

    marker = "## 19. Immediate Next Action\n\n"
    if text.count(marker) != 1:
        raise SystemExit("Immediate Next Action marker must occur exactly once")
    prefix, _ = text.split(marker, 1)
    tail = """After this docs-only closeout is accepted on `main`, open the bounded implementation issue and start\n**Phase 2G — durable realtime notification hub** from the accepted Phase 2F merge\n`42b26dc0c78c0c65dcea2bc90bb5ce6a3bd4b02b`.\n\nExecute Phase 2G inward-first in this exact order:\n\n```text\nversioned realtime-safe change signals only\n  -> per-user notification-hub ports/use cases in use-cases-notifications\n  -> outer per-user Durable Object + Hibernatable WebSocket adapter\n  -> authenticate + authorize before subscribe/deliver\n  -> durable cursor catch-up before live continuation\n  -> bounded reauthorization + immediate revoked-membership disconnect\n  -> metadata-safe signals only; no contact plaintext/mail body/secrets\n  -> frontend invalidates/refetches canonical HTTPS query data\n  -> multi-tab/device/disconnect/reconnect/cursor-gap/revoke-race evidence\n```\n\nNo Phase 2H+ implementation starts before Phase 2G is accepted and closed out. Real Gmail/IMAP,\nCamoufox, physical-device, remote R2/key and production-runtime claims remain External evidence, and\n`production_ready=false` remains unchanged.\n"""
    text = prefix + marker + tail

    PLAN.write_text(text, encoding="utf-8", newline="\n")


def materialize_architecture() -> None:
    text = ARCH.read_text(encoding="utf-8")
    text = replace_once(
        text,
        "`use-cases-devices`; shared `use-cases` compatibility re-exports do not regain canonical ownership\nof those capabilities.",
        "`use-cases-devices`; shared `use-cases` compatibility re-exports do not regain canonical ownership\nof those capabilities. `crates/use-cases` remains the canonical application owner for current Profile Catalog and Profile Generation Registry workflows until a future explicitly scoped extraction is accepted.",
        "architecture shared application ownership",
    )
    text = replace_once(
        text,
        "  use-cases-query/\n  use-cases-profiles/\n  use-cases-mailboxes/",
        "  use-cases/             # current Profile Catalog / Generation Registry application owner\n  use-cases-query/\n  use-cases-mailboxes/",
        "architecture target profile owner",
    )
    text = replace_once(
        text,
        "A temporary compatibility facade is allowed during crate migration but must not become the\npermanent cross-domain orchestration owner.",
        "A temporary compatibility facade is allowed during crate migration but must not become the\npermanent cross-domain orchestration owner. The shared `use-cases` crate is not merely such a facade\nfor Profile Catalog / Generation Registry today: it remains their explicit canonical application owner\nuntil a dedicated extraction is separately scoped and accepted.",
        "architecture target ownership explanation",
    )
    ARCH.write_text(text, encoding="utf-8", newline="\n")


def main() -> None:
    materialize_plan()
    materialize_architecture()
    print("Pre-2G documentation materialized from exact accepted anchors.")


if __name__ == "__main__":
    main()
