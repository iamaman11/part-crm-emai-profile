# #802 S1 — original Windows read-only pre-installation observation

This is an **observation interface, not an installer, rollback operation or B7 acceptance**.

## Command (Windows, new candidate Bridge executable, outside the installed app)

```powershell
profile-bridge.exe --s1-observe "<extracted signed installer inputs>" "<existing installation root>"
```

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

This first source slice:
- Opens the existing materialization root, Windows device CNG key and installed
  delivery store without creating or repairing any of them.
- Acquires the same root-exclusive native activation lock and uses the existing
  per-generation native Firefox lock check; retained inert Bridge markers are not
  deleted or mistaken for an active OS writer.
- Enumerates and streams SHA-256 over every regular file in the original local
  profiles, including WAL and pending candidates, under that exclusion boundary;
  refuses links, unreadable entries, unstable files and oversized inventories.
- Reuses the canonical local materialization-binding verifier on existing sidecars.
- Reopens the original delivery and previous retained delivery using the existing
  full byte-level stage verifier; does not modify an installation record.
- Verifies installed origin against signed bundle origin and executes existing CMS,
  signer admission, downgrade/replay and compatibility policy with no download or
  installation step.

**Not yet a clearance:** local generation CURRENT/WORKING/LKG authority and the
recoverability of an independent profile backup have no complete locally available
non-mutating proof. The report emits `UNKNOWN` for these obligations and returns
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
