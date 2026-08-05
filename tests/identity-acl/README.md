# Repository Step 4 identity and ACL evidence boundary

The permanent Step 4 suites prove the authenticated identity, membership,
client and profile authorization slice without using remote resources,
production credentials or real user data.

## Positive coverage

- Cloudflare Access JWT claims and the deterministic fake identity adapter
  produce the same verified external identity contract before membership
  resolution creates an application `ActorContext`.
- Owner bootstrap is confined to an empty tenant boundary and is idempotent.
- Governed owner transfer, invitation creation, membership lifecycle, profile
  assignment and explicit client/profile grant commands use transactional D1
  command guards. Aggregate state, idempotency, audit and outbox records commit
  or roll back together.
- Active tenant owners and explicitly granted active members can read covered
  client/profile resources through tenant-scoped projections.
- Historical profile/client assignments remain separate from authorization.

## Negative coverage

`fixtures/assignment-authorizes.sql` deliberately projects an active historical
profile/client assignment as authorization. The permanent Quality Gate executes
the Step 4 ACL suite with this fixture and requires the suite to fail.

Missing membership, suspended and revoked membership, insufficient grants,
foreign tenant resources and missing resources all resolve to the same neutral
`not_found` disclosure shape for covered reads. Stale versions, invalid owner
transfer, last-owner removal and incomplete mutation envelopes abort without
partial writes.

## Technical gate

- Baseline: `5667779d59413d4736e58d6eb83a892dfdd2f522`
- Technical head: `5b187ebd786cdca068ed209b79642ecaaebe3be6`
- Permanent Quality Gate: `31052479944`
- Green jobs: `Rust Linux and WASM`, `D1 Catalog Migrations`, `Rust Windows`,
  `Cloudflare Worker Release Build`

This evidence is repository-local only. It does not prove remote Cloudflare D1
deployment, staging credentials, production secrets, real profile or mailbox
data, multi-device behavior or production readiness. `production_ready` remains
false; authoritative completion evidence is published only by the bounded
post-merge evidence-sync change.
