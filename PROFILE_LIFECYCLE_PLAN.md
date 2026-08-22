# Browser Profile Lifecycle Plan — Superseded Forward-Execution Entrypoint

**Document status:** SUPERSEDED  
**Exact preserved former body:** `history/PROFILE_LIFECYCLE_PLAN_PRE_AR_V3_2026-08-15.md`  
**Current program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Current documentation index:** `docs/INDEX.md`  
**Camoufox runtime design input:** `docs/CAMOUFOX_RUNTIME_CUTOVER_PLAN.md`  
**Tracking issue:** #266

The former lifecycle plan is preserved byte-for-byte under `history/`. Its accepted lifecycle reasoning remains useful evidence, but this root path is no longer an independent current roadmap after AR-1.

Current lifecycle/program sequencing is governed through Architecture Re-baseline v3. Existing accepted profile lifecycle, fencing, recovery and authorization invariants are preserved unless their owning current architecture transaction/slice proves and accepts a bounded change.

The subordinate Camoufox runtime cutover plan does not create another execution sequence. It clarifies the accepted AR-10 runtime obligations and remains design/provenance input for the current normalization and later Windows/runtime work.

Do not infer the current slice or next implementation gate from historical text in this file or its preserved predecessor. Current execution must be read from `docs/INDEX.md` / `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`; at the current accepted baseline the required pre-AR-12 path begins with `F1/F2`, then `N1…N5`, before PF-1/PF-2/PF-3 and Functional Closure.
