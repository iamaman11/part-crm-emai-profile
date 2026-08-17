# AR-8C staging provider bootstrap authority

This document records the protected staging-only execution authority that may materialize the hosted prerequisite for AR-8C. The machine-readable authority is `architecture/ar8-staging-provider-bootstrap-contract.json`; it is **not** a second credential registry. Credential ownership remains canonical in `architecture/credential-authority-ar8b.json`.

## Accepted evidence before mutation

Protected accepted-main runs proved the three temporary bootstrap roots are present and active, discovered the real Cloudflare staging topology without secret readback, and classified staging D1 candidates with read-only SQL. Provider identifiers remain runtime evidence and are intentionally not committed to Git.

The current catalog runtime target is `part-crm-catalog-staging-d3-20260815`, which matches all current catalog migrations. `part-crm-catalog-staging` is stale and must not be selected. `part-crm-d3a-bootstrap-proof` is proof-only. Production resources are outside this authority.

The historical dedicated resolver D1 already exists. It is not `MISSING`; creating another resolver database is forbidden. Its base schema is valid but it lacks the accepted AR-8A refresh-fencing migration. Convergence is allowed only when the remote migration ledger is an exact ordered prefix of `migrations/resolver-d1` and only the exact missing suffix from the accepted-main checkout is applied. Any non-prefix state fails closed and requires a new remediation authority.

## Execution boundary

Provider mutation may run only from an accepted-main `workflow_dispatch` job protected by the GitHub `staging` Environment. Discovery is always first. A resource may be created only when its accepted role is classified `MISSING`; matching resources are reused; conflicts and authorization uncertainty stop execution. Delete, rename, replacement-by-duplicate, production access/mutation, Terraform, scheduled provisioning and secret-value readback are forbidden.

Runtime secret values are written directly to Cloudflare Worker secret stores. The bootstrap process may bind the four AR-8C steady-state outputs to the protected staging GitHub Environment, but it may not expose their plaintext in logs, artifacts, Git, issues or chat. The temporary bootstrap credentials must not become the steady-state deploy credentials and are revoked/removed after verified handoff.

## Accepted staging resource names

The current catalog is `part-crm-catalog-staging-d3-20260815`; the dedicated resolver D1 is `mailbox-secret-resolver-gate-b-bootstrap-20260815-033400`; the profile object bucket is `part-crm-profile-objects-staging-d3`. The accepted queues are the `part-crm-*-staging-d3` generation-verification, integration-events, mailbox-jobs and mailbox-jobs-dlq queues recorded in the machine contract.

The resolver Worker name is fixed by the existing resolver contract as `mailbox-secret-resolver-staging`. The control-plane Worker is fixed by this authority as `browser-profile-control-plane-staging`. The custom domain is `staging.alegria.by`. Access names are fixed in the machine contract so execution cannot invent resource names during a privileged run.

## External facts

Cloudflare account/zone identity is discovered live. Access issuer/team-domain metadata must be discovered live. Google and Microsoft staging OAuth application identities/secrets remain externally owned facts: they must be supplied by their legitimate provider authority when required and must never be guessed or replaced with placeholders.

## AR-8C handoff

AR-8C is not accepted until `CLOUDFLARE_API_TOKEN`, the Access service-auth pair and `CLOUDFLARE_DEPLOY_MANIFEST_JSON` are write-only bound to the protected staging Environment, required Worker secret names are present, staging smoke/attestation succeeds, bootstrap authority is retired, and the existing AR-8C hosted reconciliation returns green in read-only mode. AR-8D and AR-9 remain blocked until their prerequisite acceptance points are satisfied.
