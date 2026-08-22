# Browser Profile Platform — Development Plan

**Document status:** GENERATED_PROJECTION  
**Current architecture/program authority:** `ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Mandatory requirements:** `APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md`  
**Quality contract:** `ARCHITECTURE_EVOLUTION_QUALITY_CONTRACT.md`  
**Tracking:** #266  
**Accepted product phase:** Phase 2I  
**Accepted architecture checkpoint:** AR-11  
**Derived current architecture slice:** AR-12 — NOT STARTED  
**Production Core gate:** `BLOCKED`  
**Production readiness:** `false`

This file is a projection of the canonical program. It is not a second roadmap or lifecycle authority.

## 1. Current state

- AR-0…AR-11: accepted.
- AR-12: derived current in the static sequence, implementation NOT STARTED.
- Post-AR-11 Functional Closure #399 blocks AR-12 entry.
- Production remains disabled/fail-closed.
- `source_present != production_enabled` is binding.

## 2. Current implementation order

```text
F1  Release Set breaking-contract version discipline
+
F2  permanent architecture foundations
    - application mandatory requirements
    - opsctl pure-core / adapter boundary
    - opsctl doctor diagnostic boundary
    - canonical JSON/digest discipline
    - Python role/effect boundary
 ->
N1  AR-2 runtime/resource topology current-authority retirement
 ->
N2  AR-6 Python-estate authority retirement + role/effect normalization
 ->
N3  AR-7 current GitHub-governance normalization
 ->
N4  bounded AR-8 operator/provenance cleanup
 ->
N5  AR-10 runtime semantic-authority retirement
 ->
PF-1 #430 typed lifecycle + deterministic bounded-projection inventory cutover
 ->
PF-2 Hosted Operational Evidence / Draft #428
 ->
PF-3 #431 typed Architecture Fitness Baseline
 ->
fresh #399/#421 re-baseline
 ->
FC-6
 ->
FC-7
 ->
AR-12 implementation entry
```

F1/F2/N1…N5 are foundation/normalization transactions, not new AR/PF lifecycle slices. `architecture/architecture-program-sequence.json` remains unchanged.

## 3. Why this normalization exists

The accepted historical AR work is not being reopened. The cleanup removes transitional current semantic intermediaries so PF-1 does not become a mechanical Rust port of old JSON/Python/Node authority machinery.

Target examples:

```text
AR-2 topology -> Wrangler/provider config + Product ownership
AR-6 Python estate -> repository/source role/effect policy
AR-7 governance -> current desired governance data + live observation
AR-8 operator contract -> Rust CommandRegistry/effect registry
AR-9 D1 -> SQL + typed Rust policy                 [already cut over]
AR-10 runtime -> Product Rust + runtime-lock + Camouhost adapter + tests
```

For every cutover:

```text
natural owner proved
-> callers switched
-> old callers = 0
-> old unique current invariants = 0
-> DEAD predecessor deleted/demoted
-> history preserved in Git/evidence
```

## 4. Permanent application architecture rules

```text
one semantic fact -> one natural owner
bounded contexts own business semantics
inward dependency direction
Pure Core / Effect Shell
observation != policy decision
explicit effect capabilities
typed critical IDs/states/contracts
context-owned persistence
Release Profile = sole production-enable authority
frontend = projection, not security boundary
generated projection != semantic source
```

Do not create global authority bags, universal business repositories, generic plugin/DI/service-locator frameworks or second production feature-flag systems.

## 5. `opsctl` development boundary

`tools/opsctl` is standalone offline operator/policy/planning/verification/projection tooling.

```text
JSON/filesystem/local artifacts
        ↓
adapters + versioned DTOs
        ↓
typed semantic inputs
        ↓
PURE CORE
        ↓
typed results
        ↓
output adapter
```

Hard zero requirements:

```text
serde_json::Value crossing adapter -> pure core = 0
filesystem/process/network/provider effects in pure core = 0
Product Runtime -> opsctl/opsctl-core = 0
opsctl provider/network/process authority = 0
opsctl -> Python semantic child process = 0
global authority bag = 0
```

A small internal `opsctl-core` crate is preferred when it materially enforces this boundary at compile time.

## 6. `opsctl doctor`

`doctor` remains supported, but its permanent role is narrowed to **read-only local diagnostic composition**.

It may:

```text
resolve repository root
read local files/metadata
strictly decode local contracts through owned adapters
aggregate typed diagnostic results
render machine/human report
```

It must not:

```text
call Python/Node/Git/GitHub/provider subprocess/API
access network/providers/secrets
launch runtime/browser
perform mutation
duplicate domain/release/lifecycle/evidence policy
maintain a global authority list as semantic truth
```

Current dependencies on AR-6 Python estate, `operator-contract.json`, Python inventory generator and generated/retired root sentinels are transitional. N2/N4/PF-1 remove them. PF-3 prevents regression.

## 7. Python boundary

Python is allowed by role/effects, not by a permanent file whitelist.

Allowed examples:

```text
runtime/camouhost/real.py -> genuine Camoufox outer runtime adapter
runtime/camouhost/main.py -> synthetic/test fixture
repository/source validators/observers
deterministic generators/renderers
tests
developer-local orchestration
outer observation adapters where justified
```

Forbidden permanent roles include duplicate Product/release/D1/lifecycle/evidence/fitness semantic authority, Profile Bridge runtime bypass, hidden provider mutation and secret readback.

The AR-6/AR-10/AR-11 Python estate overlay chain is retired by N2. No successor JSON/TOML/YAML/Rust list of every Python file is allowed.

## 8. PF-1 target

```text
outer Git/GitHub raw observations
-> typed LifecycleEvaluator
-> DerivedLifecycleStateV1

D1InventoryProjection
RuntimeTopologyProjection
ApplicationInventoryProjection
OperatorInventoryProjection
GovernanceInventoryProjection
RuntimeInventoryProjection
CredentialInventoryProjection
ReleaseInventoryProjection
-> pure ArchitectureInventoryCompiler
-> architecture/inventory.json
```

PF-1 must not build `GlobalRepositoryAuthorityLoader -> GlobalAuthoritySet`.

Legacy Node lifecycle and Python architecture inventory/projection current owners are deleted after parity + zero-caller/zero-unique-invariant proof.

## 9. PF-2 target

```text
outer GitHub/provider observation
-> strict versioned DTO
-> typed Rust EvidencePolicy
-> HostedEvidenceEnvelopeV1
-> canonical durable JSON
-> immutable artifact/attestation
```

Network/provider reads and clocks remain outside `opsctl` pure policy.

## 10. PF-3 target

Fitness semantics are typed Rust:

```text
FitnessRuleRegistry
-> evaluator/enforcement mapping
-> positive/negative fixtures
-> Architecture Fitness Gate
-> optional generated report/index
```

A manually maintained semantic `architecture/architecture-fitness-policy.json` is not the target.

PF-3 permanently enforces authority uniqueness, dependency/effect boundaries, `opsctl`/doctor restrictions, Python role/effect rules, versioned external contracts, cutover-to-deletion and Release Profile admission.

## 11. AR sequence after Functional Closure

```text
AR-12 Fresh Rehearsal Environment
AR-13 Rotation Rehearsal
AR-14 Remote Recovery Rehearsal
AR-15 Windows Delivery Program / inherited Batch E
AR-16 Final Whole-project 10/10 Audit
AR-17 Architecture Closeout + Production Core Gate
```

Only after AR-17:

```text
PC-1 Production Core v1
PC-2 Mailbox Administration
PC-3 Mailbox Jobs / Automation
PC-4 Outbound / later capabilities
```

Only PC-1 may make `production_ready=true` for the accepted core Release Profile.

## 12. Windows delivery requirement

The current Production Core scope includes Camoufox/profile runtime through Windows Profile Bridge. Therefore AR-15 remains mandatory before AR-16/AR-17 closeout and must cover the inherited Batch E concerns: signed/versioned update manifest, trust/key rotation, side-by-side staging, safe activation, health/Last Known Good rollback, immutable publisher integration, permanent failure tests and production-equivalent Windows rehearsal.

## 13. Acceptance discipline

Every bounded F/N/PF/FC/AR/PC candidate requires:

```text
fresh protected-main baseline
-> natural owner/effect/contract analysis
-> positive + negative tests
-> exact-head permanent CI green
-> protected required contexts green
-> behind_by=0
-> blocking reviews=0
-> unresolved threads=0
-> guarded merge bound to exact head
-> post-merge accepted-main reread
```

Historical required-context names/counts are observations and must be re-read live at merge time.

## 14. Immutable Accepted Phase Provenance

These records are a compact immutable projection of `architecture/accepted-phases.json`. They are historical provenance, not a competing forward roadmap. The permanent architecture gate verifies them so accepted product history cannot be silently rewritten while the current execution plan evolves.

Phase 1A was accepted through issue #114 / implementation PR #115; exact proven source head
`21b4bc65cd1bb117504c0a0cfe18c8c11e411f25`; guarded squash merge
`0186b780f7fed4b7c5e7f212c2fe437cbc46a5e5`.

Phase 1B was accepted through issue #120 / implementation PR #135; exact proven source head
`22b2ef36a943d07d22755bf467ec6e7c27ef081d`; guarded squash merge
`f081e0709481d6bbaa150f5518ec8552124c78de`.

Phase 2A was accepted through issue #118 / implementation PR #137; exact proven source head
`2d80ee74bc8d05657414ea4e75dcf6f41c723926`; guarded squash merge
`a1eb2833a74d9156bce8f4b1c6e92815cc0d55bc`.

Phase 2B was accepted through issue #138 / implementation PR #140; exact proven source head
`895594e35b77ddd86395300b1644e9df6a712123`; guarded squash merge
`298062ea443c31c69212cb03b3988265b6bbcd48`.

Phase 2C was accepted through issue #142 / implementation PR #143; exact proven source head
`d3ad2e774a98ad5fed2565ba410ba9923062d170`; guarded squash merge
`042d0dc72fa37e99f971d61d21544609a69c6e31`.

Phase 2D was accepted through issue #144 / implementation PR #147; exact proven source head
`ad491e2f0c9ba9f79130923fdde6fe1407af4dc5`; guarded squash merge
`26f8fa82bdad02a5a0867b0d36748b915579ef1c`.

Phase 2E was accepted through issue #148 / implementation PR #152; exact proven source head
`0cefa67abe810db079102462f33ec28fcfc73f69`; guarded squash merge
`6c6ba4564de88b40d282081e701a2d24f1611cc2`.

Phase 2F was accepted through issue #154 / implementation PR #155; exact proven source head
`c36df418f9fa877c5143327e97b60087c33ffd02`; guarded squash merge
`42b26dc0c78c0c65dcea2bc90bb5ce6a3bd4b02b`.

Phase 2G was accepted through issue #159 / implementation PR #160; exact proven source head
`85ca77b430e7d184204082aea7d51a08fdd72cf9`; guarded squash merge
`48e24f1f365d87a07bf97322c81099dd6a89f046`.

Phase 2H was accepted through issue #163 / implementation PR #164; exact proven source head
`9add9b94d0de255b93e5a7c24584fcf6756462a7`; guarded squash merge
`a32768feddb3da69b872e701bc529aad3521e1b0`.

Phase 2I was accepted through issue #167 / implementation PR #168; exact proven source head
`c1075337cfc582d0f4c00ec34b1aa7cda9ac1101`; guarded squash merge
`800c634147d6300ea3989ff0cf87ade6e2387ee9`.
