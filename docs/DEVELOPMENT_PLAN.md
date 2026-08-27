# Browser Profile Platform — Developer Projection

**Role:** navigation/projection only

**Execution authority:** [ARCHITECTURE_REBASELINE_V3_PLAN.md](ARCHITECTURE_REBASELINE_V3_PLAN.md) +
[Issue #266](https://github.com/iamaman11/part-crm-emai-profile/issues/266)

This file helps a developer find the right change path. It does not own mutable status, a current SHA,
workflow/provider state or architecture decisions.

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

## Delivery order

```text
E0 -> E1 -> E2 -> E3 -> E4
-> P1 -> P2 -> P3
-> V1 -> V2
-> R1 -> R2 -> R3
```

| ID | Developer outcome |
|---|---|
| E0 | One current documentation/execution authority. |
| E1 | Every governed Worker ingress consumes canonical Capability Policy surface mapping. |
| E2 | Release/promotion outer JSON is admitted strictly before typed decode. |
| E3 | D1 outer JSON is admitted strictly before typed decode. |
| E4 | First proven completed/duplicate verification authority is retired without invariant loss. |
| P1 | Client card owns the complete visible Client/Profile relationship workflow. |
| P2 | One authorized shipping path launches real Camoufox through Profile Bridge. |
| P3 | Save is confirmed only after authoritative verified generation commit; reopen uses it. |
| V1 | Exact reachable release verification runs at the accepted risk tiers. |
| V2 | The complete CAP-12 scenario and negative/recovery cases pass on one candidate. |
| R1 | Exact release/environment/capability/evidence envelope exists. |
| R2 | A bounded pilot is explicitly decided with stop/recovery conditions. |
| R3 | A named authority grants GO/PILOT or NO-GO for that unchanged candidate. |

Do not infer the active row from this projection. Read the live pointer in #266 and the row's owning
Issue after refreshing protected `main`.

## How to change the system

1. Identify the bounded context and natural semantic/data/execution owner.
2. Determine whether the change is internal or changes a public/generated contract.
3. Enumerate callers, persisted obligations, side effects and the path being replaced.
4. Create or use one bounded Issue with explicit exit criteria and non-goals.
5. Implement one coherent vertical change; cut over callers and remove the predecessor.
6. Run focused positive and negative proof at the cheapest sufficient CAP-05 tier.
7. Accept only one unchanged exact PR head under protected governance.
8. Reread accepted `main` before selecting another concern.

A new provider of an existing responsibility normally adds an owner-local adapter/configuration,
persistence migration only when data changes, a public contract only when exposed, capability admission
only when independently controlled, and focused tests. It is not automatically a new bounded context or
a reason for a plugin framework.

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
