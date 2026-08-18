# Browser Profile Lifecycle Plan — Superseded Forward-Execution Entrypoint

**Document status:** SUPERSEDED  
**Exact preserved former body:** `history/PROFILE_LIFECYCLE_PLAN_PRE_AR_V3_2026-08-15.md`  
**Current program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Camoufox runtime design input:** `docs/CAMOUFOX_RUNTIME_CUTOVER_PLAN.md`  
**Tracking issue:** #266

The former lifecycle plan is preserved byte-for-byte under `history/`. Its accepted lifecycle reasoning
remains useful evidence, but this root path is no longer an independent current roadmap after AR-1.

Current lifecycle/program sequencing is governed through Architecture Re-baseline v3. Existing accepted
profile lifecycle, fencing, recovery and authorization invariants are preserved unless their owning AR
slice proves and accepts a bounded change.

The subordinate Camoufox runtime cutover plan does not create another execution sequence. It clarifies the
already-assigned AR-10 obligation from `architecture/python-estate-ar6.json`: migrate supported real
Camoufox/browser-profile execution behind Profile Bridge/Camouhost before retiring the direct research
launcher. AR-15 remains the Windows signing/update/delivery owner for the runtime accepted by AR-10, while
physical-host and production fingerprint certification remain later External evidence. AR-8D remains the
current implementation slice until the canonical authority advances it.
