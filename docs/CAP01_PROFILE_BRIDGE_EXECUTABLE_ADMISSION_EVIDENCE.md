# CAP-01 Profile Bridge executable-admission evidence

Status: **CAP-01 EVIDENCE SNAPSHOT / VERIFY AGAINST FRESH MAIN**
Audit date: **2026-08-26**  
Repository: `iamaman11/part-crm-emai-profile`

This dated record closed the Profile Bridge / Camoufox research obligation in
`CAP01_CAPABILITY_POLICY_IMPLEMENTATION_CONTRACT.md` §10.4 without inventing a runtime
surface that had no production consumer at the audit baseline. It is provenance, not live execution or
Production authority; P2/A0/V2 must prove the shipping path on their exact candidate.

## Question

Can an existing or replayed command bypass the canonical Capability Policy by reaching an
independent Profile Bridge / Camoufox production executor after the active profile changes?

CAP-01 permits only two valid answers: either server-side claim/lease reauthorization is proven to
dominate execution, or an independent Bridge executable ingress must enforce a real capability
grant before its first effect. Merely declaring `BridgeCamoufoxLaunch` is explicitly insufficient.

## Repository evidence

At the audited CAP-01 branch state:

- `apps/profile-bridge/src/main.rs` is the default `profile-bridge` executable and accepts exactly
  one `profilebridge://claim/...` URI. Its success path ends after `ClaimUri::parse` with the generic
  result `claim-uri-accepted`.
- The default executable does **not** compose `ProfileBridgeOperator`,
  `RuntimeSessionOrchestrator`, `ManagedCamouhostProcess`, Camouhost, a browser process, a network
  client, or another external-effect adapter.
- `apps/profile-bridge/src/operator_flow.rs` contains the richer operator orchestration as library
  code. It can eventually call the runtime launcher, but it is not composed by the default
  production executable.
- `apps/profile-bridge/src/camouhost_process.rs` contains a real managed process adapter whose
  `spawn` path is an external effect. It is exercised by AR-10 runtime tests, not published through
  the default production entrypoint.
- `apps/profile-bridge/src/bin/profile-bridge-synthetic.rs` is the only auxiliary binary and uses
  synthetic/fake runtime adapters for deterministic proofs.
- `ProfileCoordinatorPort` is an application port; the audited Bridge composition does not prove a
  capability-aware server implementation that reauthorizes a command at claim/lease time.

Therefore neither of the following statements would be truthful today:

1. “the production Bridge independently launches Camoufox and enforces Capability Policy”; or
2. “server-side claim/lease reauthorization has already been proven to dominate every Bridge
   execution effect.”

The truthful current state is narrower: **there is no independent production Bridge/Camoufox
executor published by the repository.**

## CAP-01 decision

`RuntimeSurface` describes executable surfaces that have a real consumer/enforcement point.
Accordingly CAP-01 removes the decorative `bridge.profile_runtime.commands` and
`bridge.camoufox.launch` surface declarations. The `profile_runtime` and `camoufox`
`ActivationUnit`s remain canonical capabilities because they are real domain/release capabilities;
only the unproven Bridge execution surfaces are removed.

This is not a fallback and does not preserve a parallel authorization path. It avoids introducing a
fake `profile-bridge -> capability-policy` dependency solely to make the catalog appear complete.

## Permanent invariant

`scripts/check-cap01-profile-bridge-boundary.py`, executed by the existing protected **Camoufox
Runtime Gate**, fails closed if the current production boundary changes without an explicit new
architecture transaction. It requires:

- the default production binary to remain the claim-only `apps/profile-bridge/src/main.rs`;
- the current auxiliary binary inventory to remain only `profile-bridge-synthetic.rs`;
- no production entrypoint composition of the operator/runtime/Camouhost/process/network effect
  path;
- no decorative `capability-policy` dependency while there is no production executor.

The checker includes negative fixtures for operator composition, direct process execution, and an
additional executable.

## Required future transaction

If a real production Bridge/Camoufox executor is introduced later, that same change must update this
boundary deliberately. Before the first process/network/provider effect it must have exactly one
canonical execution-admission mechanism:

- direct admission through the canonical `capability-policy` owner; **or**
- verification of a signed/versioned execution grant whose server-side issuance is itself
  capability-authorized at claim/lease time.

That transaction must simultaneously add the real runtime surface and consumer, remove this
claim-only guard where appropriate, and prove negative acceptance: **DENIED implies zero process
spawn, zero provider/network mutation, and zero executable side effects.** No compatibility bypass,
parallel executor, or “temporary” fallback is allowed.

## Result

CAP-01 has a truthful executable-boundary result for the current repository: the governed Worker
surfaces enforce capability policy, while Profile Bridge has no separate production Camoufox effect
surface to authorize. Any future transition from claim-only Bridge to production executor is now a
CI-visible architectural change rather than a silent bypass.
