# Plan Readiness Review

**Статус:** approved for Repository Step 0 and bounded feasibility work  
**Дата review:** 2026-08-05  
**Актуальная архитектура:** ADR-0005 Cloudflare-native control plane без VM

## Проверено

- README, product boundary, implementation plan, architecture map and UI plan;
- ADR-0001..ADR-0006 with explicit status registry;
- preservation of local Camoufox runtime with Cloudflare control plane;
- D1 catalog boundary and absence of PostgreSQL RLS;
- Durable Object per-profile coordination and fencing model;
- partial-failure protocol between D1, DO, Queue and R2;
- encrypted R2 materialization, forgotten-window and multi-device targets;
- future replacement of Cloudflare adapters by CRM identity/PostgreSQL adapters;
- current GitHub execution environment and evidence limitations.

## Corrected Readiness Meaning

The architecture contains no known contradiction blocking Repository Step 0 or a
Cloudflare/Windows feasibility spike. This does **not** mean the application,
cloud control plane, Bridge, key recovery or production profile lifecycle exists.

Repository Step 0 introduces the first executable workspace and permanent CI.
The product remains pre-functional until later steps merge.

## ADR Status Corrections

- ADR-0001 remains `proposed` and blocks production profile generation and
  certification claims.
- ADR-0002 is accepted with bounded one-device smoke evidence only.
- ADR-0003, ADR-0004 and ADR-0005 are accepted architecture decisions, not
  completed implementation.
- ADR-0006 remains `proposed` and blocks production cloud generations and
  multi-device key delivery.

The authoritative projection is `ADR_STATUS.md` and `status.json`.

## Execution Model

Development is performed as sequential Repository Steps through GitHub branches,
pull requests, permanent workflows and squash merge. Code/tests/evidence available
through GitHub can be completed autonomously. External state is never inferred:
credential rotation, Cloudflare resources, physical Windows hosts, code signing,
offline escrow and legal approval require separate evidence.

The detailed order is `DELIVERY_ROADMAP.md`.

## Repository Step 0 Gate

Step 0 is accepted only when:

- exact Rust `1.97.1` workspace and lockfile build reproducibly;
- formatting, Clippy and tests pass on Linux;
- workspace tests pass on Windows;
- pure domain primitive compiles for `wasm32-unknown-unknown`;
- machine-readable status validates;
- tracked files pass the high-confidence credential pattern check;
- product/security/privacy/evidence documentation is indexed and consistent;
- permanent GitHub workflow is green on the PR.

## Mandatory Later Gates

- revoke/rotate the legacy proxy credential before any prototype reuse;
- perform `workers-rs` cold build and pin exact Cloudflare dependencies;
- complete threat review before processing production data;
- accept ADR-0006 and pass clean-environment key recovery;
- provision separate Cloudflare environments and cost limits;
- prove Bridge lifecycle on an approved Windows host;
- obtain trusted Windows code-signing certificate before stable release;
- use a second independent Windows host for multi-device proof;
- accept ADR-0001 and complete fingerprint certification before promotion;
- accept isolation ADR before adding a second independent tenant;
- accept privacy/retention values and authorized-use policy.

## Residual Risks

- Cloudflare SDK/runtime compatibility remains unproven until Step 1 CI;
- Cloudflare account recovery is part of disaster recovery;
- D1 adapter lacks defense-in-depth RLS;
- production key root/offline escrow is a plan, not evidence;
- physical multi-device behavior is not proven;
- Windows installer, embedded runtime and process supervision remain major
  feasibility risks;
- external fingerprint checkers and provider behavior can drift;
- repository license remains a product-owner decision.

## Verdict

Proceed with Repository Step 0 and then the two highest-risk feasibility lanes:
Cloudflare cold build and Windows Bridge skeleton. Do not begin production cloud
profile handling, claim multi-device support or reuse the leaked credential until
their explicit gates are satisfied.
