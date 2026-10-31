# #802 — one-cycle signed Windows update and physical B7 acceptance

This runbook executes the already-owned W01–W08, S01–S17 and K1–K3 of
[#802](https://github.com/iamaman11/part-crm-emai-profile/issues/802);
it does not create another acceptance authority or test mode.

## What the installer now guarantees (and what it does not)

The native signed installer, not the read-only S1 inspector, owns the update
decision. It checks exact detached CMS release identity, compatible release
floor, existing durable delivery state, and an exclusive OS-backed root
admission with per-generation native Firefox locks. An open/writer-owned
browser is a typed failure; a dead Bridge process and inert marker are not
mistaken for a live writer.

**Before any update-state/staging/activation change**, under that same
exclusive root admission, the native installer creates an independent,
immutable, bounded-streaming byte rescue when the original profile root has
data. Location (for the standard installation):

`%LOCALAPPDATA%\Alegria\ProfileBridge-recovery\snapshot-v1\profiles`

The snapshot preserves **every ordinary profile file**, including SQLite WAL,
IndexedDB, local pending, cookies and session data. It creates its own
validated materialization root control marker, never copies the actively
OS-locked source marker via a second handle, and verifies full source and copy
inventories with SHA-256. A receipt inside `snapshot-v1` is synced before
atomic publication. A published verified rescue is never overwritten or
pruned automatically on a later update. Interrupted unpublished stages are
retained for diagnosis, not silently adopted/deleted. A partially readable
source, locked WAL, insufficient storage, corrupt receipt, reparse or active
writer aborts activation; old executable, original bytes and existing
rescue remain untouched. The installer reacquires native exclusion at
activation to prevent a preflight/activation TOCTOU race.

This is **same-device byte-level disaster evidence**, not an asserted
Firefox WORKING rollback or cloud-generation authority. A nonexportable
Windows CNG private key is **not backed up** into an unencrypted folder;
cross-device/new Windows recovery requires separate authorized key/cloud
policy. A user who requires a browser-WORKING certificate must open an
isolated disposable test clone with the same pinned Camoufox and verify
the original test markers, *without claiming ownership or committing cloud
state*. Do not risk the sole original profile for this test.

S1 `--s1-observe` remains a read-only audit and may correctly emit
`UNKNOWN` for cloud generation authority forever. **It is not a mandatory
installer gate**. The installer is allowed to complete an inert signed
binary update while Coordinator is unavailable, because it does not
perform a browser Profile Open or remote generation mutation. The existing
machine-authenticated
`GET /bridge/v1/tenants/{tenantId}/profiles/{profileId}/coordinator`
is used by the ordinary Bridge/Worker lifecycle; it requires current CNG
device identity, actor membership and Profile ACL before any browser
writer or Save/Commit is authorized. D1 `READY` alone is not proof.

## What to do on the original Windows — one ordinary installation

1. Verify accepted main/release, local installer SHA-256 and that neither
   Bridge's browser nor Firefox currently owns a native lock. Do not reset
   local markers, CNG, WAL or pending snapshots. Do not run S1 to demand a
   made-up `READY`. S1 optional audit only; capture its genuine `UNKNOWN`.
2. Download and **run the exact published signed Setup once**, through the
   ordinary Inno wizard. It automatically performs CMS/release validation,
   root lock, immutable rescue, staging, exact installation and activation.
   On failure preserve installer stdout/stderr, existing state and backup
   paths; never do a blind reinstall/delete-to-retry.
3. Inspect the installer's allowlisted
   `PROFILE_BRIDGE_RESCUE_STATUS_V1=NEW_RESCUE_BYTES_VERIFIED`,
   `EXISTING_RESCUE_BYTES_VERIFIED` or `EMPTY_NEW_PROFILE_ROOT` event
   in the ordinary setup log; verify
   the receipt, original+rescue inventories, persistent CNG public identity
   and exact installed Bridge/runtime/delivery components. Do not confuse
   old Bridge release LKG with a browser WORKING snapshot. If installer
   returned `BRIDGE_RESCUE_FAILED`, classify its exact reason rather than
   claiming install success.
4. From an ordinary authorized browser session open the Profile Operations
   UI and press **Launch** once. Let Cloudflare Access, application current
   tenant/membership, device CNG, ACL and Coordinator issue/redeem a new
   launch intent. Do not forge/replay a `profilebridge://` claim. If the
   agent's external GUI environment blocks access, record
   `BLOCKED_BY_ENVIRONMENT` and use permitted ordinary human UI controls,
   not a policy bypass.
5. On the already-owned live Camouhost PID, use
   `scripts/camouhost-agent.py`, built-in inbox
   `%LOCALAPPDATA%\ProfileBridgeAgentTests`, and run the unchanged
   `scripts/b7-playwright-observe.py` as the **first** browser operation.
   Then short same-owner scripts may set/read isolated test markers in
   DOM, cookies, localStorage and IndexedDB. No second browser/Playwright
   writer, CDP port, enable flag or test profile allowlist.
   The agent CLI must run with the SAME Windows security token/owner as
   Camouhost, normally an unelevated terminal started by that user.
   Same account name with elevated Administrator token is not equivalent:
   the CLI rejects incompatible inbox ownership BEFORE publishing a job
   (exit 126, `PLAYWRIGHT_AGENT_SECURITY_CONTEXT_MISMATCH`).
   Do not run Set-Acl/takeown or modify Firefox profile permissions;
   use the supported same-token terminal. A failed/uncertain claim is
   preserved with typed evidence and must not be replayed automatically.
6. Normal GUI Close → ensure durable local Save → remote exact verify and
   *authoritative Coordinator* commit → release writer and lease → fresh
   frontend-issued Open → first read-only Playwright observer. Compare
   exact saved browser test markers and URLs, not only window appearance
   or D1 projection. Confirm 0 unnecessary GET with a real transport trace.
7. Repeat independent positive tests on three distinct profiles; then run
   W06–W08 and S01–S17 on separately recoverable disposable fixtures.
   Exercise controlled offline Close/reconnect, lost PUT/commit reply,
   stale fence, second writer, browser/Bridge crashes, disk full, safe
   N-1/key recovery, independent cold cloud restore, Windows sleep and
   second registered device. Never power-cut or kill the sole original
   working profile for a negative test.
   Hosted packaged Windows regression also exercises three independent
   native Camoufox workspaces across two clean Close/Reopen roundtrips each:
   the original same-owner context performs a read-only first observer,
   then verifies cookie/localStorage/IndexedDB readback on the original
   local test origin. This is **browser persistence only**: no frontend
   claim, Coordinator commit, R2 upload/download, signed release, or
   independent cloud restore is implied by its PASS. The ordinary
   three-profile frontend-authorized cloud composition remains a separate
   mandatory release gate and must not be marked PASS from this test.
8. K2: 256 and 512 MiB real profile fixtures with SQLite/WAL/IndexedDB,
   bounded RAM/disk stream PUT+GET, independent browser reopen and
   retained WORKING LKG; 100 successive Saves and 100 distinct Profiles.
   K3: user-visible true statuses, normal errors, multiple Profiles,
   >=15-minute session and an actual eight-hour run where available.
   Unavailable second device or fault fixture is BLOCKED, not a CI PASS.

## Evidence: post to existing #802, not a new issue

Keep all local raw profile/Playwright/Camouhost data, cookies and traces in
a durable operator-controlled evidence directory outside installation and
rescue roots. Publish only metadata, SHA-256 of named artifacts, source
commit, signed release, installed exact version, UTC, profile opaque ID,
scenario ID, expected/actual state, Coordinator authority receipt **distinct
from D1 projection**, bridge/local state and evidence paths. Do not post
cookies, plaintext profile data, private keys, tokens, screenshots of
private content or entire traces to a public GitHub issue.

Create sequential comments for installation safety, W03–W05, negatives,
performance, and final matrix. Each row:

`ID | PASS/FAIL/BLOCKED/UNKNOWN/NOT_RUN | host/profile | before→after |
actual state and server receipt | local evidence paths and SHA | blocker`

No scenario becomes PASS merely because a PR/CI/Release succeeded.
Only update #266 after publishing a verified #802 checkpoint. Keep #802
OPEN until original-device positive and negative B7/K1–K3 are actually
proven. Stop destructive operations immediately for double writer,
missing sole WORKING data, false cloud Saved, broken CNG identity,
corrupt LKG, unknown fence or an incomplete installer transaction.
