# Cloudflare Access verification boundary

The Step 4 Worker parses typed Access JWT claims, validates issuer, audience,
expiry and not-before bounds, selects an RSA/RS256 signature JWK by `kid`, and
uses the Workers WebCrypto runtime with a verify-only imported key. A verified
external identity is then resolved through the tenant-scoped D1 membership
repository before an `ActorContext` can be constructed.

Tests use a deterministic signature verifier and fake identity adapter to prove
that both paths produce the same verified external identity. Missing, invalid,
expired or unresolvable identities remain disclosure-neutral at the HTTP
boundary.
