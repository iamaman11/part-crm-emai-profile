# Threat Model

**Status:** canonical current repository-local threat model  
**Production authorization:** not granted by this document  
**Method:** trust-boundary and STRIDE-oriented analysis

## 1. Protected assets

- browser Profile generations: cookies, login databases, localStorage, IndexedDB and materialized state;
- Profile entropy/fingerprint/network identity policy and observations;
- mailbox, proxy and OAuth secret handles;
- root wrapping keys, tenant KEK, generation DEK and device private keys;
- memberships, grants, Client/contact records and historical Profile assignments;
- bounded one-time Profile launch authorities and their digest-bound metadata;
- launch intents, durable jobs, leases, fencing tokens, sessions and realtime cursors;
- runtime/Bridge installers, signatures, update metadata and immutable generation objects;
- audit, support, operational evidence, backup and recovery artifacts.

## 2. Trust boundaries

1. Browser user -> Cloudflare Access -> verified application actor.
2. Verified actor -> live tenant membership/capability/grant authorization.
3. React SPA -> Rust Control Plane Worker API; UI state is never authorization authority.
4. Authorized Profile launch -> server-resolved active device/generation -> bounded one-time launch
   authority; browser does not select trusted device/generation.
5. `profilebridge://claim/<opaque-code>` -> Windows Profile Bridge -> dedicated machine-authenticated
   Worker perimeter; raw claim alone is insufficient to establish machine authority.
6. Cloudflare edge-verified mTLS certificate identity -> existing active device-principal binding ->
   fresh launch redemption authorization/state revalidation -> atomic one-time claim consumption.
7. Machine-authenticated Bridge -> Profile coordinator exact launch-intent claim -> session/epoch/fence.
8. Bridge -> local workspace writer lock -> pinned materialization/runtime preflight -> supervised
   Camouhost/Camoufox process.
9. Worker application orchestration -> D1/R2/Queue/Durable Objects through outer adapters.
10. Mailbox application ports -> cloud provider adapters or browser/device execution lane.
11. Immutable encrypted generation -> exact verification -> authoritative catalog activation.
12. Durable notification/realtime event -> current authorization -> metadata-only invalidation -> refetch.
13. Operators -> Cloudflare account, signing material, recovery escrow and production rollout controls.
14. Future CRM -> versioned contracts only; never direct Profile/mailbox authority.

## 3. Accepted Repository-Local Controls

| Threat | Accepted repository-local control | Permanent evidence class |
|---|---|---|
| Cross-tenant / IDOR access | live membership/capability/grant checks before projection/provider/device/realtime access; neutral denial | identity/query/application boundary gates and cross-component acceptance |
| Result-count / existence disclosure | foreign and absent resources are public-response neutral; denied paths return no foreign projections/counts | query/transport negative fixtures |
| Client assignment becomes ACL | relationship and Profile authorization remain independent; inverse Client->Profiles read independently authorizes every returned Profile | relationship/application negative acceptance |
| Browser chooses trusted launch target | public `launchProfile` request carries no trusted device/generation selector; server resolves both from authoritative state | OpenAPI/generated frontend + launch use-case tests |
| Launch claim stolen or replayed | short bounded TTL, raw claim never persisted server-side, digest-only lookup, exact actor/Profile/device/generation binding and atomic one-time consume | launch-authority D1/use-case replay/concurrency tests |
| Claim remains usable after revoke/state change | redemption rechecks active membership, Profile ACL, exact active device/generation and execution preconditions immediately before consume | launch redemption TOCTOU negative tests |
| Wrong machine redeems valid claim | dedicated Bridge perimeter + edge-verified mTLS fingerprint resolved through existing active device-principal owner; returned device must match local machine identity | machine ingress + native shipping adapter tests |
| Human browser credential becomes desktop credential | shipping Bridge does not persist/reuse a human Access bearer and introduces no static launch bearer | Bridge boundary/source and machine-ingress checks |
| Claim leaks through logs/argv/telemetry | redacted claim type; narrow transport-only exposure; native HTTP body supplied through stdin; generic errors and no claim-bearing evidence | bridge-domain/native/shipping acceptance tests |
| Concurrent or stale writer | coordinator launch-intent claim, expected-version sequencing, lease epoch/fencing and local atomic writer lock | coordinator/operator/local lifecycle tests |
| Lost coordinator ownership leaves browser running | exact heartbeat checks session/epoch/fence; heartbeat/process failure force-terminates runtime and enters recovery cleanup | operator heartbeat negative tests |
| Runtime/materialization substitution | selected runtime version/inventory/entrypoint bytes must match exact redeemed generation materialization identity before process start | runtime bundle/preflight + real Camoufox gates |
| Revoked actor receives realtime data | current authorization before catch-up/live delivery; durable cursor semantics | notification/realtime policy/tests |
| Realtime becomes business authority | metadata-only invalidation followed by authorized refetch; no direct business query mutation | frontend realtime policy/self-tests |
| Duplicate/replayed command | idempotency receipts, replay neutrality and atomic governed mutation envelopes | D1/application/mailbox/device tests |
| Unverified/corrupt generation becomes active | immutable candidate, exact verification, quarantine/fail-closed parsing, then activation | profile-generation/encrypted-generation/R2 gates |
| Failed remote commit destroys recoverable local state | retained dirty/operator-owned state until verified remote commit | Bridge/materialization recovery tests |
| Provider outage/auth expiry reported as success | explicit retry/auth-required/suspended/failed durable states | mailbox application failure tests |
| Offline/busy device reported as success | durable retry/remediation state and bounded claims; no false completion | device domain/application tests |
| Corrupt backup/restore | point-in-time restore plus schema/data/integrity validation | recovery/DR drills |
| Sensitive/high-cardinality telemetry | metadata/class-only dimensions and explicit forbidden identifier/content classes | operational-bounds negative policy |
| Sensitive support evidence | allowlist-only support fields and sanitizer/forbidden-data policy | support-bundle negative policy |
| Dependency/CI source substitution | exact dependency locks, approved sources and SHA-pinned permanent actions | supply-chain/license/runtime policies |
| Malicious archive/path escape | safe paths, bounded extraction and deterministic inventory | runtime bundle/materialization gates |
| Browser/runtime command abuse | typed bounded IPC/capability allowlists; no generic privileged command channel | Bridge/runtime contract gates |

### Current device-authorization authority

Production device authorization semantics are owned by `device-domain`, application/use-case
orchestration and the D1/device-principal persistence boundary. Certification remains evidence, not a
second runtime authorization owner.

For P2 machine ingress, the mTLS certificate fingerprint is evidence used to resolve the existing device
principal. The certificate fingerprint or Bridge route does not become an independent device ACL model.

## 4. Authorized Launch Security Invariants

The accepted launch security chain is:

```text
verified browser actor
-> Profile authorization
-> server-owned active device + generation
-> device authorization + execution preconditions
-> bounded one-time claim (digest persisted, raw code returned once)

actual Bridge machine
-> dedicated machine perimeter + verified mTLS identity
-> digest lookup
-> current actor/Profile/device/generation/readiness revalidation
-> atomic consume
-> exact launchIntentId coordinator claim
-> local writer lock + pinned runtime
```

Non-negotiable invariants:

- raw claim possession alone never proves device identity;
- issuance never freezes authorization so that later revocation can be bypassed;
- claim consumption happens only after current-state revalidation;
- one claim has at most one successful redemption under concurrency;
- UI/public launch cannot inject trusted device/generation identity;
- coordinator claim requires the exact server-issued launch intent;
- machine heartbeat/release applies only to its exact current session/epoch/fence;
- local browser launch happens only after distributed and local ownership are both established;
- no second launcher, fallback machine credential or synthetic production runtime may bypass this chain.

## 5. General Fail-Closed Rules

- Unknown membership, grant, device, runtime, mailbox or generation state denies access.
- Foreign and absent resources produce accepted neutral public denial behavior.
- Authorization precedes projection, provider, device and realtime access.
- Unverified/corrupt/quarantined generation cannot become authoritative.
- Dirty or recovery-required local state is not silently evicted or overwritten.
- Expired/stale fencing, claim, generation or session state cannot write newer authority.
- Missing key/recovery evidence quarantines data rather than guessing.
- Signature/update verification failure preserves the previous accepted runtime.
- Confidential mail input stays in request bodies; sanitized mail HTML remains sandboxed and non-networked.
- Technical telemetry/support/evidence never carries raw PII, secrets, mailbox content or unbounded IDs.

## 6. External / Production Residual Risks

Repository tests can prove code paths, exact source/runtime composition and real Camoufox execution under
the CI environment, but they do **not** by themselves prove the target Production environment.

Still separately required before Production authorization where applicable:

- exact Cloudflare Access/mTLS application and certificate enrollment/revocation configuration;
- physical Windows device-key/certificate protection and machine lifecycle;
- trusted Windows signing, installer/distribution, updater and rollback behavior;
- exact Production D1/R2/Durable Object bindings, backup/recovery and key escrow;
- physical multi-device recovery and endpoint compromise behavior;
- provider/fingerprint behavior under the target network/environment;
- independent cryptographic/security review and privacy/retention approval;
- operational rollout, monitoring, incident and stop/rollback readiness.

Cloudflare account compromise remains high impact; D1 has no PostgreSQL-style RLS defense in depth; an
authorized compromised endpoint can observe plaintext while a Profile is in active use; provider and
fingerprint behavior changes independently. These risks are accepted only through their exact later-stage
evidence/authorization owners, never by relabeling repository CI as Production evidence.

## 7. Security Authority And Review Gates

This file is the canonical current threat model. Historical Phase/AR security documents preserve
provenance but do not override it.

Update this model whenever a trust boundary, cryptographic protocol, identity provider, operating-system
lane, mailbox provider, tenant model or future CRM adapter changes.

Production promotion requires one exact Release Candidate, its target-specific authorization envelope
and fresh evidence for all reachable mandatory security/recovery controls. Until that decision,
`source_present != production_enabled` remains binding.
