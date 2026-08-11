# Phase 2I Repository-Local Disaster Recovery Runbook

**Scope:** repository-local release-candidate drills only  
**Phase:** 2I  
**Production readiness:** unchanged; `production_ready=false`  
**External/remote recovery evidence:** explicitly deferred to Phase 2J

## 1. Recovery rules

1. Restore authority, not convenience caches.
2. Never promote an unverified generation object or stale coordination snapshot.
3. Never infer success from an unavailable dependency, missing provider response or offline device.
4. Preserve fencing, compare-and-swap and idempotency boundaries through recovery.
5. Keep mailbox content, credentials, decrypted generation bytes and local browser state out of logs, metrics, audit, realtime and support/evidence bundles.
6. A failed or ambiguous recovery remains blocked/remediation-required; it is never converted to empty or successful business state.

## 2. D1 catalog

### Repository-local drill

Run:

```text
python scripts/test-phase2i-d1-backup-restore.py
```

The drill:

- applies the complete ordered D1-compatible migration set to an isolated SQLite catalog;
- seeds synthetic tenant/member/client/profile state through the accepted schema helper;
- verifies `PRAGMA integrity_check` before backup;
- captures a point-in-time physical backup through SQLite's consistent backup API;
- captures a logical SQL export before later live-state mutation;
- mutates the live source after the backup to prove the restore does not silently mirror newer state;
- restores both physical and logical snapshots into separate databases;
- compares schema signature and full logical data digest to the point-in-time source;
- verifies restored database integrity;
- truncates a copied backup and proves corrupted recovery evidence is rejected.

No restored database or SQL dump is committed as evidence. The command emits metadata-only pass/fail output.

### Production/remote boundary

Phase 2I does not claim a real Cloudflare D1 export/import. Phase 2J must execute the provider-supported remote backup/export and restore procedure against approved non-production/production-like resources, record timestamps and object identifiers only, validate schema/data invariants, and keep raw row exports outside repository evidence.

## 3. R2 immutable encrypted generations

Repository-local recovery is governed by the encrypted-generation lifecycle and the immutable R2 boundary:

```text
python scripts/check-r2-generation-objects.py
cargo test --locked -p encrypted-generation-domain
```

Required recovery properties:

- immutable object identity is exact;
- wrong key, wrong generation identity, tampering and truncation are rejected;
- nonce reuse and immutable overwrite conflicts fail closed;
- stale current-generation pointers cannot overwrite newer authority;
- rollback may select only a retained verified rollback generation;
- verification precedes authoritative activation.

Phase 2I does not claim remote R2/key recovery. Phase 2J must separately prove remote object availability/versioning policy, approved key recovery, exact object verification and pointer recovery without exposing plaintext or key material in evidence.

## 4. Durable Object / coordinator authority

Repository-local coordinator recovery uses the existing replayable storage journal, fencing model and repairable D1 projection:

```text
python scripts/check-step5-profile-coordinator.py
python scripts/test-step5-coordinator-projection.py
```

Recovery rules:

- the coordinator remains authoritative for writer ownership/fence state;
- a D1 projection is never promoted into coordination authority;
- stale fences and stale compare-and-swap versions are rejected;
- projection repair is derived from authoritative coordinator state;
- uncertain coordinator ownership blocks mutation rather than allowing a second writer.

Real Durable Object outage/recreation evidence remains Phase 2J External evidence.

## 5. Profile Bridge local state

Repository-local Bridge recovery is exercised by the retained-operator boundary and Profile Bridge tests:

```text
python scripts/check-phase2f-retained-operator.py
python scripts/check-phase2f-retained-operator.py --self-test
cargo test --locked -p profile-bridge --all-targets
```

Recovery rules:

- `cleanup_blocked` and active/retained dirty ownership fail closed;
- dirty local browser state remains recoverable until a new immutable generation is uploaded, exactly verified and committed;
- a failed remote commit does not delete the last recoverable dirty local state;
- a local materialization is never authoritative merely because it exists;
- stale generation/fence context cannot be reopened as current authority.

Physical multi-device restore and real Camoufox recovery remain Phase 2J External evidence.

## 6. Incident decision table

| Failure | Repository-local expected state | Forbidden response |
|---|---|---|
| D1 backup corrupt | restore rejected | partial or best-effort catalog promotion |
| R2 generation corrupt/missing | generation unavailable/quarantined; prior verified generation retained where valid | activate unverified bytes |
| Coordinator uncertain/unavailable | writer path blocked/retry/remediation | create a second writer from D1 projection |
| Mail provider rate-limited | bounded retry pending | tight retry loop or empty-success |
| Mail authentication expired | explicit auth-required state | silent retry forever or empty-success |
| Device offline | durable pending/retry/remediation state | successful completion without device evidence |
| Profile busy | explicit retry/busy state with fence ownership preserved | steal ownership or report success |
| Bridge dirty commit failure | retain recoverable dirty local state | delete local state before verified commit |

## 7. Evidence policy

Repository evidence may contain only metadata-safe scenario names, statuses, counts, durations, versions, opaque IDs where necessary, and digests. It must not contain mailbox body/subject/from/to plaintext, credentials/tokens, decrypted browser profile bytes, raw cookies, local browser storage, generation keys, or raw database exports.

Passing these drills advances Phase 2I repository-owned recovery evidence only. It does not change `production_ready=false` and must not be used as a substitute for the real-provider, real-device and remote-key recovery evidence required by Phase 2J.
