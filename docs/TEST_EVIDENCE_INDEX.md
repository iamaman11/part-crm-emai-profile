# Test And Evidence Authority Guide

**Status:** CURRENT EVIDENCE NAVIGATION / NOT A STATUS REGISTRY
**Historical ledger:** [snapshot before D0](https://github.com/iamaman11/part-crm-emai-profile/blob/b8d5e31b263d5682aab99c6f17c9a000a2e5210e/docs/TEST_EVIDENCE_INDEX.md)

Evidence proves only its named invariant, candidate, environment and time window. This guide explains
where evidence is owned; it does not copy mutable workflow results or promote readiness.

## Evidence owners

| Claim | Current owner |
|---|---|
| Pure/domain/application invariant | owner-local Rust/unit/property tests |
| Public/generated contract | contract owner, compatibility tests and generated drift proof |
| Repository/source boundary | focused repository checker and its positive/negative fixtures |
| Exact PR acceptance | exact candidate head, applicable permanent workflows, protected contexts, reviews/threads and accepted-main reread |
| Release/artifact identity | immutable Release Set/artifact manifests and exact-byte verification |
| Hosted/provider/environment fact | fresh redacted observation from the protected natural owner |
| CAP-12 scenario | V2 B1–B10 package on one `ReleaseCandidateId` and non-Production target envelope |
| Production authorization | R1/R2 evidence + named R3 decision for exact Release Candidate and Production envelope |

Historical reports under [`evidence/`](evidence/) and completed CAP/AR/PF/PAS Issues remain provenance.
They may satisfy a current obligation only when the current owner proves identity, scope and freshness
are still applicable.

## Acceptance rules

1. Use one objective invariant and the cheapest sufficient proof tier.
2. A changed behavior has positive and negative/fail-closed proof on the same effect path.
3. Local/synthetic proof never claims a real provider, physical device, trusted signature or Production
   environment.
4. Mutable hosted evidence records observation time, recheck/expiry condition and redacts secrets/PII.
5. Exact-head CI from an older commit is not candidate evidence.
6. Removing or materially changing source, artifact, migration, effective set, target/config or an
   expired observation invalidates affected evidence.
7. A generated projection such as `docs/status.json` cannot create a claim or authorize Production.
8. A new permanent checker satisfies the future-check creation standard in
   [`ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`](ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md).

## Durable repository reports

Create a report under `docs/evidence/` only when a named current consumer needs a durable repository
artifact rather than a workflow/Issue locator. Use `YYYY-MM-DD-short-name.md` and record:

- exact source/release/artifact identity;
- environment/runtime versions and target identity;
- invariant, inputs, positive/negative results and failure behavior;
- evidence timestamp/freshness and durable external locators;
- limitations and explicitly unproven properties;
- whether user data, secrets or provider mutation were involved.

Never store secret values, raw cookies/tokens, mailbox/browser payload, uncontrolled PII or unsafe
screenshots in evidence. Evidence tooling remains calculation/verification machinery; the named R3
authority makes the Production decision.
