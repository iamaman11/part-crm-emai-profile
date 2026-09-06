# Browser Profile Platform — Developer Projection

**Role:** navigation/projection only

**Execution authority:** [ARCHITECTURE_REBASELINE_V3_PLAN.md](ARCHITECTURE_REBASELINE_V3_PLAN.md) +
[Issue #266](https://github.com/iamaman11/part-crm-emai-profile/issues/266)

This file helps a developer find the right change path. It does not own mutable status, a current SHA,
workflow/provider state, CURRENT-stage selection or architecture decisions.

## Product slice

```text
managed login
-> Client create/view/edit
-> persistent Browser Profile create
-> attach/detach/reassign
-> launch from Client card through the shipping Bridge
-> controlled close and confirmed successor generation
-> reopen confirmed state
-> logout
```

The first slice excludes native CRM passwords, Mailboxes, Notifications, Automation, new providers,
tenant-wide Audit, global Sessions UI, complex roles, mobile parity and generic export.

## Delivery navigation

The ordered stages and their exit criteria live only in the
[execution program](ARCHITECTURE_REBASELINE_V3_PLAN.md). This projection records the user-facing path
and development method, not a second stage table. Read fresh #266 and follow it to exactly one CURRENT
stage Issue after refreshing protected `main`; never infer the active stage from this file, a historical
Issue, or a chat handoff.

## How to change the system

1. Read fresh protected `main`, #266 and the one CURRENT stage Issue.
2. Identify the bounded context and natural semantic/data/execution owner for the stage's current concern.
3. Determine whether the change is internal or changes a public/generated contract.
4. Enumerate callers, persisted obligations, side effects and the path being replaced.
5. Execute one bounded implementation transaction inside the CURRENT stage; do not create another stage Issue for a PR-sized concern.
6. Implement one coherent vertical change; cut over callers and remove the predecessor.
7. Run focused positive and negative proof at the cheapest sufficient CAP-05 tier.
8. Accept only one unchanged exact PR head under protected governance.
9. Reread accepted `main`; update the CURRENT stage Issue and #266 only when the evidence justifies it.

A new provider of an existing responsibility normally adds an owner-local adapter/configuration,
persistence migration only when data changes, a public contract only when exposed, capability admission
only when independently controlled, and focused tests. It is not automatically a new bounded context or
a reason for a plugin framework.

A new stage is justified only when work has an independent objective/DoD or a genuine acceptance or
authority boundary. Future-stage Issues are not pre-created.

## Completion levels

```text
CODE_COMPLETE
  implementation and local/integration proof are complete

SCENARIO_COMPLETE
  accepted user flow and required negative/recovery cases pass on one exact non-Production candidate

PRODUCTION_AUTHORIZED
  CAP-08 exact-candidate authority separately issues GO/PILOT
```

One level never implies the next.

## Canonical references

- [Product definition](PRODUCT.md)
- [Architecture](ARCHITECTURE.md)
- [Mandatory architecture requirements](APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md)
- [Architecture evolution quality contract](ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md)
- [Current execution program](ARCHITECTURE_REBASELINE_V3_PLAN.md)
- [Architecture acceptance protocol](ARCHITECTURE_ACCEPTANCE_PROTOCOL.md)
- [Contract policy](CONTRACT_POLICY.md)
- [Documentation index](INDEX.md)
- [Context-loss governance reference #625](https://github.com/iamaman11/part-crm-emai-profile/issues/625) — orientation only, never authority
