# Repository Step 7 — Camouhost Runtime Bundle Evidence

**Дата:** 2026-08-06  
**Статус:** accepted  
**Baseline:** `6f76109d48272109ea305c6f8690cc4c6540542f`  
**Accepted source head:** `936d3c9529b897daac2ea5d13ba01f7babf07b8a`  
**Pull request:** #24  
**Exact-head Quality Gate run:** `31060683502`  
**Exact-head Runtime Bundle Gate run:** `31060683898`  
**Squash merge:** `9d01ccb34598a8aeb9406570b623582d710c88e7`

## 1. Реализованный Runtime Bundle Boundary

Step 7 introduces a dependency-free Rust runtime-bundle domain, deterministic
standard-library Python packaging and verification tooling, a committed fake
Camouhost subprocess, and typed Windows Profile Bridge approval before process
spawn.

The accepted boundary is explicitly synthetic. It does not redistribute or
execute a real Camoufox browser, resolve Python dependencies from the network or
read the legacy profile corpus.

## 2. Manifest, Inventory And Content Addressing

Manifest schema version `1` fixes the accepted development runtime contract:

- runtime version `0.1.0`;
- Python compatibility `3.12`;
- IPC version `1`;
- platform `windows-x86_64`;
- entrypoint `camouhost/main.py`;
- canonical inventory and SHA-256 inventory digest.

The pure domain validates bounded canonical relative paths, lowercase SHA-256
shape, manifest versions, entrypoint presence and case-insensitive inventory
uniqueness. The packaging tool independently computes per-file and inventory
digests from actual bytes.

Unknown or missing manifest fields, version drift, platform mismatch,
entrypoint change, inventory mismatch and payload-byte tampering fail before
runtime launch.

## 3. Safe Path And Extraction Protocol

Bundle paths reject:

- absolute and drive-qualified paths;
- backslashes, duplicate separators and trailing slash;
- `.` and `..` traversal;
- invalid or overlong segments;
- trailing dot or space;
- Windows-reserved names including `CON`, `NUL`, `COM1` and `LPT1`;
- duplicate and case-colliding names;
- symbolic links, directories and encrypted archive entries.

The complete archive is verified before extraction. Extraction requires an
otherwise empty destination containing the exact synthetic-destination marker,
and every resolved parent must remain beneath that destination.

## 4. Deterministic Development Bundle

The accepted tool uses canonical JSON, canonically sorted inventory entries,
fixed ZIP timestamps, stored entries and stable mode metadata. Rebuilding the
same explicitly marked synthetic source produces byte-identical bundle output
and the same inventory digest. Changing one source byte changes both the digest
and bundle bytes.

Source and destination markers prevent accidental use of arbitrary directories.
The Step 7 policy forbids network package installation and direct references to
the legacy profile corpus.

## 5. Bridge Approval And Runtime Rollback

The Profile Bridge composes the new domain through `ApprovedRuntimeBundle`.
Approval requires both the calculated inventory digest and the entrypoint check
to pass before `ProcessControlPort::spawn` can be called.

After spawn the runtime orchestrator requires:

1. IPC version `1` hello/ack;
2. launch/ready with the exact typed session ID;
3. graceful process-close request;
4. closed-clean response with the same session ID.

If hello or launch negotiation fails after spawn, forced termination is invoked.
Protocol failure or rollback failure is never represented as a clean close.

## 6. Synthetic Profile Lifecycle

The committed fake Camouhost requires an explicitly marked and otherwise empty
synthetic profile directory. Launch atomically writes `.runtime-active.json`
with the exact session ID. Only an exact graceful close writes
`.runtime-closed-clean.json` and removes the active marker.

Session mismatch and premature EOF retain active evidence and do not create a
clean marker. This proves the bounded invariant that abnormal runtime completion
cannot silently become a clean profile close.

## 7. Permanent CI Result

Both permanent workflows succeeded on exact source head
`936d3c9529b897daac2ea5d13ba01f7babf07b8a`.

### Quality Gate `31060683502`

- architecture, D1, identity/ACL, profile coordinator and Windows Bridge policy
  regressions remained green;
- rustfmt, Clippy with warnings denied and all native workspace tests passed;
- Windows native tests and non-empty Profile Bridge release artifact passed;
- D1 migrations applied and replayed successfully;
- Cloudflare Worker WASM check and release artifact verification passed;
- delivery-status and tracked-tree secret checks passed.

### Runtime Bundle Gate `31060683898`

#### Linux

- all Step 7 Python sources compiled;
- runtime bundle policy passed;
- deliberate legacy-corpus reference fixture was rejected;
- deterministic build, verify, tamper, traversal, symlink, case-collision and safe
  extraction tests passed;
- fake Camouhost IPC and synthetic profile lifecycle passed;
- provider-free runtime-bundle domain compiled for WASM.

#### Windows

- runtime-bundle domain tests passed;
- fake Camouhost subprocess and synthetic profile lifecycle passed;
- Profile Bridge release executable rebuilt and remained non-empty.

## 8. Доказанные Свойства

The accepted repository evidence proves:

- the same synthetic input produces byte-identical bundles;
- any accepted source-byte, manifest, inventory or payload change is detected;
- unsafe, reserved, traversal, symlink and case-colliding paths fail closed;
- extraction cannot target an unmarked or non-empty destination;
- bundle approval precedes process spawn;
- failed runtime negotiation triggers forced-termination rollback;
- fake Camouhost preserves exact IPC version and session identity;
- clean evidence exists only after exact graceful close;
- premature EOF and session mismatch retain active evidence;
- Step 7 source cannot reference the legacy profile corpus or network dependency
  installation;
- Linux, Windows, WASM, Profile Bridge and Cloudflare Worker regression artifacts
  all build on the exact accepted head.

## 9. Defects Found And Corrected

- the first static spawn-order marker matched pre-rustfmt formatting; the policy
  was changed to a formatting-independent marker;
- the initial fake Camouhost proved IPC only; explicit synthetic active/clean
  profile evidence was added;
- the Bridge initially exposed runtime fakes without a typed approval object;
  `ApprovedRuntimeBundle` now gates process spawn and rollback;
- Python dynamic module loading was registered in `sys.modules` for deterministic
  dataclass execution;
- temporary formatting and lockfile workflows were removed and permanent source
  hygiene rejects their return.

## 10. Ограничения И Внешние Gates

This evidence does not prove:

- real Camoufox or third-party browser redistribution and licensing;
- real browser launch, rendering, fingerprint behavior or profile compatibility;
- production Python dependency resolution, SBOM or vulnerability review;
- trusted Windows signing, installer, update or rollback channel;
- physical Windows Job Object behavior or process-tree termination;
- execution, repair, migration or mutation of any legacy user profile;
- encrypted R2 generations, production device keys or physical multi-device
  operation;
- production credentials, privacy readiness or production readiness.

No production credential, remote resource, real user profile, mailbox content or
personal data was used. All runtime files, session IDs and profile directories
were synthetic. `production_ready` remains `false`.
