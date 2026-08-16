# AR-8 staging provider bootstrap prerequisite

**Status:** candidate clarification for AR-8C / #314 under AR-8 umbrella #308.  
**Scope:** staging only. Production mutation remains forbidden through AR-17.  
**Machine contract:** `architecture/ar8-staging-provider-bootstrap-contract.json`.  
**Canonical credential authority:** `architecture/credential-authority-ar8b.json`.  
**Accepted historical bootstrap design:** `architecture/pre2j-d3-resolver-bootstrap-authority.json` / #256 / PR #257.

## 1. Decision

AR-8C hosted reconciliation is an **audit of already materialized provider state**. It is not the mechanism that initially creates Cloudflare staging resources, mints provider credentials or invents deployment identity.

The permanent lifecycle is:

```text
accepted Git/provider authority
        ->
read-only staging discovery
        ->
explicit protected staging bootstrap ceremony
(create only missing resources allowed by accepted authority)
        ->
bind runtime secrets directly to Cloudflare protected Worker secret stores
        ->
issue steady-state deploy + Access credentials
        ->
build the real staging deploy manifest from provider-issued/discovered identities
        ->
bind protected GitHub staging inputs
        ->
verify outputs and revoke bootstrap authority
        ->
AR-8C read-only hosted reconciliation
        ->
AR-8D -> AR-8E -> AR-8F -> AR-8 final closeout
```

This is not a new AR stage and is not a second infrastructure/credential registry. It is the explicit hosted prerequisite that bridges already accepted design authority to the existing AR-8C read-only audit.

## 2. Why the previous flow looked circular

The current AR-8C audit requires real staging bindings such as `CLOUDFLARE_API_TOKEN`, the Access service-token pair and `CLOUDFLARE_DEPLOY_MANIFEST_JSON`. Those values depend on real provider state: a token must first be issued, an Access service token must first exist, and manifest resource identifiers cannot be known honestly before the corresponding resources are discovered or created.

Therefore those bindings are **outputs of staging bootstrap**, not arbitrary values that an operator must guess before bootstrap is allowed to run.

The accepted #256 authority already separated repository implementation from later external provisioning. Its `automatic_resource_provisioning_forbidden` rule means that ordinary release/deployment code must not silently create or rename infrastructure. It does **not** prohibit an explicit, reviewed, staging-only external provider ceremony from creating the resources that the accepted authority explicitly ordered.

## 3. Hard boundaries

The bootstrap ceremony must remain all of the following:

- staging-only;
- read-only discovery first;
- explicit and protected, never automatic IaC or routine deploy behavior;
- no Terraform;
- no production provisioning/promotion/credential mutation;
- no secret plaintext in Git, issues, pull requests, logs, artifacts or manifests;
- no placeholder/dummy/guessed provider IDs, domains or credentials;
- no duplicate resource creation when a matching accepted resource already exists;
- no secret-value readback requirement;
- no mutable `opsctl` authority;
- bootstrap credential distinct from steady-state `CLOUDFLARE_API_TOKEN`;
- bootstrap credential revoked after verified handoff.

A missing external fact or missing permission is a **hard stop with an exact diagnostic**, not permission to invent a value or widen the architecture.

## 4. Discovery before mutation

Before creating anything, inventory the selected staging Cloudflare account/zone and classify each accepted resource as:

```text
PRESENT_AND_MATCHING
MISSING
PRESENT_BUT_CONFLICTING
UNKNOWN_DUE_TO_AUTHORIZATION
```

`MISSING` may be created only through the accepted staging bootstrap ceremony. `PRESENT_AND_MATCHING` must be reused. `PRESENT_BUT_CONFLICTING` and `UNKNOWN_DUE_TO_AUTHORIZATION` fail closed for operator resolution.

A missing GitHub Environment secret alone is never proof that the corresponding Cloudflare object is absent.

## 5. Bootstrap authentication

The initial provider mutation needs an external bootstrap root because a steady-state deployment token cannot issue itself.

Allowed bootstrap roots are:

1. an authenticated connected Cloudflare provider session whose permissions match the accepted bootstrap profile; or
2. a temporary, environment-scoped Cloudflare bootstrap API token transferred outside Git/issues/chat.

The accepted #256 authority defines the bootstrap capability profile and separately defines the narrower steady-state deployment profile. The bootstrap credential must never be stored as `CLOUDFLARE_API_TOKEN`.

After the steady-state credentials/bindings have been verified, revoke the bootstrap token/session authority that is no longer required.

## 6. Provider/resource order

The staging ceremony preserves the accepted order exactly:

1. select the accepted immutable resolver release artifact;
2. discover/create the dedicated staging resolver D1;
3. classify/create the staging catalog D1 and apply only the accepted D3A bootstrap when required;
4. discover/create isolated staging R2 and queues;
5. issue isolated R2 S3 credentials;
6. deploy the accepted resolver Worker with the complete required staging secrets and verify secret names only;
7. reserve/verify the staging hostname, Access application, Service Auth policy and service token;
8. bind the protected GitHub staging Environment inputs;
9. perform the first control-plane deployment from the accepted immutable artifact with the real resolver binding/custom domain and run staging smoke/attestation.

Routine deployments after bootstrap must not repeat this resource-creation lifecycle.

## 7. Secret ownership after bootstrap

### Git

Git stores names, schemas, policy, lifecycle metadata and required-secret declarations only. It is never the secret-value authority.

### GitHub repository secrets

Repository-level governance credentials remain in GitHub repository secret storage, including the existing `GOVERNANCE_AUDIT_TOKEN` and `GH_ADMIN_OPERATOR_TOKEN` authority.

### GitHub `staging` Environment

Deployment/smoke-facing protected inputs are:

- `CLOUDFLARE_API_TOKEN`;
- `CLOUDFLARE_ACCESS_CLIENT_ID`;
- `CLOUDFLARE_ACCESS_CLIENT_SECRET`;
- `CLOUDFLARE_DEPLOY_MANIFEST_JSON` (protected configuration, not a credential).

The current `CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON` and `CLOUDFLARE_RESOLVER_SECRETS_JSON` bindings are transitional bootstrap/rotation transport. Final AR-8 normalization must remove runtime-secret value transport from **routine application deployment** while retaining a distinct protected bootstrap/rotation mutation path.

### Cloudflare Worker secret stores

Steady-state runtime secret authority is the target Worker secret store. Relevant secret names include:

Control plane:

- `CLIENT_CONTACT_PROTECTION_KEYRING`;
- `MAILBOX_RESOLVER_CALLER_AUTH_KEY`;
- `R2_GENERATION_ACCESS_KEY_ID`;
- `R2_GENERATION_SECRET_ACCESS_KEY`.

Resolver:

- `GOOGLE_OAUTH_CLIENT_SECRET`;
- `MAILBOX_RESOLVER_CALLER_AUTH_KEY`;
- `MAILBOX_RESOLVER_ENCRYPTION_KEYRING`;
- `MAILBOX_RESOLVER_HANDLE_HMAC_KEY`;
- `MICROSOFT_OAUTH_CLIENT_SECRET`.

`wrangler.secrets.required` remains the repository-safe name contract. Hosted audit verifies presence/metadata only; it does not retrieve secret values.

### Resolver D1

Dynamic tenant/mailbox OAuth/token credentials remain under the accepted AR-8A encrypted resolver authority. They are not moved to GitHub or a second key/value registry.

## 8. What can be generated and what cannot

### Project-generated cryptographic material

When absent, a protected bootstrap/rotation ceremony may cryptographically generate and directly bind project-owned secret material such as:

- `CLIENT_CONTACT_PROTECTION_KEYRING`;
- `MAILBOX_RESOLVER_CALLER_AUTH_KEY`;
- `MAILBOX_RESOLVER_ENCRYPTION_KEYRING`;
- `MAILBOX_RESOLVER_HANDLE_HMAC_KEY`.

The generated plaintext must never pass through Git, issues, PRs, logs or ordinary artifacts.

### Provider-issued material

Provider-issued values must come from the provider ceremony, including R2 access credentials, Cloudflare operational/API credentials, Access service-token credentials and Google/Microsoft OAuth client secrets.

### External facts that may require the owner/provider

Never guess:

- the selected Cloudflare account and owned DNS zone;
- a staging hostname/custom domain if accepted provider state does not already fix it;
- the Google staging OAuth application identity/secret when Google OAuth is enabled;
- the Microsoft staging OAuth application identity/secret when Microsoft OAuth is enabled.

If any of these do not exist, the correct result is an explicit external setup requirement, not a placeholder.

## 9. AR-8C handoff acceptance

Before AR-8C can be accepted, staging must have:

- the real accepted provider substrate;
- all required Worker secrets bound to the correct Cloudflare Workers;
- an active bounded steady-state `CLOUDFLARE_API_TOKEN` bound to GitHub `staging`;
- the real staging Access service-token pair bound to GitHub `staging`;
- a strict schema-v1 `CLOUDFLARE_DEPLOY_MANIFEST_JSON` built from real provider identities;
- the bootstrap authority revoked/removed from normal project deployment authority;
- an accepted-main `GitHub Governance Gate` where both GitHub hosted-state and Cloudflare operational-credential hosted-state reconciliation are green.

AR-8C remains read-only during this final reconciliation.

## 10. Remaining work to finish AR-8 completely

After AR-8C is exact-green and accepted, the remaining mandatory work is:

### AR-8D — encryption/service-auth lifecycle

Implement and prove monotonic key versions, active/previous transition, read-old/write-new, downgrade rejection, interrupted-rotation recovery, explicit retirement, coordinated/dual-valid caller-auth transition where required, handle-HMAC migration/dual lookup and permanent negative tests.

### AR-8E — OAuth application credential lifecycle

Harden Google/Microsoft **application** credentials separately from user/mailbox tokens: provider metadata validation, replacement issuance, overlap/cutover where provider semantics support it, redirect/client-ID integrity, verify-before-revoke and fail-closed recovery. AR-8A remains the mailbox OAuth state-machine authority.

### AR-8F — operator UX and rehearsal

Provide metadata-only status/drift/rotation-readiness UX and runbooks. Rehearse stale/revoked/unknown credentials, wrong environment/consumer, interrupted/partial rotation, provider outage, rollback, dual authority, leakage prevention and recovery.

### Steady-state deployment normalization

Normal immutable application deployment must deploy code/config without receiving or resending the steady-state runtime secret values. Secret mutation belongs to the separate protected bootstrap/rotation ceremony; routine deployment verifies required names/metadata only.

### Final AR-8 closeout

Use one unchanged final candidate SHA, require all applicable permanent workflows/rehearsals green, `behind_by=0`, zero blocking reviews and unresolved review threads, then perform one guarded merge. Only after accepted-main reread and #308 closeout may AR-9 begin.

## 11. Current external prerequisites versus repository work

Repository-side documentation/contract enforcement can be prepared without any secret values.

Actual AR-8C hosted completion requires provider access and real staging facts. If the connected execution environment has no authenticated Cloudflare account action, the remaining bootstrap cannot be honestly executed from repository code alone. The operator must provide/authorize the external Cloudflare bootstrap root through a protected provider connection or secret-entry surface; secret values must not be pasted into chat.

The same rule applies to any missing Google/Microsoft OAuth application secret: create/import it through the provider/protected secret surface, then let the project verify only metadata and behavior.
