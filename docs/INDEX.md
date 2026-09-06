# Documentation Authority Index

This index classifies documentation by authority. It does not store mutable execution, provider,
workflow or readiness state.

## Start path

1. [Product definition](PRODUCT.md) — what the product is and what the first release promises.
2. [Architecture](ARCHITECTURE.md) — runtime topology, layers and data ownership.
3. [Mandatory architecture requirements](APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md) —
   prospective invariants for every change.
4. [Current CAP execution program](ARCHITECTURE_REBASELINE_V3_PLAN.md) — the single ordered
   implementation program.
5. [Issue #266](https://github.com/iamaman11/part-crm-emai-profile/issues/266) — the **sole live stage
   pointer**; from it open exactly one owning Issue for the CURRENT stage.
6. [AGENTS.md](../AGENTS.md) and [CONTRIBUTING.md](../CONTRIBUTING.md) — execution protocol.

For recovery after complete chat/context loss, reference Issue
[#625](https://github.com/iamaman11/part-crm-emai-profile/issues/625) explains how these owners compose.
It is orientation/provenance only: it is not a CURRENT stage, live pointer, roadmap or authorization
owner.

## Knowledge owners

| Knowledge | Canonical owner |
|---|---|
| Product scope and first-release promises | [PRODUCT.md](PRODUCT.md) |
| Runtime topology, layers, context/data ownership | [ARCHITECTURE.md](ARCHITECTURE.md) |
| Mandatory architecture invariants | [APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md](APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md) |
| Architecture change/simplification quality | [ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md](ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md) |
| Temporary ordered execution | [ARCHITECTURE_REBASELINE_V3_PLAN.md](ARCHITECTURE_REBASELINE_V3_PLAN.md) |
| Live stage position and minimal accepted-main summary | [Issue #266](https://github.com/iamaman11/part-crm-emai-profile/issues/266) |
| CURRENT stage objective, change envelope and evidence | Exactly one CURRENT stage Issue selected and linked from [Issue #266](https://github.com/iamaman11/part-crm-emai-profile/issues/266); durable provenance after completion/close |
| Execution-model orientation after context loss | [Reference Issue #625](https://github.com/iamaman11/part-crm-emai-profile/issues/625) — explanatory only, never authority |
| Research coverage and decisions | [CAP-INDEX #505](https://github.com/iamaman11/part-crm-emai-profile/issues/505) and owning CAP Issues |
| Non-active future product-evolution options | [FUTURE_DEVELOPMENT.md](FUTURE_DEVELOPMENT.md) — never execution authority or `NEXT` |
| Public contract evolution | [CONTRACT_POLICY.md](CONTRACT_POLICY.md) |
| Exact-head acceptance | [ARCHITECTURE_ACCEPTANCE_PROTOCOL.md](ARCHITECTURE_ACCEPTANCE_PROTOCOL.md) |
| Data classification/retention | [DATA_CLASSIFICATION.md](DATA_CLASSIFICATION.md), [PRIVACY_AND_RETENTION.md](PRIVACY_AND_RETENTION.md) |
| Security and threats | [THREAT_MODEL.md](THREAT_MODEL.md) |
| `opsctl` role/effects | [OPSCTL_ARCHITECTURE_BOUNDARY.md](OPSCTL_ARCHITECTURE_BOUNDARY.md), [OPSCTL_DOCTOR_CONTRACT.md](OPSCTL_DOCTOR_CONTRACT.md) |
| Python role/effects | [PYTHON_USAGE_BOUNDARY.md](PYTHON_USAGE_BOUNDARY.md) |
| Developer workflow and local setup | [CONTRIBUTING.md](../CONTRIBUTING.md) plus component-local README files |
| Developer projection | [DEVELOPMENT_PLAN.md](DEVELOPMENT_PLAN.md) — navigation only |

## Authority classes

- **Permanent normative knowledge** lives in the natural owners listed above and changes only when its
  product, architecture, security, contract or lifecycle meaning changes.
- **Temporary execution authority** lives only in the CAP execution program. It owns order and gates,
  not mutable completion state.
- **Live state** lives in protected Git/GitHub/provider owners. Issue #266 is the sole live stage
  pointer; the exactly one linked CURRENT stage Issue contains bounded discovery/change/acceptance
  evidence and becomes provenance after completion; exact environment and candidate evidence lives
  with its natural stage/evidence owner.
- **Future product options** may be recorded in `FUTURE_DEVELOPMENT.md`, but they cannot select work,
  create `NEXT`, authorize a provider mutation or justify pre-creating an execution Issue. They are
  reconsidered from then-current accepted `main` only when #266 selects that concern as a CURRENT stage.
- **Projection/navigation/reference** may summarize canonical owners but cannot create status,
  readiness, order, CURRENT-stage selection or authorization. `DEVELOPMENT_PLAN.md`, reference Issue
  #625, generated status JSON and capability/evidence matrices are in this class unless a listed bounded
  contract explicitly assigns a narrower role.
- **History/provenance** explains accepted decisions and evidence but cannot select work or authorize
  runtime, staging or Production effects. An open historical Issue is still non-current unless #266
  explicitly selects it as the one CURRENT stage Issue.

The program sequence is intentionally not duplicated here. Read the current execution program for its
meaning and fresh #266 for the single CURRENT stage.

## Bounded contracts

Use the natural owner's document when a transaction inside the CURRENT stage touches that boundary:

- [UI architecture](UI_ARCHITECTURE.md)
- [Client application boundary](CLIENT_APPLICATION_BOUNDARY.md)
- [Profile application boundary](PROFILE_APPLICATION_BOUNDARY.md)
- [Profile generation application boundary](PROFILE_GENERATION_APPLICATION_BOUNDARY.md)
- [Local Profile lifecycle](LOCAL_PROFILE_LIFECYCLE.md)
- [Encrypted cloud generations](ENCRYPTED_CLOUD_GENERATIONS.md)
- [Profile coordinator](PROFILE_COORDINATOR.md)
- [Certification and multi-device](CERTIFICATION_MULTI_DEVICE.md)
- [Mailbox binding](MAILBOX_BINDING_APPLICATION_BOUNDARY.md)
- [Mailbox jobs](MAILBOX_JOB_APPLICATION_BOUNDARY.md)
- [Realtime notifications](REALTIME_NOTIFICATIONS.md)
- [D1 catalog](D1_CATALOG.md)
- [Accepted immutable D3 resolver/bootstrap boundary](PRE2J_D3_RESOLVER_BOOTSTRAP_AUTHORITY.md) —
  retained because its repository checker still enforces the accepted boundary; it does not authorize
  a fresh provider mutation or select current work.

## Projection and history rules

Generated JSON/status files are projections only and cannot create current factual, readiness or
Production authority. The E4/V1 `status.json` transition debt has been retired after its executable
callers were removed and its only readiness invariant remained in the stronger typed external-evidence
owner; do not recreate a mutable current-state status mirror.

Documents named for completed Phase, Pre-2J, AR, PF, PAS or Functional Closure work are historical
provenance unless this index explicitly lists them as a current bounded contract. Historical documents
may preserve evidence and rationale; they do not select the next stage.

`architecture/architecture-program-sequence.json` and its evaluator are the frozen AR-program
acceptance model, not the CAP execution sequence or CURRENT-stage owner. Any retirement of their
remaining executable consumers belongs to a separately accepted CAP-05/CAP-06 transaction inside a
stage selected by #266.
