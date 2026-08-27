# Pre-2J D3 resolver and first-deploy bootstrap authority

**Status:** proposed authority for issue #256. Implementation and Cloudflare provisioning remain
separate later units.
**Decision base:** accepted post-D3A main
`65550585baa471c8fb3c452c85ee5db7e79d9b5b`.
**Parent:** D3 #251 / Draft PR #255. **Umbrella blocker:** #203.

## Why D3 must stop

The accepted control-plane Worker calls a fixed `MAILBOX_SECRET_RESOLVER` service binding, but the
repository has no deployable resolver Worker. The inspected Cloudflare account also has no Worker
that could satisfy that binding. A Cloudflare service binding names another Worker in the same
account; a placeholder Worker, self-binding or undocumented external implementation would not
satisfy the accepted credential boundary.

The accepted control-plane config also marks three Worker secret names as required. Required-secret
validation fails when those secrets do not already exist, while the current D3 workflow does not
materialize runtime secrets during the first upload. Running `wrangler secret put` before D3 is not
a read-only setup action: it creates a Worker version and deploys it. The first deployment therefore
needs an explicit, reviewed secret-upload path rather than an undocumented manual ceremony.

This authority closes the design ambiguity only. It creates no provider resources and accepts no
deployment evidence.

## Resolver ownership and ingress

The implementation unit must add one repository-owned Rust Cloudflare Worker at
`apps/mailbox-secret-resolver-worker` and one canonical environment template at
`deploy/cloudflare/mailbox-secret-resolver.wrangler.jsonc`. Staging and production use the exact
names recorded in the machine authority and separate resources/secrets.

The resolver has no public route and `workers_dev=false`. OAuth callbacks terminate at the existing
control-plane public surface; the control plane forwards the bounded completion request through the
service binding. Every resolver request must additionally carry a versioned HMAC-SHA-256 signature
over method, path, body digest, tenant, timestamp and single-use nonce. Service-binding reachability
alone is not caller authentication. Resolver-side timestamp/nonce replay protection is mandatory;
unsigned, stale, replayed, cross-tenant and wrong-purpose requests fail before secret lookup or
provider I/O.

Sensitive authorization codes, passwords and bearer material move in bounded request bodies, never
in query strings or ordinary log fields. Existing caller adapters that currently place transient
material in headers must be migrated in the implementation unit and covered by negative scans.

The exact internal endpoint inventory is machine-owned by
`architecture/pre2j-d3-resolver-bootstrap-authority.json` and must match all accepted resolver URLs
used by `crates/cloudflare-adapters`. The implementation may not silently omit an accepted Gmail,
standards IMAP/SMTP, Microsoft Graph, cursor, refresh, send-capability or discard operation.

## Resolver release provenance

The resolver is released independently from the existing control-plane D2 artifact; it is not
rebuilt ad hoc during environment provisioning and is not silently added under the existing D2
release identity. A permanent resolver release build starts only from accepted `main` and records an
immutable release ID, exact source SHA, resolver Worker digest, resolver-D1 migration-manifest
digest, canonical resolver-config digest and pinned build toolchain.

The protected resolver promotion workflow consumes that artifact without rebuilding it. Staging and
production receive the same resolver bits; only controlled resource identities and secret values
differ. A failed resolver staging deployment or verification cannot advance the resolver artifact to
production. The implementation unit owns this release/promotion machinery and its positive/negative
evidence before any external resolver deployment is authorized.

## Resolver credential storage

Resolver state uses a dedicated D1 database per environment under the `RESOLVER_DB` binding. It is
not the business/catalog D1. Credential payloads, OAuth state/PKCE material and provider cursors are
encrypted before D1 persistence with AES-256-GCM, a fresh 96-bit nonce and authenticated context
covering tenant, provider, record kind, logical identifier and key version. Lookup identifiers use
HMAC-SHA-256 so a resolver-database disclosure does not expose usable raw handles.

The encryption keyring and handle-HMAC key are Worker secrets. Key versions are explicit; writes use
the active version, reads permit only the bounded retained rotation set, and retirement requires a
verified re-encryption/reconciliation pass. OAuth state is short-lived and single-use. Credential
discard/revoke is idempotent. Neither the business D1 nor audit/outbox/browser/ordinary logs may
receive credential plaintext, encrypted credential blobs, PKCE material, authorization codes or
provider tokens.

Provider application client IDs and callback URIs are non-secret resolver variables; Google and
Microsoft client secrets remain resolver Worker secrets. Redirect URIs terminate only at the
environment's control-plane callback surface and must exactly match the provider registrations.

## First-deploy secret ceremony

Each GitHub Environment owns two additional protected JSON secret documents:

- `CLOUDFLARE_RESOLVER_SECRETS_JSON` — exact resolver Worker secret-name/value object;
- `CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON` — exact control-plane Worker secret-name/value object.

The implementation checker must reject missing, extra, placeholder or cross-environment-identical
documents. Values are written only to mode-0600 files under `$RUNNER_TEMP`, used with the pinned
Wrangler `--secrets-file` option during the corresponding Worker upload, and removed by an installed
trap. They are never printed, hashed into public evidence, stored in the deploy manifest or uploaded
as artifacts. Verification records secret names only.

The caller-auth key appears in both Worker secret documents within one environment, but staging and
production values must differ. The bootstrap API token may mint/configure the substrate and scoped
steady-state tokens, but it must never be installed as `CLOUDFLARE_API_TOKEN`. Steady-state deploy
tokens remain environment-specific and limited to `Workers Scripts Write`, `Workers Routes Write`,
`D1 Read`, `Workers R2 Storage Read` and `Queues Read`. The bootstrap authority is separately held
and limited to the write counterparts needed to create those resources, plus `Access: Apps and
Policies Write`, `Access: Service Tokens Write` and `API Tokens Write`. Every permission is narrowed
to the selected account/zone/environment resources where Cloudflare supports resource scoping.

## Ordered implementation and provisioning

The next repository unit implements and tests the resolver, signed caller protocol, dedicated D1
migrations/config, and exact first-deploy secret handling. It must not provision real resources.

Only after that implementation is accepted may the external staging ceremony execute this order:

1. select one accepted immutable resolver release artifact without rebuilding it;
2. create the dedicated staging resolver D1;
3. classify/create the staging catalog D1 and apply only the accepted D3A empty-target bootstrap;
4. create isolated staging R2 and four queues;
5. mint isolated R2 S3 credentials for the staging bucket;
6. deploy the accepted resolver artifact with its complete secrets file and verify
   name-only inventory;
7. reserve the staging hostname and create its Access application, Service Auth policy and token;
8. store the six exact GitHub staging Environment inputs;
9. run D3, which performs the first control-plane deploy from the exact D2 artifact with
   `--secrets-file`, the real resolver binding and the dedicated custom domain, then smoke-tests and
   attests that same deployment.

There is no pre-D3 placeholder or preparatory control-plane deployment. The first control-plane
version visible in Cloudflare is the exact accepted D2 artifact deployed by D3.

Production resource creation remains forbidden until the unchanged staging release has accepted
evidence and a separately authorized production ceremony repeats the isolation checks. No automatic
resource provisioning, resource renaming, shared credential, placeholder value or dummy Worker is
allowed.

## Acceptance boundary

This authority PR contains only the machine authority, this decision record, its fail-closed checker
and permanent gate wiring. It does not add the implementation marker, resolver source/config/D1
migrations, runtime secret values, Cloudflare identifiers or `openapi/v1/**` changes. Phase 2J
remains blocked and `production_ready=false` remains mandatory.
