# #802 S1 — original Windows read-only pre-installation observation

This is an **observation interface, not an installer, rollback operation or B7 acceptance**.

## Command (Windows, new candidate Bridge executable, outside the installed app)

```powershell
profile-bridge.exe --s1-observe "<extracted signed installer inputs>" "<existing installation root>" ["<existing isolated profile-copy root>"]
```

An optional third argument names an **already-existing**, byte-for-byte isolated
copy of the `profiles` root **outside the installation tree**. S1 never makes
this copy. It compares the full relative directory and file names, contents
(including WAL and pending N+1), and materialization evidence under the live
OS-backed original root lock. A mismatch is UNKNOWN and preserves both copies.
An exact match is useful independent byte evidence, but **not** a Firefox
reopen/WORKING certificate and does not by itself grant READY.

The first argument must be an existing scratch directory with the exact installer-owned
`installation.json`, `windows-delivery-manifest.json`, and
`windows-delivery-signature.json`. The second must be the *existing* canonical root,
normally `%LOCALAPPDATA%\Alegria\ProfileBridge`. Do not run `--delivery-prepare`
to obtain these inputs. The local agent must use a verified extraction of the
same release and verify the installer SHA-256 independently.

The command prints exactly one JSON observation. Exit **3** means
`UNKNOWN` or `BLOCKED`, *not* success; it never activates a release or
starts the browser. The read-only command does **not** write the regular application
diagnostic journal. Save the output under an operator-controlled directory outside
the installation root.

If `PROFILE_INVENTORY_UNVERIFIED` is reported, **do not re-inventory manually
or delete anything**. Read the structured `inventory_failure` in the same JSON:
`code` is the exact failed operation (for example `READ_DIRECTORY`,
`OPEN_FILE`, `READ_FILE`, `LINK_REJECTED` or `CONCURRENT_MODIFICATION`);
`path` is the original full Windows path; `detail` and `os_error`
are the underlying Windows I/O diagnostic when available. Existing invalid
materialization sidecars are listed in `inventory.sidecar_failures` with the
exact path and reason. An invalid optional isolated copy is reported in
`isolated_copy_failure`. These are **observations, never repair commands**.
All ordinary Windows filenames are hashed as native UTF-16LE names, even when
the file name cannot be represented as UTF-8. No copy, key, lock, pending item
or profile is changed to produce the diagnostics.

This first source slice:
- Opens the existing materialization root, Windows device CNG key and installed
  delivery store without creating or repairing any of them. The existing CNG key
  signs and locally verifies a domain-separated challenge; the public SPKI SHA-256
  fingerprint is reported for independent device registration matching.
  **Local PoP alone does not certify the server's registered public key**.
- Acquires the same root-exclusive native activation lock and uses the existing
  per-generation native Firefox lock check; retained inert Bridge markers are not
  deleted or mistaken for an active OS writer.
- Uses a separate no-repair delivery-journal reader that refuses pending recovery evidence without moving or finalizing it.
- Inspects every scoped BPAT / LOCAL_UNSYNCED in the original root, including N+1 without a browser-sidecar, and rejects torn or foreign pending siblings without cleanup.
- Enumerates and streams SHA-256 over every regular file in the original local
  profiles, including WAL and pending candidates, under that exclusion boundary;
  refuses links, unreadable entries, unstable files and oversized inventories.
- Reuses the canonical local materialization-binding verifier on existing sidecars.
- Reopens the original delivery and previous retained delivery using the existing
  full byte-level stage verifier; does not modify an installation record.
  The `previous_delivery_rollback` report field is **not** browser-profile LKG
  and cannot satisfy `independent_recovery`.
- Verifies installed origin against signed bundle origin and executes existing CMS,
  signer admission, downgrade/replay and compatibility policy with no download or
  installation step.

**Not yet a clearance:** local generation CURRENT/WORKING/LKG authority and the
recoverability of an independent profile backup have no complete locally available
non-mutating proof. Verification of an older Bridge release is not verification
of a restorable browser-profile generation. The report emits `UNKNOWN` for these obligations and returns
a nonzero process exit. It must not be turned into `READY` just because files
exist or because an older application release is present.

The installer must reacquire `DeliveryActivationGuard` at its own activation
boundary; S1 observations cannot hold ownership across an operator delay.

## Negative tests and scope

The branch tests missing CNG key does not get created, existing key remains the
same, missing root is never created, corrupt marker stays untouched, a live native
Firefox handle blocks while a stale bridge marker remains intact, no WAL/pending
bytes are modified, absent inputs never produce a positive verdict, and the CLI
rejects extra and relative paths. Existing delivery-policy tests separately exercise
invalid CMS, downgrade, replay and incompatibility. Native Windows CI is mandatory
before any release.

No application profile, old installer, Cloudflare provider or production setting
is changed by source work. Source tests do not imply a real original-device S1 PASS.
