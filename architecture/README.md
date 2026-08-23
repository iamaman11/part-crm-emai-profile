# Architecture authority map

This directory is organized by **system subject**, not by remediation chronology. A developer should be able to locate the current architecture without reconstructing AR history.

The governing precedence is defined by `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md` and `docs/APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`:

```text
current prospective architecture
-> current product/security/runtime invariants
-> proved current external/durable contracts and real consumers
-> accepted historical outcomes/evidence
-> historical internal implementation shape
```

An accepted AR preserves required outcomes and provenance, not a permanent right for its old JSON/Python/Node implementation to remain executable architecture.

## Current subject-domain authorities

- `inventory.json` — tracked **generated architecture projection**. It is output only and must never be used as semantic input for the facts it projects.
- `credential-authority.json` — current bounded credential composition root. It references accepted AR-8B provenance and current lifecycle/profile-security contracts; it is not a global repository authority bag.
- `credential-lifecycle.json` — current bounded replacement / verification / activation / retirement / rollback / recovery contract for governed credentials and application OAuth credentials. Its long-term representation remains subject to the owning cutover; it is not permission to duplicate executable semantics.
- `profile-security.json` — current bounded Browser Profile / Camoufox security classification/ownership contract. It remains subject-domain data, not a global architecture registry.
- provider/resource topology — current executable ownership is provider-native Wrangler configuration + Product Rust, not an AR-qualified runtime-topology JSON.
- application/runtime/operator/release semantics — current executable semantics belong to their natural Rust/domain/runtime owners; generated JSON may project them but may not authorize them.

## Transitional current artifacts scheduled for normalization

The following artifacts may still be reachable on the current pre-PF-1 branch of history, but they are **TRANSITIONAL_SEMANTIC_SOURCE** or provenance, not permanent architecture shape:

- `python-estate-ar6.json` and AR-10/AR-11 Python-estate overlays — retired by N2. There will be no successor 1:1 Python file registry.
- `github-governance-ar7.json` and historical governance overlays — normalized by N3 so current desired governance + live observation + typed policy replace historical overlay reconstruction.
- `operator-contract.json` — N4 makes typed Rust `CommandRegistry`/effect ownership authoritative; retained JSON, if any, is projection only.
- `runtime-cutover-ar10.json` — N5 reassigns every still-current fact to Product Rust, `runtime-lock.json`, Camouhost/Bridge contracts, current governance or lifecycle/release owners, then retires the transitional document.

These artifacts must not be copied into successor JSON/TOML/YAML or a giant Rust registry merely to preserve their historical shape.

## Retired historical authorities

AR-2 runtime-topology authority is already retired from current architecture. Provider-native Wrangler configuration and Product Rust own executable topology/workloads; bounded fitness rules preserve anti-regression and AR-2 provenance remains in `docs/ARCHITECTURE_REBASELINE_V3_AR2.md` plus Git history.

As N2…N5 complete, the same rule applies: historical AR artifacts remain evidence of acceptance, while current semantics move to natural owners and obsolete executable intermediaries are deleted after zero-current-caller/zero-unique-invariant proof.

## AR-8 provenance and completion artifacts

AR numbers identify **how a contract was accepted**, not the current system domain model.

- `credential-authority-ar8b.json` — immutable accepted AR-8B credential registry provenance dataset. It is not the current composition root and must not be retroactively rewritten.
- `ar8-staging-provider-bootstrap-contract.json` — accepted AR-8C staging bootstrap/provider execution provenance and protected execution prerequisite. It is not a competing credential registry.
- `ar8-d-secret-transport-successor.json` — AR-8D transition provenance from Pre-2J D3 bundle transport to metadata-only steady-state Worker secret binding verification. Its durable steady-state rules are represented by current bounded credential lifecycle ownership.
- `ar8-completion-lifecycle.json` — completion-candidate provenance. Durable credential lifecycle/profile-security requirements are represented by their current subject owners.
- `ar8-operator-rehearsal.json` — AR-8F rehearsal/candidate provenance. Operator behavior is moving to typed Rust ownership under N4.

Full candidate snapshots are preserved under `docs/evidence/`:

- `docs/evidence/ar8-completion-lifecycle-candidate.json`
- `docs/evidence/ar8-operator-rehearsal-candidate.json`
- `docs/evidence/ar8-d-secret-transport-successor-candidate.json`

The AR-8 candidate files above must not become a second mutable source of truth. Compatibility inputs are retained only for a proved current consumer or explicit durable contract; historical acceptance alone is insufficient.

## Profile/Camoufox ownership

Profile/Camoufox ownership is prospective, not AR-shaped:

```text
browser/profile business semantics -> Product Rust
launch/lifecycle/writer ownership   -> Windows Profile Bridge / Product Rust
cross-language IPC                  -> versioned Bridge contract + validation
runtime dependency identity         -> runtime/camouhost/runtime-lock.json
real Camoufox adapter               -> runtime/camouhost/real.py
synthetic fixture                   -> runtime/camouhost/main.py test-only
security classifications            -> bounded profile-security owner
production admission                -> Release / Capability Profile
```

AR-13 owns profile-key rotation rehearsal; AR-14 remote recovery rehearsal; AR-15 Windows delivery/update chain. After PF-3 the architecture is frozen by design and those phases implement/rehearse the established architecture rather than creating new generic architecture mechanisms.
