# Plan Readiness Review — Historical Repository-Step Record

**Document status:** HISTORICAL_ACCEPTED_REVIEW  
**Original review date:** 2026-08-05  
**Current authority:** [`INDEX.md`](INDEX.md) -> [`ARCHITECTURE_REBASELINE_V3_PLAN.md`](ARCHITECTURE_REBASELINE_V3_PLAN.md)

This document preserves an early Repository-Step readiness review. Statements below about the then-current step, next step, `DELIVERY_ROADMAP.md`, Phase 2J or future execution order are historical context only. They do **not** define current work.

Current execution is governed by Architecture Re-baseline v3. As of the accepted pre-PF-1 normalization plan, the required continuation is `F1/F2 -> N1 -> N2 -> N3 -> N4 -> N5 -> PF-1 -> PF-2 -> PF-3 -> fresh #399/#421 re-baseline -> FC-6 -> FC-7 -> AR-12`. `DELIVERY_ROADMAP.md` is itself historical.

---

## Historical review content

**Статус на момент review:** Repository Step 0 accepted; Step 1 cold build passed in PR  
**Дата review:** 2026-08-05  
**Актуальная на тот момент архитектура:** ADR-0005 Cloudflare-native control plane без VM

## Проверено

- README, product boundary, implementation plan, architecture map and UI plan;
- ADR-0001..ADR-0006 with explicit status registry;
- preservation of local Camoufox runtime with Cloudflare control plane;
- D1 catalog boundary and absence of PostgreSQL RLS;
- Durable Object per-profile coordination and fencing model;
- partial-failure protocol between D1, DO, Queue and R2;
- encrypted R2 materialization, forgotten-window and multi-device targets;
- future replacement of Cloudflare adapters by CRM identity/PostgreSQL adapters;
- current GitHub execution environment and evidence limitations;
- exact Rust/Cloudflare repository cold build in Quality Gate run `31036328555`.

## Corrected Readiness Meaning

The architecture contained no known contradiction blocking the then-current Repository-Step execution.
Repository Step 0 established the executable workspace and permanent CI.
Repository Step 1 proved that Rust `1.97.1`, `worker 0.8.5`, direct
`wasm-bindgen 0.2.126` and `worker-build 0.8.5` could compile and package the
selected D1/R2/Queue/Durable Object/Static Assets boundary.

This did **not** mean the application, remote Cloudflare deployment,
Bridge, key recovery or production profile lifecycle existed. The Worker at that time was a binding/route skeleton without business data or production credentials.

## ADR Status Corrections

- ADR-0001 remained `proposed` and blocked production profile generation and certification claims.
- ADR-0002 was accepted with bounded one-device smoke evidence only.
- ADR-0003, ADR-0004 and ADR-0005 were accepted architecture decisions, not completed product implementation.
- ADR-0006 remained `proposed` and blocked production cloud generations and multi-device key delivery.

The authoritative projection at that time was `ADR_STATUS.md` and `status.json`.

## Historical Execution Model

Development was then described as sequential Repository Steps through GitHub branches,
pull requests, permanent workflows and squash merge. Code/tests/evidence available
through GitHub could be completed autonomously. External state was never inferred:
credential rotation, Cloudflare resources, physical Windows hosts, code signing,
offline escrow and legal approval required separate evidence.

The historical detailed order was `DELIVERY_ROADMAP.md`. That ordering is now superseded for forward execution.

## Accepted Step 0 Gate

Step 0 proved:

- exact Rust `1.97.1` workspace and committed lockfile;
- formatting, Clippy and tests on Linux;
- workspace tests on Windows;
- pure domain primitive for `wasm32-unknown-unknown`;
- machine-readable status and tracked-file secret checks;
- indexed product/security/privacy/evidence governance.

## Step 1 Evidence Scope

Permanent Quality Gate run `31036328555` proved:

- exact Worker dependency graph compiles for WASM;
- D1, R2, Queue, Durable Object and Static Assets bindings coexist in the Worker;
- generated Durable Object export compiles;
- `worker-build --release` produces an optimized Wasm package and JS shim;
- permanent CI remains read-only and requires no Cloudflare credential.

It did not prove real binding IDs, Access, D1 migrations, Durable Object storage,
Queue delivery, R2 operations, remote deploy/rollback or cost limits. Full scope
was recorded in the Step 1 evidence report.

## Historical Mandatory Later Gates

- revoke/rotate the legacy proxy credential before any prototype reuse;
- deploy the accepted Worker to isolated Cloudflare staging and prove bindings;
- complete threat review before processing production data;
- accept ADR-0006 and pass clean-environment key recovery;
- provision separate Cloudflare environments and cost limits;
- prove Bridge lifecycle on an approved Windows host;
- obtain trusted Windows code-signing certificate before stable release;
- use a second independent Windows host for multi-device proof;
- accept ADR-0001 and complete fingerprint certification before promotion;
- accept isolation ADR before adding a second independent tenant;
- accept privacy/retention values and authorized-use policy.

## Historical Residual Risks

- remote Cloudflare runtime compatibility and binding behavior remained unproven;
- the published `worker-build 0.8.5` tool lock emitted yanked transitive-package warnings and needed upgrade/supply-chain review;
- Cloudflare account recovery was part of disaster recovery;
- D1 adapter lacked defense-in-depth RLS;
- production key root/offline escrow was a plan, not evidence;
- physical multi-device behavior was not proven;
- Windows installer, embedded runtime and process supervision remained major feasibility risks;
- external fingerprint checkers and provider behavior could drift;
- repository license remained a product-owner decision.

## Historical Verdict

The original verdict was to accept Step 1 after its final PR quality gate and then proceed according to the historical `DELIVERY_ROADMAP.md`, while keeping production/external claims blocked until proved. That verdict is preserved only as historical evidence; current sequencing comes exclusively from the current authority hierarchy linked at the top of this file.
