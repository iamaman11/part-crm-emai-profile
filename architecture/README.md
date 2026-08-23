# Architecture authority map

This directory is organized by **system subject**, not by remediation chronology. A developer should be able to locate the current architecture without reconstructing AR history.

## Current subject-domain authorities

- `inventory.json` — canonical generated architecture projection.
- `credential-authority.json` — current composition root for credential authority. It references the accepted AR-8B registry dataset as immutable provenance and points to the current lifecycle, profile-security, and operator contracts.
- `credential-lifecycle.json` — current replacement / verification / activation / retirement / rollback / recovery rules for governed credentials and application OAuth credentials.
- `operator-contract.json` — permanent read-only, metadata-only operator surface and negative/recovery contract.
- `profile-security.json` — permanent Browser Profile / Camoufox security classifications and ownership boundaries.
- `python-estate-ar6.json`, `github-governance-ar7.json` — accepted architecture artifacts whose phase-qualified names remain tracked provenance/decision inputs.
- AR-2 runtime-topology authority is retired from the tracked current architecture: provider-native Wrangler configuration and Product Rust own executable topology/workloads, bounded current fitness rules prevent regression, and AR-2 provenance remains in `docs/ARCHITECTURE_REBASELINE_V3_AR2.md` plus Git history.

## AR-8 provenance and completion artifacts

AR numbers identify **how a contract was accepted**, not the current system domain model.

- `credential-authority-ar8b.json` — immutable accepted AR-8B credential registry provenance dataset. It is not the current composition root and must not be retroactively rewritten.
- `ar8-staging-provider-bootstrap-contract.json` — accepted AR-8C staging bootstrap/provider execution provenance and protected execution prerequisite. It is not a competing credential registry.
- `ar8-d-secret-transport-successor.json` — AR-8D transition provenance from Pre-2J D3 bundle transport to metadata-only steady-state Worker secret binding verification. Its durable steady-state rules are promoted into `credential-lifecycle.json`.
- `ar8-completion-lifecycle.json` — completion-candidate provenance. Durable credential lifecycle and profile-security rules are promoted into `credential-lifecycle.json` and `profile-security.json`.
- `ar8-operator-rehearsal.json` — AR-8F rehearsal/candidate provenance. Durable operator rules are promoted into `operator-contract.json`.

Full candidate snapshots are preserved under `docs/evidence/`:

- `docs/evidence/ar8-completion-lifecycle-candidate.json`
- `docs/evidence/ar8-operator-rehearsal-candidate.json`
- `docs/evidence/ar8-d-secret-transport-successor-candidate.json`

The AR-8 candidate files above must not become a second mutable source of truth. Permanent tooling reads the subject-domain authorities; AR-specific artifacts remain acceptance provenance/evidence or compatibility inputs where an accepted historical checker still needs them.

## Profile/Camoufox ownership after AR-8

AR-8 defines classifications and trust boundaries only. Real Camoufox runtime integration remains AR-10; profile-key rotation rehearsal AR-13; remote recovery AR-14; Windows delivery AR-15; Windows signing/update trust AR-15B through `windows.release-signing-trust`.

The six permanent profile security domains and the `CREDENTIAL_EQUIVALENT` browser-profile-generation payload are defined in `profile-security.json` and mechanically enforced by `.github/scripts/profile-security-authority-check.mjs`.
