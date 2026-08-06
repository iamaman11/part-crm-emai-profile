# Repository Step 8 — Local Profile Lifecycle Evidence

**Дата:** 2026-08-06  
**Статус:** accepted implementation evidence  
**Baseline:** `ef8777b69ff6c89c176b79b04adecce17bc6c68e`  
**Accepted source head:** `dbf3770f58c45b9f247579191b2b2d5f342c1bc8`  
**Pull request:** #27  
**Tracking issue:** #26  
**Exact-head Quality Gate run:** `31068856595`  
**Exact-head Local Profile Gate run:** `31068856619`  
**Exact-head Runtime Bundle regression run:** `31068856601`  
**Squash merge:** `eb55f67d742661019438891764c388dc19f62d96`

## 1. Реализованный Local Lifecycle Boundary

Step 8 adds a bounded local Profile Bridge lifecycle for synthetic generations:
marked materialization roots, opaque tenant/profile/generation paths, an atomic
Bridge-owned writer-lock protocol, deterministic regular-file inventory,
clone-only recovery evidence, explicit dirty/recovery states, forgotten-window
policy, safe quota planning and metadata-only support summaries.

The implementation does not read, open, repair, migrate or execute the legacy
profile corpus. It does not claim real Camoufox compatibility, browser database
repair, production cryptography or production readiness.

## 2. Safe Materialization

A materialization root must be absolute, must resolve to a real directory rather
than a symbolic link and must contain the exact `.profile-platform-root` marker.
An existing non-empty unmarked directory is rejected rather than adopted.

Generation workspaces are composed only from typed opaque tenant, profile and
generation IDs. Each generation has an exact `.profile-generation` marker.
Existing targets, symlinked path components, missing markers, marker mismatch and
special filesystem objects fail closed.

Control marker reads verify that the marker itself is a regular file. A symlink
that points to otherwise valid marker content is rejected.

## 3. Bridge-Owned Writer Lock

Profile Bridge owns only `.profile-platform.lock`. Acquisition uses filesystem
`create_new` semantics and records a typed device ID plus a non-zero local epoch.
A second acquisition for the same generation fails with `LockBusy`.

Release is explicit and requires exact ownership content. A tampered lock is not
deleted and fails with `LockOwnershipMismatch`. There is intentionally no
automatic `Drop` cleanup: process failure leaves evidence rather than silently
representing the generation as unlocked or clean.

Browser-owned `.parentlock`, `parent.lock` and `lock` files are never blindly
deleted. Dedicated positive tests prove preservation, and a committed negative
fixture containing browser-lock deletion is rejected by the permanent policy.

## 4. Deterministic Inventory

Inventory recursively accepts regular files and directories only. Symbolic links,
special files, unsafe or non-UTF-8 relative paths, excessive path length and
excessive file count are rejected.

Entries are normalized with `/`, sorted deterministically and record relative
path, byte length and deterministic content identity. Aggregate total bytes and
an ordered inventory identity are produced. Bridge control files are excluded
from logical inventory, while browser-owned runtime lock files remain visible.

The Step 8 standard-library digest is explicitly a deterministic repository-test
identity aid, not cryptographic authenticity. Step 9 encrypted cloud generations
must introduce reviewed cryptographic digests and streaming AEAD.

## 5. Clone-Only Recovery

Recovery records the source inventory, creates a distinct generation with a new
generation ID, copies only accepted regular-file entries, compares clone and
source inventories and inventories the source again after clone creation.

Source mutation during cloning fails with `SourceChanged`. Clone mutation after
creation fails with `CloneChanged`. Tests prove that integrity checks operate on
the clone and that the original synthetic source remains unchanged.

This evidence does not claim SQLite, IndexedDB or browser semantic repair. Any
future database recovery must still run on a clone and preserve the source.

## 6. Generation State, Forgotten Windows And Quota

The accepted local state machine distinguishes:

```text
MATERIALIZED_CLEAN
  -> IN_USE
  -> DIRTY_LOCAL
  -> SYNCED_EVICTABLE
  -> EVICTED

IN_USE
  -> RECOVERY_REQUIRED
  -> MATERIALIZED_CLEAN | QUARANTINED
```

Graceful close produces `DIRTY_LOCAL`, not an evictable state. Crash produces
`RECOVERY_REQUIRED`. Eviction is valid only from unlocked
`SYNCED_EVICTABLE`.

Forgotten-window policy requires strictly increasing non-zero warn, drain and
hard-TTL thresholds. Tests prove no-action, warning, drain and force-close
progression and fail closed on clock regression.

Quota planning is deterministic by oldest activity then generation ID. Dedicated
tests prove that dirty, recovery-required, actively in-use and locked synced
generations are not selected. If safe candidates cannot reclaim enough bytes,
the plan reports that result rather than broadening eligibility.

## 7. Privacy-Safe Support Output

The support summary accepts aggregate local records and inventory-failure counts.
Its stable output contains only total generation count, total bytes, state counts
and failure count.

Tests prove that generation IDs, paths, filenames, email addresses and secret
markers are not emitted. No profile payload, cookie, mailbox content, proxy value
or credential handle is accepted by the summary type.

## 8. Permanent CI Result

All permanent workflows succeeded on exact source head
`dbf3770f58c45b9f247579191b2b2d5f342c1bc8`.

### Quality Gate `31068856595`

- Step 0–7 architecture, contract, D1, identity/ACL, coordinator, Windows Bridge
  and runtime policies remained green;
- rustfmt, Clippy with warnings denied and all native workspace tests passed;
- pure crates compiled for Workers WASM;
- D1 migrations applied, replayed and verified;
- Windows native tests and non-empty release `profile-bridge.exe` passed;
- Cloudflare Worker WASM check, pinned release build and artifact verification
  passed;
- delivery-status validation and tracked-tree high-confidence secret scan passed.

### Local Profile Gate `31068856619`

#### Linux

- Step 8 policy source compiled and passed;
- deliberate browser-lock deletion fixture was rejected;
- rustfmt and Profile Bridge Clippy passed;
- all local profile lifecycle tests passed.

#### Windows

- all local profile lifecycle tests passed;
- release Profile Bridge executable rebuilt and remained non-empty.

### Runtime Bundle Gate `31068856601`

- Linux and Windows Step 7 runtime-bundle regressions passed;
- fake Camouhost lifecycle remained green;
- Windows Profile Bridge release executable remained present and non-empty.

## 9. Доказанные Свойства

The accepted repository evidence proves within the synthetic local boundary:

- unmarked non-empty roots are not adopted;
- typed opaque IDs determine generation paths;
- symlinked roots, components, markers and inventory entries fail closed;
- a second Bridge writer cannot acquire the same lock;
- release deletes only an ownership-matching Bridge lock;
- browser-owned runtime lock files are preserved;
- regular-file inventory is deterministic;
- recovery creates and verifies a distinct clone;
- source and clone mutations are detected;
- graceful close preserves `DIRTY_LOCAL`;
- crash preserves `RECOVERY_REQUIRED`;
- dirty, active, recovery-required and locked generations are not quota victims;
- forgotten-window decisions progress from warn to drain to force-close;
- support output contains aggregate metadata only;
- the complete repository still builds and tests on Linux, Windows and Workers
  WASM with verified Profile Bridge and Cloudflare Worker release artifacts.

## 10. Defects Found And Corrected

- initial formatting differences were corrected from exact `rustfmt --check`
  output;
- the monolithic module was split into filesystem, lifecycle and test modules for
  stable formatting and review;
- control markers were strengthened to require regular files after review found
  that valid content behind a symlink could otherwise be followed;
- lock tampering evidence was added so ownership mismatch leaves the file intact;
- the quota test was strengthened with a distinct actively in-use generation;
- browser-lock tests and cleanup helpers were separated so the existing
  conservative Step 6 deletion scanner remains unchanged and green.

## 11. Ограничения И Внешние Gates

This evidence does not prove:

- a kernel advisory lock or hostile local-process race resistance beyond the
  accepted atomic Bridge lock-file protocol;
- real Camoufox execution, rendering, fingerprint behavior or profile
  compatibility;
- semantic SQLite, IndexedDB or browser crash repair;
- execution, repair, migration or mutation of any legacy user profile;
- cryptographic inventory authenticity or encrypted R2 generations;
- production DPAPI/CNG/TPM device-key protection;
- trusted Windows signing, installer or update rollback;
- remote Cloudflare staging behavior or physical multi-device operation;
- production privacy, disaster recovery or production readiness.

No production credential, remote resource, real user profile, mailbox content or
personal data was used. All paths, IDs, files and lifecycle records were
synthetic. `production_ready` remains `false`.
