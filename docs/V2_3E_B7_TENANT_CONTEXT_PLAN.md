# V2.3E / B7 — authenticated tenant-context closure plan

**Document status:** CURRENT-STAGE REFINEMENT ONLY  
**Owning stage:** V2 / Issue #584 while Issue #266 selects V2  
**Purpose:** remove the discovered B7 user-journey prerequisite without creating a second identity, authorization, tenant, device or execution owner.

This document refines only the current B7 acceptance work. It does not change the global execution order in `ARCHITECTURE_REBASELINE_V3_PLAN.md`, does not authorize provider or Production mutation, and must not become a second roadmap.

## 1. Problem discovered during real B7 execution

The real staging journey reached the repository-local Profile Operations UI through Cloudflare Access, but the normal browser flow still requires a raw `Tenant ID` before authorized Profile/Client projections can be used.

Current observed behavior is insufficient for an ordinary external-user B7 journey:

- Cloudflare Access authenticates the human successfully;
- the UI exposes an operator-style raw `Tenant ID` control;
- no authoritative B7 staging tenant identifier is durably discoverable from the current acceptance authority;
- the tenant must not be guessed, probed or substituted with infrastructure UUIDs;
- when no tenant is selected, data fetching may be disabled while the UI can still present a misleading loading state;
- therefore the B7 normal-user path cannot currently proceed deterministically from authenticated human identity to application tenant context.

This is a current user-journey/source prerequisite, not permission to add a new identity system or a test-only bypass.

## 2. Binding simplification decision

The normal first-release user flow must not require knowledge of an internal tenant UUID.

The natural chain is:

```text
Cloudflare Access human identity
-> existing application actor identity
-> existing ACTIVE membership authority
-> current tenant context
-> existing Client/Profile UI
-> existing Connect this computer flow
```

One fact keeps one owner:

- **Cloudflare Access** owns initial human authentication at the edge;
- **existing backend identity/membership authority** owns actor, membership and tenant authorization semantics;
- **frontend** consumes a read-only current context and renders deterministic bootstrap states;
- **Profile Bridge / Windows CNG device authority** remains unchanged and begins only after the authorized UI pairing action.

No new tenant registry, identity service, policy engine, session owner or device authority is permitted.

## 3. Required user behavior

After successful human authentication, the application resolves the caller's authorized current context through existing backend authority.

```text
0 ACTIVE memberships
-> explicit no-organization/no-access state

1 ACTIVE membership
-> select that tenant automatically
-> continue to normal Client/Profile UI

N ACTIVE memberships
-> present an explicit organization choice using human-readable application identity
-> raw internal UUID is not the normal user-facing selector

backend/auth/context failure
-> explicit bounded retry/error state
-> never infinite or misleading loading
```

A raw `Tenant ID` field may remain only where a repository-local/operator/debug surface still has an independently justified use. It is not part of B7's ordinary external-user journey.

## 4. Smallest implementation transaction

Implementation starts with a fresh source check. Do not write a duplicate API merely because the current UI does not use one.

### Gate A — discover existing owner

Inspect current protected source for a read-only route/use case that already projects one or more of:

- authenticated application actor;
- ACTIVE memberships;
- authorized tenant/application context.

If an adequate current-context projection already exists, **reuse it** and change only frontend wiring/state handling plus focused tests.

### Gate B — only if the projection is absent

Add the smallest read-only projection in the existing backend identity/membership natural owner. The route may expose a current-context read model, but it must not become a second semantic owner.

Conceptual shape only; exact contract naming follows existing repository conventions:

```text
current authenticated actor
+ authorized ACTIVE memberships
+ enough tenant display identity for deterministic user selection
= current application context
```

Do not add a mutable selected-tenant database merely to implement this flow. Selection is request/session/UI context unless an existing application owner already defines otherwise.

### Gate C — frontend bootstrap

The frontend must distinguish at least:

1. resolving current context;
2. no authorized membership;
3. one automatically selected tenant;
4. multiple authorized tenants requiring explicit human-readable selection;
5. retryable context/backend failure;
6. authorized resource loading after tenant context is known.

A missing tenant must not be rendered as an indefinite resource-loading state.

## 5. Verification

Focused tests must cover the existing natural-owner semantics, not a synthetic B7 backend:

- unauthenticated / untrusted human identity is rejected by the existing boundary;
- authenticated actor with zero ACTIVE memberships obtains no tenant context;
- exactly one ACTIVE membership resolves deterministically;
- multiple ACTIVE memberships return only authorized choices and require explicit selection;
- inactive/revoked membership is never selected;
- backend/context failure produces a bounded frontend error state rather than endless loading;
- normal authorized Client/Profile data requests begin only after tenant context is resolved;
- `Connect this computer` continues to use the existing pairing/device/application-session path unchanged.

Existing contract, architecture and negative checks remain authoritative. Add only the narrowest new tests needed for this behavior.

## 6. Candidate and B7 evidence rule

Any source change made to close this prerequisite creates a new source identity and therefore a new exact candidate for B7.

Do not preserve or relabel the previous B7 candidate through a compatibility shim.

After the bounded change is accepted on protected `main`:

```text
fresh exact candidate
-> required candidate-bound build/deploy/evidence
-> real Windows B7 from the beginning
-> Cloudflare Access human login where required
-> authorized Client/Profile UI
-> Connect this computer
-> non-exportable CNG P-256 device proof
-> device claim / application-session
-> real Camoufox launch
-> deterministic browser-state mutation
-> controlled save
-> full Bridge restart
-> fresh application-session via persisted CNG proof
-> reopen exact authoritative state
-> logout
```

Only genuine same-path behavioral PASS can complete B7.

## 7. Hard boundaries / non-goals

This transaction does **not** authorize:

- Cloudflare Access policy/group mutation;
- D1 provider mutation or schema expansion;
- Worker promotion by itself;
- Production mutation;
- service-token or mTLS machine authority;
- a second human-authentication layer;
- a second tenant/membership owner;
- a tenant registry or mutable status database;
- changes to Windows CNG key ownership;
- changes to device claim, device proof or application-session authority;
- a test-only identity/effect backend;
- hard-coded or guessed staging tenant IDs;
- unrelated cleanup or broad application refactoring.

If implementation discovers that provider state must change, stop at that boundary and require a separate exact authorization before any effect.

## 8. Definition of Done for this prerequisite

This prerequisite is complete only when all of the following are true:

1. normal authenticated users do not need to know or type an internal tenant UUID;
2. actor -> ACTIVE membership -> authorized tenant context is resolved by the existing application authority;
3. 0/1/N membership behavior is deterministic and tested;
4. missing context and backend failures render explicit states, not misleading infinite loading;
5. frontend uses the resolved authorized context for normal Client/Profile flows;
6. Profile Bridge/CNG/device/application-session authority remains unchanged;
7. no second semantic, identity, tenant, provider or mutation owner was introduced;
8. protected-main acceptance produces a fresh exact candidate;
9. B7 is rerun from the beginning on that exact candidate;
10. #584 is completed only by real B7 behavioral evidence, after which #266 may advance.

The preferred implementation is always the smaller of:

```text
reuse existing current-context projection + frontend wiring
```

or, only if proven absent:

```text
one minimal read-only projection in the existing backend owner + frontend wiring
```

No additional architecture is justified by this prerequisite.
