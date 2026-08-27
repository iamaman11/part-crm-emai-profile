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
   and accepted-main evidence.
6. [AGENTS.md](../AGENTS.md) and [CONTRIBUTING.md](../CONTRIBUTING.md) — execution protocol.

## Knowledge owners

| Knowledge | Canonical owner |
|---|---|
| Product scope and first-release promises | [PRODUCT.md](PRODUCT.md) |
| Runtime topology, layers, context/data ownership | [ARCHITECTURE.md](ARCHITECTURE.md) |
| Mandatory architecture invariants | [APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md](APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md) |
| Architecture change/simplification quality | [ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md](ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md) |
| Temporary ordered execution | [ARCHITECTURE_REBASELINE_V3_PLAN.md](ARCHITECTURE_REBASELINE_V3_PLAN.md) |
| Live active transaction and accepted evidence | [Issue #266](https://github.com/iamaman11/part-crm-emai-profile/issues/266) |
| Research coverage and decisions | [CAP-INDEX #505](https://github.com/iamaman11/part-crm-emai-profile/issues/505) and owning CAP Issues |
| Public contract evolution | [CONTRACT_POLICY.md](CONTRACT_POLICY.md) |
| Exact-head acceptance | [ARCHITECTURE_ACCEPTANCE_PROTOCOL.md](ARCHITECTURE_ACCEPTANCE_PROTOCOL.md) |
| Data classification/retention | [DATA_CLASSIFICATION.md](DATA_CLASSIFICATION.md), [PRIVACY_AND_RETENTION.md](PRIVACY_AND_RETENTION.md) |
| Security and threats | [THREAT_MODEL.md](THREAT_MODEL.md) |
| `opsctl` role/effects | [OPSCTL_ARCHITECTURE_BOUNDARY.md](OPSCTL_ARCHITECTURE_BOUNDARY.md), [OPSCTL_DOCTOR_CONTRACT.md](OPSCTL_DOCTOR_CONTRACT.md) |
| Python role/effects | [PYTHON_USAGE_BOUNDARY.md](PYTHON_USAGE_BOUNDARY.md) |
| Developer projection | [DEVELOPMENT_PLAN.md](DEVELOPMENT_PLAN.md) |

## Ordered program

```text
E0 -> E1 -> E2 -> E3 -> E4
-> P1 -> P2 -> P3
-> V1 -> V2
-> R1 -> R2 -> R3
```

The meaning, gates and exit criteria of these IDs live only in the current execution program. Their
active/completed state lives only in #266.

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

## Projection and history rules

Generated JSON/status files are projections only where their owning contract says so. A projection
cannot promote readiness or become semantic input merely because it is tracked.

Documents named for completed Phase, Pre-2J, AR, PF, PAS or Functional Closure work are historical
provenance unless this index explicitly lists them as a current bounded contract. Historical documents
may preserve evidence and rationale; they do not select the next transaction.

`architecture/architecture-program-sequence.json` and its evaluator are the frozen AR-program
acceptance model, not the CAP execution sequence or active-transaction owner. Any retirement of their
remaining executable consumers belongs to a separately accepted CAP-05/CAP-06 transaction.
