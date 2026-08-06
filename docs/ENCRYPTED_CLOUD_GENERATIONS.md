# Encrypted Cloud Generations Boundary

**Status:** Repository Step 9 implementation candidate  
**Tracking issue:** #29  
**Baseline:** `e596fbe5692aa5b020700e7462c608dd23bacc15`  
**ADR-0006:** proposed; production cloud generations remain blocked

## 1. Scope

This boundary defines a synthetic, provider-free encrypted generation container
and fake immutable cloud lifecycle. It exists to produce reviewable cryptographic
and state-machine evidence before any production key hierarchy, remote R2/D1
adapter or user profile is introduced.

The implementation uses exact pure-Rust dependencies:

- `chacha20poly1305 0.11.0` with XChaCha20-Poly1305;
- `sha2 0.11.0` with SHA-256;
- `zeroize 1.9.0` for bounded key-memory cleanup.

The container layer has no hidden entropy or key-provider dependency. A caller
must supply a synthetic 256-bit generation DEK, an opaque key ID and a unique
128-bit nonce prefix. The repository lifecycle rejects reuse of the same nonce
prefix with the same key ID for a different generation.

## 2. Canonical Container

The binary container is deterministic for identical metadata, key, nonce prefix
and plaintext. All integers are unsigned big-endian.

```text
magic                 8 bytes  "BPGC0001"
metadata_length       u32
metadata              canonical bytes
record*                authenticated chunk records
final_record           authenticated empty terminal record
```

Canonical metadata contains:

```text
container_version      u16 = 1
algorithm_id           u16 = 1
length-prefixed tenant ID
length-prefixed profile ID
length-prefixed generation ID
optional length-prefixed base generation ID
length-prefixed opaque key ID
nonce_prefix           16 bytes
chunk_size             u32
plaintext_size         u64
plaintext_sha256       32 bytes
```

Metadata is authenticated by every record through its SHA-256 digest. Plaintext,
DEK bytes, cookies, filenames, paths and profile contents never appear in
metadata.

## 3. Chunk Records

Each plaintext chunk is independently encrypted with XChaCha20-Poly1305:

```text
record_type            u8 = 1
chunk_index             u64, contiguous from zero
plaintext_length       u32
ciphertext_length      u32 = plaintext_length + 16
ciphertext_and_tag      bytes
```

The 24-byte XChaCha nonce is:

```text
nonce_prefix[16] || chunk_index_be[8]
```

The additional authenticated data is:

```text
SHA256(canonical_metadata) || record_type || chunk_index || plaintext_length
```

Chunk size is policy-bounded from 1 KiB through 1 MiB. Synthetic plaintext is
bounded to 64 MiB and total container size to 80 MiB. The current repository
implementation operates on bounded byte slices and produces independently
verifiable chunks; production streaming I/O remains a later adapter/review item.

## 4. Authenticated Final Record

Exactly one terminal record is required:

```text
record_type            u8 = 2
chunk_index             u64 = number of plaintext chunks
plaintext_length       u32 = 0
ciphertext_length      u32 = 16
authenticated_tag       encrypted empty message
```

The final record uses the next unused nonce and the same AAD construction. A
missing, duplicated, reordered, truncated or tampered final record fails closed.
Trailing bytes after the final record are rejected.

## 5. Restore Verification

Restore performs all of the following before returning plaintext:

- validates magic, version, algorithm and strict bounds;
- decodes typed tenant/profile/generation IDs and opaque key ID;
- requires requested identity and supplied key ID to match authenticated metadata;
- requires monotonically contiguous chunk indexes;
- authenticates every chunk and the final record;
- verifies reconstructed plaintext size;
- verifies reconstructed SHA-256 digest;
- verifies the cataloged SHA-256 digest of the immutable container object.

A correct key with corrupted bytes fails authentication or digest validation. A
wrong key with the same opaque key ID fails authentication. A caller supplying a
different key ID fails before decryption and does not quarantine the object.

## 6. Immutable Object Lifecycle

The fake repository uses the deterministic object key:

```text
tenants/<tenant-id>/profiles/<profile-id>/generations/<generation-id>.bpgc
```

An absent object may be created. Repeating the exact same bytes is idempotent.
Trying to write different bytes at the same object key fails with
`ImmutableConflict`; no overwrite path exists.

Catalog records move through:

```text
STAGED -> VERIFIED
STAGED | VERIFIED -> QUARANTINED
```

Only `VERIFIED` generations may become current. Restore corruption changes the
catalog record to `QUARANTINED`, leaves the current pointer unchanged and blocks
future commit.

## 7. Pointer CAS And Rollback

The pointer snapshot contains a monotonically increasing version, current
generation and one retained rollback generation. Commit and rollback require an
exact expected pointer version.

A successful commit moves the former current generation into the retained
rollback slot and increments the version. Rollback is accepted only for the
retained rollback generation, swaps current and rollback, and increments the
version. Stale versions fail without mutation.

This repository-local fake is a state-machine proof. Production D1/R2 pointer
persistence, distributed transactions and reconciliation must preserve the same
CAS and immutable-object invariants.

## 8. Orphan Reconciliation

Orphan planning is deterministic by generation ID and accepts a caller-supplied
age cutoff. The current and retained rollback generations are always protected,
regardless of age or status. The plan reports candidate IDs and reclaimable
container bytes; it does not delete objects.

Production reconciliation must add durable tombstones, retry/idempotency,
provider listing pagination and evidence that pointer state was re-read before
deletion.

## 9. Secret And Support Boundary

`GenerationDek` does not implement `Clone` or `Debug`, exposes no key-byte getter
and zeroizes its fixed-size byte array on drop. Source policy rejects random/key
provider coupling, debug/output macros, legacy profile paths and temporary
lockfile bootstrap workflows in the pure crate.

The support summary emits only aggregate counts, aggregate encrypted bytes,
pointer version and boolean current/rollback presence. It does not accept or emit
generation IDs, object keys, key IDs, nonces, plaintext digests or profile data.

## 10. Permanent Evidence

`.github/workflows/encrypted-generation-gate.yml` runs on pull requests and
`main`:

- Step 9 policy compilation and positive enforcement;
- deliberate key-output negative fixture;
- rustfmt and Clippy with warnings denied;
- adversarial encrypted generation tests on Linux and Windows;
- pure crate compilation for `wasm32-unknown-unknown`.

The repository Quality Gate continues to run the complete workspace, existing
architecture/contract/D1/Worker/Windows tests and release artifact checks.

## 11. Explicit Limitations

This boundary does not accept ADR-0006 and does not prove:

- independent cryptographic review or certification;
- fuzzing, side-channel analysis or memory-forensics resistance;
- production entropy, nonce allocation or key providers;
- root wrapping key, tenant KEK or generation DEK wrapping;
- device key delivery, revocation or multi-device operation;
- offline escrow restore, key rotation or account-loss recovery;
- production zeroization of allocator, OS, crash-dump or swap copies;
- remote Cloudflare R2/D1 behavior, consistency, lifecycle or cost;
- real Camoufox, real browser profiles or legacy-profile compatibility;
- production privacy, disaster recovery or production readiness.

All keys, IDs, plaintext and objects used by tests are synthetic. No production
credential, remote resource, mailbox content, personal data or legacy profile is
used. `production_ready` remains `false`.
