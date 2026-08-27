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
5. [Issue #266](https://github.com/iamaman11/part-crm-emai-profile/issues/266) — live transaction pointer
   and accepted-main evidence; from it open the one linked owning Issue for the current transaction.
6. [AGENTS.md](../AGENTS.md) and [CONTRIBUTING.md](../CONTRIBUTING.md) — execution protocol.

## Knowledge owners

| Knowledge | Canonical owner |
|---|---|
| Product scope and first-release promises | [PRODUCT.md](PRODUCT.md) |
| Runtime topology, layers, context/data ownership | [ARCHITECTURE.md](ARCHITECTURE.md) |
| Mandatory architecture invariants | [APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md](APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md) |
| Architecture change/simplification quality | [ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md](ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md) |
| Temporary ordered execution | [ARCHITECTURE_REBASELINE_V3_PLAN.md](ARCHITECTURE_REBASELINE_V3_PLAN.md) |
| Live program position and accepted-main summary | [Issue #266](https://github.com/iamaman11/part-crm-emai-profile/issues/266) |
| Bounded transaction record, change envelope and evidence | Exactly one current owning Issue linked from [Issue #266](https://github.com/iamaman11/part-crm-emai-profile/issues/266); durable provenance after close |
| Research coverage and decisions | [CAP-INDEX #505](https://github.com/iamaman11/part-crm-emai-profile/issues/505) and owning CAP Issues |
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
- **Live state** lives in protected Git/GitHub/provider owners. Issue #266 is the sole live transaction
  pointer; the one linked owning Issue contains bounded discovery/change/acceptance evidence and becomes
  provenance after close; exact environment and candidate evidence lives with its owning R-stage evidence.
- **Projection/navigation** may summarize canonical owners but cannot create status, readiness, order
  or authority. `DEVELOPMENT_PLAN.md`, generated status JSON and capability/evidence matrices are in
  this class unless a listed bounded contract explicitly assigns them a narrower role.
- **History/provenance** explains accepted decisions and evidence but cannot select work or authorize
  runtime, staging or Production effects.

The program sequence is intentionally not duplicated here. Read the current execution program for its
meaning and fresh #266 for the single active transaction.

## Bounded contracts

Use the natural owner's document when a transaction touches that boundary:

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

Generated JSON/status files are projections only. Existing executable consumers of `status.json` are
transition debt owned by an explicit E4/V1 cutover: the file must not be deleted until
`old_current_callers=0` and `old_unique_current_invariants=0`, and its existence does not make it
current factual or Production authority.

Documents named for completed Phase, Pre-2J, AR, PF, PAS or Functional Closure work are historical
provenance unless this index explicitly lists them as a current bounded contract. Historical documents
may preserve evidence and rationale; they do not select the next transaction.

`architecture/architecture-program-sequence.json` and its evaluator are the frozen AR-program
acceptance model, not the CAP execution sequence or active-transaction owner. Any retirement of their
remaining executable consumers belongs to a separately accepted CAP-05/CAP-06 transaction.
