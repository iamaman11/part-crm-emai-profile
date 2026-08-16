# Architecture Re-baseline v3 — AR-6 Full Python Estate + read-only Rust opsctl

**Document status:** EVIDENCE / AR-6 accepted  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Tracking:** #266 / implementation #294 / closeout #296  
**Implementation PR:** #295  
**Exact-green implementation head:** `9b06d542873ffa3122e53e107105098e21f5933c`  
**Accepted implementation merge:** `d0229fedd81ee870822b6d9394bc4ee313ea3a3c`  
**Applicable permanent workflows:** **13/13 success** on the unchanged exact head  
**Production mutation:** forbidden

## 1. Purpose

AR-6 establishes one fail-closed disposition for the complete Git-tracked Python estate and a bounded Rust operator-tool foundation without authorizing a global Python-to-Rust rewrite. It also establishes `tools/opsctl` as project-specific operator tooling while keeping application/domain/runtime code independent of it.

AR-6 is an operational/tooling authority dimension. It does not pretend that application architecture advanced beyond the accepted AR-4C remediation, and it does not replace the accepted AR-5 Wrangler/runtime-authority cleanup.

## 2. Accepted Python estate

`architecture/python-estate-ar6.json` is the accepted machine-readable disposition of all **116** Git-tracked Python files. Every tracked file is AST-parsed and semantically capability-audited; unknown tracked Python fails closed.

Accepted classification summary:

- `KEEP_PYTHON`: **108**;
- `MIGRATE_TO_RUST`: **2**;
- `DELETE_AFTER_SEQUENCE`: **6**;
- `WRAP_WITH_RUST`: **0**.

The two AR-11 migrations are the legacy D3 promotion entrypoint/core and target the future `opsctl` release/promotion command family. The six AR-10 retirements are retired markers or direct legacy browser/profile/R2 executables whose removal requires their recorded compatibility and retirement proofs.

The semantic audit deliberately rejected filename-only trust. It initially surfaced `scripts/check-external-review-attestations.py` because it performs bounded GitHub API reads and can consume a workflow-scoped token. Manual review confirmed that it is a read-only attestation verifier with no mutation surface, so it is an explicit `KEEP_PYTHON` exception rather than an implicit `check-*` allowance.

## 3. Accepted opsctl boundary

`tools/opsctl` is a standalone, dependency-free Rust workspace. AR-6 accepts exactly these commands:

- `doctor` — execute the two canonical Python repository validators in read-only `--check` mode;
- `status` — return canonical `docs/status.json`;
- `inventory` — return canonical `architecture/inventory.json`.

Permanent checks reject mutation commands, additional process-spawn sites, filesystem/environment mutation capabilities, network/provider/database/secret clients, and third-party Rust dependencies. `opsctl` remains an operator interface, not application business logic and not a competing state registry.

GitHub Actions / Environments remain orchestration, approval, concurrency and credential boundaries. Wrangler/provider APIs remain the eventual low-level provider mutation executors when a later owning slice explicitly authorizes mutation.

## 4. Cross-platform evidence and finding

The exact AR-6 candidate built and tested `opsctl` on Linux and Windows and executed `doctor`, `status`, and `inventory` on Windows. The first Windows execution correctly exposed a pre-existing portability defect in canonical documentation authority: repository-relative machine identities were compared using platform-native `str(Path(...))`.

AR-6 fixed the canonical validator rather than adding an `opsctl` workaround: repository identities now use POSIX representation through `Path.as_posix()`. The final unchanged exact head then passed real `opsctl.exe doctor/status/inventory` execution in the permanent Windows Local Profile Gate.

## 5. Accepted evidence

- implementation issue: #294;
- implementation PR: #295;
- exact-green implementation head: `9b06d542873ffa3122e53e107105098e21f5933c`;
- accepted implementation merge: `d0229fedd81ee870822b6d9394bc4ee313ea3a3c`;
- permanent PR workflows: **13/13 success**;
- implementation branch at acceptance: `behind_by=0`;
- unresolved review threads: **0**;
- blocking reviews: **0**;
- PR Conversation comments: **0**;
- no production/provider/customer-state mutation.

The semantic remediation strengthened the initial candidate before acceptance: the final inventory is **108 KEEP / 2 MIGRATE / 6 DELETE / 0 WRAP**, not the earlier mechanical 111/2/3/0 classification.

## 6. Preserved invariants and handoff

After this mandatory post-merge authority closeout:

```text
accepted architecture checkpoint = AR-6
python / operator-tool authority = ACCEPTED
application architecture = ACCEPTED_THROUGH_AR4C
runtime authority cleanup = ACCEPTED_AR5
AR-4D = NOT_REQUIRED
next slice = AR-7 — Environments + GitHub Governance + Operational Boundaries
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
production_mutation = false
```

AR-7 must start from the accepted AR-6 main state. It may harden environments/governance/operational boundaries, but it must not silently widen `opsctl` into a second provider mutation authority or reclassify Python outside the accepted estate without updating the canonical AR-6 disposition and its gates.
