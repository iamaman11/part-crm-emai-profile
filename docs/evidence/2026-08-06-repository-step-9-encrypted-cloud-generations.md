# Repository Step 9 — Encrypted Cloud Generations Evidence

**Дата:** 2026-08-06  
**Статус:** accepted bounded synthetic implementation evidence  
**Baseline:** `e596fbe5692aa5b020700e7462c608dd23bacc15`  
**Accepted source head:** `73685241a6d70cf6d8ec80210d94b66cf37b1b45`  
**Pull request:** #30  
**Tracking issue:** #29  
**Exact-head Quality Gate run:** `31072625808`  
**Exact-head Encrypted Generation Gate run:** `31072625852`  
**Exact-head Local Profile regression run:** `31072625849`  
**Exact-head Runtime Bundle regression run:** `31072625892`  
**Squash merge:** `bc5286e3fea767acf955fb2622dab6221ecf1c3b`

## 1. Реализованный Boundary

Step 9 introduces a pure repository-local encrypted-generation domain for
synthetic profile payloads. It defines a versioned authenticated chunk container,
immutable generation-object semantics, restore verification, pointer
compare-and-swap, rollback, corruption quarantine, orphan planning and
metadata-only support output.

The implementation is a cryptographic design and test candidate. It does not
connect to production R2 or D1, does not generate or deliver production keys and
does not promote ADR-0006 from `proposed`.

## 2. Cryptographic Container

The workspace pins the following pure-Rust dependencies exactly:

- `chacha20poly1305 = 0.11.0` for XChaCha20-Poly1305;
- `sha2 = 0.11.0` for SHA-256;
- `zeroize = 1.9.0` for bounded sensitive-memory cleanup.

The caller supplies a synthetic 32-byte generation DEK and a 128-bit nonce
prefix. The container layer contains no hidden RNG or key-provider fallback.
Each encrypted record derives a 192-bit XChaCha nonce from the prefix and a
monotonic 64-bit record index.

Canonical metadata authenticates tenant, profile, generation, optional base
generation, opaque key ID, nonce prefix, chunk size, plaintext byte count and
plaintext SHA-256 digest. Every chunk binds record type, index and plaintext
length as additional authenticated data. Exactly one authenticated final record
is required.

The deterministic container regression vector has SHA-256 identity:

```text
5bd226f83e0a8cf37df0f818076ae466256348c9658714a7883ca9957d758616
```

This vector is a repository regression aid, not an independent cryptographic
certification.

## 3. Strict Parsing And Authentication

The parser enforces bounded container, metadata, key-ID, chunk and plaintext
sizes. It rejects unsupported versions, invalid identifiers, malformed lengths,
missing or duplicate final records, reordered indices and bytes after the final
record.

Tests prove fail-closed behavior for:

- metadata mutation;
- encrypted chunk mutation;
- truncation;
- reordered chunk indices;
- final-record mutation;
- wrong key and expected-identity mismatch;
- invalid magic and unsupported version;
- oversized metadata length;
- trailing bytes after the final record.

Restore does not allocate a plaintext buffer from the unauthenticated metadata
size. The bounded buffer grows only after each record authenticates and passes
its structural limits. Decrypted temporary chunks and accumulated plaintext use
`Zeroizing` buffers.

## 4. Key And Nonce Memory Boundary

`GenerationDek` does not implement `Clone` or `Debug`, exposes no key-byte getter
and zeroizes its fixed-size byte array on drop.

Nonce reuse is registered against a non-exported SHA-256 domain derived from the
actual DEK bytes rather than against a caller-controlled key ID. A dedicated
regression proves that the same key bytes and nonce prefix are rejected even when
two different opaque key IDs are supplied.

The internal nonce-domain value is not cloneable, copyable or printable and is
zeroized on drop. This repository-local in-memory map is evidence only. A future
production implementation must persist nonce claims atomically with immutable
object publication.

Plaintext-bearing `OpenedGeneration` and `RestoreResult` do not implement
`Clone` or `Debug`. They expose borrowed plaintext only and keep owned buffers in
`Zeroizing` wrappers.

## 5. Immutable Generation Lifecycle

The fake immutable object store accepts a new generation object once. Repeating
the identical object is idempotent; using the same object key with different
bytes fails with `ImmutableConflict`.

Publication stages a generation, stores the immutable container, reopens and
verifies it, then marks the catalog record verified. A generation can become the
current pointer only after verification.

Pointer changes require the exact expected pointer version. Stale commits fail.
Rollback is limited to the retained rollback generation and also advances the
pointer version.

A cataloged container-digest mismatch quarantines the generation and prevents it
from becoming current. Supplying an incorrect DEK does not quarantine an
otherwise unchanged immutable object, avoiding a caller-triggered local denial
of service. A later restore with the correct key remains successful.

Orphan planning is deterministic and excludes both the current generation and
the retained rollback generation. Support output contains aggregate counts and
bytes only; generation IDs, key IDs, object paths and payloads are excluded.

## 6. Permanent Policy

`scripts/check-step9-encrypted-generation.py` requires the cryptographic,
lifecycle and sensitive-memory boundaries and exact dependency pins. It rejects
randomness hidden inside the pure container layer, diagnostic output macros,
legacy-profile paths, key-byte accessors, sensitive `Debug` derives and temporary
Step 9 workflows.

The committed negative fixture otherwise resembles the required Step 9 surface
but derives `Debug` for key material and prints it. The permanent gate proves the
fixture is rejected.

The architecture gate also registers the new crate as a pure domain that may
compile natively and for `wasm32-unknown-unknown` without Cloudflare, Windows,
Python, browser or storage SDK dependencies.

## 7. Permanent CI Result

All permanent workflows succeeded on exact source head
`73685241a6d70cf6d8ec80210d94b66cf37b1b45`.

### Quality Gate `31072625808`

- repository policy scripts compiled;
- all accepted architecture, contract, D1, identity/ACL, coordinator, Bridge,
  runtime and local-lifecycle gates remained green;
- rustfmt, warnings-denied Clippy and all native workspace tests passed;
- pure crates compiled for Workers WASM;
- D1 migration apply/replay/schema verification passed;
- Windows tests and non-empty release `profile-bridge.exe` verification passed;
- delivery-status validation and tracked-tree high-confidence secret scan passed;
- pinned Cloudflare Worker release build and artifact verification passed.

### Encrypted Generation Gate `31072625852`

#### Linux And WASM

- Step 9 policy compiled and passed;
- the deliberate key-output fixture was rejected;
- rustfmt and warnings-denied Clippy passed;
- all encrypted-generation tests passed;
- the pure encrypted-generation crate compiled for Workers WASM.

#### Windows

- all encrypted-generation lifecycle tests passed on the Windows runner.

### Regression Gates

- Local Profile Gate `31072625849` passed on Linux and Windows and rebuilt the
  verified Profile Bridge executable;
- Runtime Bundle Gate `31072625892` passed on Linux and Windows and retained fake
  Camouhost and Profile Bridge artifact evidence.

## 8. Доказанные Свойства

Within the synthetic repository boundary, the accepted evidence proves:

- exact cryptographic dependency pins compile on Rust `1.97.1`;
- deterministic container construction and the committed test vector are stable;
- metadata, chunk and final-record authentication fail closed on mutation;
- truncated, reordered, malformed, oversized and trailing input is rejected;
- plaintext byte count and SHA-256 digest are verified after authenticated open;
- plaintext is absent from canonical metadata and support output;
- DEK bytes are not printable or exported and are zeroized on drop;
- nonce reuse is rejected by actual DEK domain across key-ID aliases;
- plaintext-bearing result buffers are zeroizing and not debug-printable;
- restore does not preallocate from unauthenticated plaintext-size metadata;
- immutable conflicting writes are rejected;
- stale pointer updates and invalid rollback are rejected;
- cataloged corruption quarantines a generation and blocks pointer promotion;
- a wrong key does not quarantine an unchanged object;
- orphan planning protects current and rollback generations;
- the complete repository remains green on Linux, Windows and Workers WASM.

## 9. Defects Found And Corrected

During implementation and review:

- initial `rustfmt` and Clippy findings were corrected without lint suppression;
- the nonce registry was changed from caller-controlled key ID to a domain derived
  from actual DEK bytes;
- nonce-domain state was made non-cloneable/non-printable and zeroizing;
- plaintext-bearing result types lost `Clone` and `Debug` and adopted
  `Zeroizing` buffers;
- restore stopped preallocating from unauthenticated metadata;
- wrong-key authentication failure stopped quarantining a digest-matching object;
- malformed magic/version/metadata/trailing-byte proofs were added;
- the negative policy fixture was strengthened to fail specifically on sensitive
  output rather than on missing positive surface;
- temporary bootstrap/application workflows were removed before the accepted
  source head;
- a connector-authored no-tree-change retrigger produced normal permanent
  exact-head workflow evidence after the Actions-generated hardening commit.

The controlled hardening workflow run `31072532939` separately passed its policy,
negative fixture, Clippy and tests before committing the final memory-boundary
changes. Acceptance is based on the later permanent exact-head runs listed above.

## 10. Ограничения И Внешние Gates

This evidence does not prove:

- production entropy generation or nonce allocation across distributed writers;
- root key, tenant KEK or generation DEK wrapping and rotation;
- DPAPI/CNG/TPM-backed device delivery or device-scoped unwrap;
- atomic remote nonce-claim persistence with R2/D1 publication;
- real R2 immutable behavior, remote D1 pointer CAS or reconciliation;
- clean-environment restore after local state and keys are removed;
- offline escrow, dual control, account-loss recovery or revocation;
- independent cryptographic review, fuzzing campaign or side-channel analysis;
- real Camoufox, real legacy profiles or real user data;
- physical multi-device operation, trusted signing or production readiness.

No production credential, Cloudflare resource, real profile, mailbox content or
personal data was used. ADR-0006 remains `proposed_blocks_production` and
`production_ready` remains `false`.
