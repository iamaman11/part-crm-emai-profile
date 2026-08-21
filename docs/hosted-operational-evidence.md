# Hosted Operational Evidence

## Purpose

Hosted Operational Evidence is the single reusable mechanism for turning provider/GitHub observations into durable, typed, attestable evidence without giving `opsctl` network, credential, execution, scheduling, signing, or state-backend authority.

The permanent boundary is:

```text
GitHub Actions / official provider tools
    -> secret-free raw observation
    -> opsctl typed policy
    -> HostedEvidenceEnvelopeV1
    -> immutable GitHub Actions Artifact
    -> GitHub Artifact Attestation
```

The issue tracker may link to evidence. It is not evidence storage or an evidence authority.

## Authority split

- **GitHub Actions / Environments** own orchestration, approval, GitHub OIDC and the credential boundary.
- **Official provider tooling** (`wrangler`, provider APIs, `gh`) owns live observation or mutation when the calling operation permits it.
- **`opsctl evidence`** owns typed schemas, version dispatch, canonicalization, secret rejection, environment/effect policy and local verification.
- **GitHub Actions Artifacts** own run-scoped immutable transport between jobs.
- **GitHub Artifact Attestations** own signing/provenance. `opsctl` never signs and never implements a competing cryptographic system.

No hosted evidence command is allowed to invoke `curl`, `gh`, Wrangler, a provider API, a database, a child-process executor, or a secret store.

## Envelope V1

`HostedEvidenceEnvelopeV1` contains exactly:

- `schema_version`;
- `evidence_kind` and `payload_version`;
- `repository`, `source_sha`, `source_ref`;
- `workflow` (`name`, `workflow_ref`, `run_id`, `run_attempt`, `observation_job`);
- `environment` (`rehearsal`, `staging`, `production`);
- canonical UTC `observed_at`;
- `provider_mutation` and `production_mutation` flags;
- one typed payload.

Unknown envelope fields, evidence kinds, payload versions and payload fields fail closed. Inputs are bounded to 1 MiB. Secret-bearing field names and obvious bearer/private-key material are rejected recursively before evidence is accepted.

Initial payload variants are deliberately small and typed:

1. `credential_readiness` v1 — provider, credential identity, provider metadata identifier, `READY|NOT_READY`, and a canonical capability set.
2. `hosted_resource_state` v1 — provider, resource type/id, observed state, optional revision and enabled flag.
3. `release_set_transition` v1 — provider, capability profile, previous/target Release Set IDs, `APPLIED|NO_CHANGE|ROLLED_BACK|BLOCKED`, and compatibility decision.

Adding D1 migration evidence, rollback/recovery, runtime certification, updater activation/rollback, production cutover or a future module means adding a new typed Rust variant/version. It does **not** require a new transport, reporter framework, database, signing system or GitHub workflow family.

## `opsctl evidence`

The namespace is part of the existing `opsctl` parser/composition root and `architecture/operator-contract.json`; it is not a second CLI.

```text
opsctl evidence build --raw-observation raw.json --context-json context.json
opsctl evidence validate --evidence-json hosted-evidence.json
opsctl evidence inspect --evidence-json hosted-evidence.json
opsctl evidence verify --evidence-json hosted-evidence.json --context-json expected-context.json
```

`build` is the only transformation command. It reads saved JSON only and writes canonical evidence to stdout. The calling workflow chooses the destination file. `verify` requires exact canonical bytes and an independently reconstructed context, so a publisher job can bind the artifact back to its expected repository/source/workflow/run/environment/effect identity.

## Provider-facing observation job pattern

A caller remains provider-specific only where real observation is unavoidable. It should:

1. use the minimum staging/provider read credential needed by the official tool/API;
2. write a **secret-free typed raw observation** to a file;
3. construct `context.json` from trusted GitHub run metadata and explicit operation inputs;
4. run `opsctl evidence build` and `opsctl evidence verify` locally;
5. upload exactly `hosted-evidence.json` as an Actions Artifact using `actions/upload-artifact`;
6. call `.github/workflows/hosted-evidence-publish.yml` in a dependent job without passing or inheriting provider secrets.

The observation job must mask provider credentials and must never place a credential value, Authorization header, token, private key, secret handle or plaintext secret in raw observations, contexts, logs, artifacts or issue comments.

## Reusable publication workflow

`.github/workflows/hosted-evidence-publish.yml` is the one permanent reusable publication path. It accepts only non-secret evidence identity inputs and the artifact name. Its publisher job:

1. checks out the exact policy source for the run;
2. downloads the immutable Actions Artifact;
3. requires exactly one `hosted-evidence.json` subject;
4. reconstructs expected context independently from workflow/run inputs;
5. runs `opsctl evidence verify` again;
6. derives a small custom predicate from verified `opsctl` output;
7. invokes the official digest-pinned `actions/attest` action with GitHub OIDC/attestations permission.

The publisher job has no Cloudflare/provider secret declaration and no provider network step. The signing authority and provider credential therefore do not coexist in one job.

A caller invokes it at job level, for example:

```yaml
  publish-evidence:
    needs: observe
    permissions:
      contents: read
      id-token: write
      attestations: write
    uses: ./.github/workflows/hosted-evidence-publish.yml
    with:
      artifact_name: hosted-evidence-${{ github.run_id }}-${{ github.run_attempt }}
      evidence_kind: credential_readiness
      payload_version: 1
      environment: staging
      source_sha: ${{ github.sha }}
      source_ref: ${{ github.ref }}
      observation_workflow_name: ${{ github.workflow }}
      observation_workflow_ref: ${{ github.workflow_ref }}
      observation_job: observe
      observed_at: ${{ needs.observe.outputs.observed_at }}
      provider_mutation: false
      production_mutation: false
```

Do not add `secrets: inherit` to this call.

## Verification model

There are two intentionally separate checks:

- `opsctl evidence verify` verifies the project policy: schema/version, secret-free typed payload, effect flags, deterministic canonical bytes and expected run/source binding. It is offline.
- `gh attestation verify` verifies the GitHub/Sigstore attestation and signer identity. It is an official GitHub verification path and is not reimplemented inside `opsctl`.

For this repository the custom predicate type is:

```text
https://github.com/iamaman11/part-crm-emai-profile/attestations/hosted-operational-evidence/v1
```

## Non-goals

This primitive is not an observability platform. It does not add an evidence database, queue, scheduler, daemon, custom PKI, custom signer, provider SDK inside `opsctl`, or a family of per-feature reporter workflows. Production authorization remains owned by the existing release/governance architecture; merely having an evidence kind in source does not enable production.
