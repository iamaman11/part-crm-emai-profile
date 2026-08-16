# Architecture Re-baseline v3 — AR-4B Client Mail route ownership

Status: **EVIDENCE / AR-4B accepted**

Parent program: #266  
Accepted baseline: `c705dc45c9e923582daf531242bd2c6af2239597` (`main`, AR-4A accepted)  
Implementation issue: #282  
Implementation PR: #283  
Exact-green implementation head: `7ccdd1b0ed0c0eae974cd9bde15c87524315c023`  
Implementation merge: `04b62c97813010ac283d8b70c81089f1c16f5672`  
Applicable permanent workflows: **13/13 success**

## Purpose

AR-4B closes the route-classifier ownership debt identified by AR-3 and preserved intentionally through AR-4A.

The remediation is deliberately structural and behavior-neutral: all nested Client Mail HTTP contract routes are classified by `crates/control-plane-contract/src/routes/client_mail.rs`, while `routes/clients.rs` owns only Client capability routes.

## Accepted ownership

`crates/control-plane-contract/src/routes/client_mail.rs` is the single classifier owner for:

- `POST /api/v1/tenants/{tenant_id}/clients/{client_id}/mail/search` -> `ClientMailSearchApi`;
- `POST /api/v1/tenants/{tenant_id}/clients/{client_id}/mail/message` -> `ClientMailMessageApi`;
- `POST /api/v1/tenants/{tenant_id}/clients/{client_id}/mail/send` -> `ClientMailSendApi`.

`crates/control-plane-contract/src/routes/clients.rs` must not contain `ClientMailSearchApi`, `ClientMailMessageApi`, or `ClientMailSendApi` classifier ownership.

## Preserved behavior and authority

AR-4B does **not** change:

- public HTTP paths or methods;
- `RouteClass` identities;
- classifier dispatch order (`client_mail` remains before `clients`);
- authentication classification;
- dynamic-route fail-closed behavior;
- handler/application/provider behavior;
- OpenAPI semantics;
- D1 schema or migration history;
- Cloudflare resources, Wrangler bindings, queues, providers or credentials;
- AR-4C outbound-mail composition scope;
- AR-4D decision (`NOT_REQUIRED`).

## Permanent fitness enforcement

The canonical AR-3/AR-4 architecture checker is updated so that:

1. `routes/client_mail.rs` must contain all three Client Mail route classes;
2. `routes/clients.rs` must not contain any Client Mail route class;
3. a negative self-test injects split ownership back into `routes/clients.rs` and must be rejected;
4. route-module unit tests prove search/message/send are exact and POST-only, including rejection of wrong methods and extra path segments.

This turns route ownership into a fail-closed repository invariant rather than a reviewer convention.

## Accepted machine state

After post-merge authority closeout:

- `accepted_through = AR-4B`;
- accepted status = `ROUTE_OWNERSHIP_CONSOLIDATION_ACCEPTED`;
- next required slice = `AR-4C`;
- `architecture_complete = false`;
- Production Core remains `BLOCKED`;
- `production_ready = false`;
- production/provider mutation remains forbidden.

## Verification contract

The implementation candidate was required to pass, at minimum:

```text
cargo fmt --all -- --check
cargo test --locked -p control-plane-contract
python scripts/generate-architecture-inventory.py --check
python scripts/generate-architecture-inventory.py --self-test
python scripts/check-contract-compatibility.py
```

Acceptance additionally requires the full applicable permanent PR workflow set green on one unchanged exact human-authored head, `behind_by=0`, no blocking reviews, no unresolved review threads, guarded merge, and a post-merge authority closeout before AR-4C begins.

## Non-goals

AR-4B does not perform AR-4C provider/composition extraction, capability activation, production provisioning, production deployment, provider mutation, migration mutation, runtime-topology redesign, or unrelated cleanup.
