# Camouhost Runtime Bundle Boundary

**Status:** Repository Step 7 implementation specification  
**Production readiness:** false

## Responsibility

The runtime bundle boundary packages a deterministic development Camouhost
runtime for validation by the Windows Profile Bridge. It is content-addressed,
versioned and restricted to explicitly marked synthetic inputs and destinations.
It does not redistribute or execute a real Camoufox browser binary.

The Bridge validates the typed manifest, canonical inventory and calculated
digest before invoking any process-control port. A process cannot be launched
from an unapproved bundle.

## Manifest Contract

Manifest schema version `1` fixes:

- runtime version `0.1.0`;
- Python compatibility `3.12`;
- IPC version `1`;
- platform `windows-x86_64`;
- entrypoint `camouhost/main.py`;
- canonical file inventory and SHA-256 inventory digest.

Unknown fields, missing fields, version drift, unsupported platform, entrypoint
change and digest mismatch fail before launch.

## Safe Paths And Inventory

Every bundle path is a canonical relative POSIX path. Validation rejects:

- absolute or drive-qualified paths;
- backslashes, empty segments and duplicate separators;
- `.` and `..` traversal;
- trailing dot or space;
- characters outside bounded ASCII alphanumeric, dot, underscore and hyphen;
- Windows-reserved names such as `CON`, `NUL`, `COM1` and `LPT1`;
- duplicate and case-colliding paths;
- symbolic links, directories and encrypted archive entries.

Extraction validates the complete archive before writing and requires an empty,
explicitly marked synthetic destination. Each target parent must remain below the
resolved destination.

## Deterministic Bundle

The development bundle uses canonical JSON, sorted inventory paths, fixed ZIP
timestamps, stored entries and stable Unix mode metadata. The same synthetic
source bytes produce the same bundle bytes and inventory digest. Any source,
manifest or payload byte change is detected.

Sources must contain the exact `.synthetic-runtime-root` marker. Destinations
must contain the exact `.synthetic-runtime-destination` marker. Network package
resolution is forbidden; all inputs are committed or generated synthetic
fixtures.

## Bridge Approval And Rollback

`ApprovedRuntimeBundle` exists only after:

1. the calculated inventory SHA-256 equals the manifest digest;
2. the manifest entrypoint exists in the validated inventory.

The runtime orchestrator accepts only an approved bundle. It then:

1. invokes process spawn for the typed session;
2. negotiates Camouhost IPC version `1`;
3. requires `Ready` with the exact session ID;
4. requests graceful process close;
5. requires `Closed(clean=true)` with the exact session ID.

If hello or launch negotiation fails after spawn, the Bridge invokes forced
termination. IPC failure or rollback failure is never converted into a clean
close.

## Synthetic Profile Lifecycle

The fake Camouhost subprocess requires an explicitly marked, otherwise empty
synthetic profile directory. On launch it atomically writes
`.runtime-active.json` containing the exact session ID. Only a valid graceful
close writes `.runtime-closed-clean.json` and removes the active marker.

Session mismatch, malformed IPC or premature EOF leaves active evidence and no
clean marker. This proves that abnormal termination cannot silently become a
clean profile close. No browser runtime lock file is deleted.

## Legacy Corpus Prohibition

Step 7 runtime sources, tools and tests must not reference, read, write, copy,
launch or mutate the legacy profile corpus. The permanent policy scans the Step 7
boundary, and a deliberate fixture containing a legacy-corpus reference must be
rejected by CI.

## Permanent Evidence

The dedicated Runtime Bundle Gate runs on Linux and Windows and proves:

- policy enforcement and deliberate negative fixture rejection;
- deterministic build, verification and safe extraction;
- tamper, traversal, symlink and case-collision rejection;
- fake Camouhost version negotiation and exact-session lifecycle;
- synthetic active versus clean profile evidence;
- provider-free runtime bundle domain compilation for WASM;
- Windows domain tests and continued non-empty Profile Bridge release build.

The normal repository Quality Gate remains mandatory and preserves all accepted
Cloudflare, D1, ACL, coordinator and Bridge regression evidence.

Real Camoufox execution, third-party redistribution rights, production Python
dependency resolution, trusted signing, installer/update channels, physical
Windows host lifecycle, encrypted generations and production readiness remain
explicitly unproven.
