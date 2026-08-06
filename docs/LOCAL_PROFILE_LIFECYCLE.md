# Local Profile Lifecycle Boundary

**Status:** Repository Step 8 implementation candidate  
**Tracking issue:** #26  
**Baseline:** `ef8777b69ff6c89c176b79b04adecce17bc6c68e`

## 1. Scope

This boundary governs repository-local materialization, inventory, writer locking,
clone-only recovery, dirty-state preservation, forgotten-window decisions, quota
planning and privacy-safe support metadata for Profile Bridge.

It is intentionally provider-free and operates only on synthetic test
generations created beneath an explicitly marked local root. It does not read,
repair, migrate or execute the legacy profile corpus.

## 2. Safe Materialization Root

A local root must be absolute, must be a real directory rather than a symbolic
link, and must contain the exact `.profile-platform-root` marker. A non-empty
unmarked directory is rejected rather than adopted.

Generation paths are composed only from opaque typed tenant, profile and
generation IDs:

```text
<marked-root>/<tenant-id>/<profile-id>/<generation-id>/
```

Each generation contains an exact `.profile-generation` marker. Existing targets,
symlinked path components, missing markers and marker mismatch fail closed.
Email addresses and user-selected labels are never path segments.

## 3. Bridge Writer Lock

Profile Bridge owns only `.profile-platform.lock`. Acquisition uses atomic
create-new semantics and records the typed device ID plus monotonic local epoch.
A second writer receives `LockBusy`.

Release is explicit and verifies exact ownership content before deleting the
Bridge-owned lock. There is deliberately no automatic `Drop` cleanup: a crash
leaves evidence instead of silently declaring the workspace clean.

Browser-owned lock files such as `.parentlock`, `parent.lock` and `lock` are
ordinary inventory entries. Application code does not delete them. A permanent
negative policy fixture proves that browser-lock deletion is rejected.

## 4. Deterministic Inventory

Inventory recursively accepts regular files and directories only. It rejects
symbolic links, non-UTF-8/unsafe relative paths, special files, excessive path
length and excessive file count.

Entries are sorted by normalized `/` relative path and contain:

- relative path;
- byte length;
- deterministic standard-library content digest.

The aggregate inventory includes total bytes and a deterministic digest over the
ordered entries. The accepted Step 8 digest is a repository-test identity aid,
not a cryptographic authenticity claim. Encrypted cloud generations in Step 9
must use reviewed cryptographic digests and AEAD.

Bridge control files are excluded from logical inventory. Browser-owned lock
files remain included.

## 5. Clone-Only Recovery

`RecoveryClone::create` records the source inventory, creates a new generation
with a new generation ID, copies only accepted regular-file inventory entries,
and compares clone inventory with the recorded source inventory.

The source is inventoried again after clone creation. Any source change fails the
operation. Subsequent integrity verification is exposed only through
`RecoveryClone::verify_clone_only`; mutation of the clone is detected while the
source remains unchanged.

This step does not claim browser SQLite semantic repair. Real database integrity,
recovery open and compatibility checks remain future work and must still execute
on a clone.

## 6. Local Generation State Machine

The local record uses explicit states:

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

Opening requires a held local lock. Graceful close produces `DIRTY_LOCAL`, not a
clean or evictable state. Crash produces `RECOVERY_REQUIRED`. Recovery completion
requires an explicit clone-integrity result. Eviction is accepted only from
`SYNCED_EVICTABLE` while unlocked.

## 7. Forgotten-Window Policy

A valid policy requires strictly increasing non-zero thresholds:

```text
warn-after < drain-after < hard-ttl
```

For an active generation, the decision is monotonic:

- no action before the warning threshold;
- warn after idle warning threshold;
- typed drain after idle drain threshold;
- force-close decision at hard session TTL.

A clock regression fails closed.

## 8. Quota Policy

Quota planning calculates total local bytes and selects only unlocked
`SYNCED_EVICTABLE` generations. Candidates are deterministic: oldest activity
first, then generation ID.

The following are never quota candidates:

- `IN_USE`;
- `DIRTY_LOCAL`;
- `RECOVERY_REQUIRED`;
- `QUARANTINED`;
- any locked generation.

A quota plan reports whether enough safe bytes are reclaimable; it does not
silently broaden eligibility when the quota cannot be satisfied.

## 9. Privacy-Safe Support Summary

The support summary accepts local state records and failure counts only. Its
stable text output contains aggregate generation counts, aggregate bytes, state
counts and inventory-failure count.

It does not accept or emit generation IDs, paths, filenames, email addresses,
profile contents, proxy values, tokens or secret handles.

## 10. Permanent Evidence

`.github/workflows/local-profile-gate.yml` runs on pull requests and `main`:

- Step 8 policy compilation and positive enforcement;
- deliberate browser-lock deletion negative fixture;
- rustfmt and Clippy with warnings denied;
- local lifecycle tests on Linux and Windows;
- Windows release Profile Bridge build and non-empty executable check.

The existing repository Quality Gate continues to run the complete workspace,
D1, Worker, Windows and prior-step regression suite.

## 11. Explicit Limitations

This boundary does not prove:

- real Camoufox execution or compatibility;
- mutation, repair or migration of any legacy profile;
- production filesystem snapshot semantics;
- browser SQLite/IndexedDB semantic recovery;
- cryptographic inventory authenticity;
- production device-key protection;
- trusted Windows signing or installer behavior;
- encrypted R2 generations, restore or rollback;
- physical multi-device behavior;
- production readiness.

`docs/status.json` remains unchanged until the implementation PR has exact-head
green permanent workflows, review is complete and a separate evidence-sync PR is
accepted.
