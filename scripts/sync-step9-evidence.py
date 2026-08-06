#!/usr/bin/env python3
"""Synchronize accepted Repository Step 9 evidence, then remove this script."""

from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

BASELINE = "e596fbe5692aa5b020700e7462c608dd23bacc15"
SOURCE_HEAD = "73685241a6d70cf6d8ec80210d94b66cf37b1b45"
SQUASH_MERGE = "bc5286e3fea767acf955fb2622dab6221ecf1c3b"
QUALITY_RUN = 31072625808
ENCRYPTED_RUN = 31072625852
LOCAL_RUN = 31072625849
RUNTIME_RUN = 31072625892


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one match, found {count}")
    return text.replace(old, new, 1)


def sync_readme() -> None:
    path = ROOT / "README.md"
    text = path.read_text(encoding="utf-8")
    text = replace_once(
        text,
        "**Repository Steps 0–8 приняты.**",
        "**Repository Steps 0–9 приняты.**",
        "README accepted-step headline",
    )
    step8 = (
        "- Step 8 добавил marked opaque materialization paths, atomic Bridge-owned\n"
        "  lock-file protocol, deterministic regular-file inventory, clone-only recovery,\n"
        "  explicit dirty/recovery lifecycle, forgotten-window and safe quota policies,\n"
        "  metadata-only support evidence и отдельный Linux/Windows Local Profile Gate.\n"
    )
    step9 = step8 + (
        "- Step 9 добавил exact-pinned XChaCha20-Poly1305/SHA-256 container,\n"
        "  authenticated canonical metadata и final record, immutable generation\n"
        "  lifecycle, strict pointer CAS/rollback/quarantine/orphan planning, DEK-bound\n"
        "  nonce-reuse policy, zeroizing plaintext boundaries и Linux/Windows/WASM gate.\n"
    )
    text = replace_once(text, step8, step9, "README Step 9 bullet")

    pattern = re.compile(
        r"Accepted Step 8 source head: `[^`]+`\.\n"
        r"Exact-head Quality Gate run: `\d+`\. Local Profile Gate run:\n"
        r"`\d+`\. Runtime Bundle regression run: `\d+`\. Squash merge:\n"
        r"`[^`]+`\.\n\n"
        r"Следующий этап — \*\*Repository Step 9: Encrypted Cloud Generations\*\*:.*?"
        r"Машиночитаемый статус: \[`docs/status\.json`\]\(docs/status\.json\)\.",
        re.DOTALL,
    )
    replacement = (
        f"Accepted Step 9 source head: `{SOURCE_HEAD}`.\n"
        f"Exact-head Quality Gate run: `{QUALITY_RUN}`. Encrypted Generation Gate run:\n"
        f"`{ENCRYPTED_RUN}`. Local Profile regression run: `{LOCAL_RUN}`. Runtime Bundle\n"
        f"regression run: `{RUNTIME_RUN}`. Squash merge: `{SQUASH_MERGE}`.\n\n"
        "Следующий этап — **Repository Step 10: Certification And Multi-Device**:\n"
        "bounded certification policy, drift/repeatability evidence, device-scoped\n"
        "unwrap/revoke contracts и signed-update rollback boundary. Accepted ADR-0001,\n"
        "второй независимый Windows host и trusted signing остаются внешними gates и не\n"
        "считаются доказанными. ADR-0006 остаётся proposed; production key management,\n"
        "remote R2/D1 atomicity и clean-environment escrow restore ещё не доказаны.\n"
        "Машиночитаемый статус: [`docs/status.json`](docs/status.json)."
    )
    text, count = pattern.subn(replacement, text, count=1)
    if count != 1:
        raise SystemExit(f"README accepted evidence block: expected one match, found {count}")

    text = replace_once(
        text,
        "- [Local profile lifecycle boundary](docs/LOCAL_PROFILE_LIFECYCLE.md)\n",
        "- [Local profile lifecycle boundary](docs/LOCAL_PROFILE_LIFECYCLE.md)\n"
        "- [Encrypted cloud generations boundary](docs/ENCRYPTED_CLOUD_GENERATIONS.md)\n",
        "README Step 9 boundary link",
    )
    text = replace_once(
        text,
        "- [Local profile lifecycle evidence](docs/evidence/2026-08-06-repository-step-8-local-profile-lifecycle.md)\n",
        "- [Local profile lifecycle evidence](docs/evidence/2026-08-06-repository-step-8-local-profile-lifecycle.md)\n"
        "- [Encrypted cloud generations evidence](docs/evidence/2026-08-06-repository-step-9-encrypted-cloud-generations.md)\n",
        "README Step 9 evidence link",
    )
    old_summary = (
        "Step 8 подтвердил safe marked local\n"
        "materialization, atomic Bridge lock-file ownership, deterministic inventory,\n"
        "clone-only integrity evidence, dirty/recovery preservation, quota exclusion and\n"
        "metadata-only support output. Remote staging, real Camoufox, kernel advisory\n"
        "locking, third-party redistribution, trusted signing, backup/restore, physical\n"
        "multi-device runtime и account recovery пока не считаются выполненными."
    )
    new_summary = old_summary.replace(
        ". Remote staging",
        ". Step 9 подтвердил synthetic authenticated encrypted-generation container,\n"
        "immutable lifecycle, DEK-bound nonce protection, zeroizing plaintext memory,\n"
        "strict parsing, pointer/rollback/quarantine/orphan behavior и native/WASM\n"
        "portability. Remote staging",
    )
    text = replace_once(text, old_summary, new_summary, "README Step 9 summary")
    path.write_text(text, encoding="utf-8")


def sync_status() -> None:
    path = ROOT / "docs/status.json"
    data = json.loads(path.read_text(encoding="utf-8"))
    data["as_of"] = "2026-08-06"
    data["repository_step"] = {
        "number": 9,
        "name": "Encrypted cloud generations",
        "status": "completed",
        "tracking_issue": 29,
        "baseline": BASELINE,
        "technical_evidence_head": SOURCE_HEAD,
        "accepted_source_head": SOURCE_HEAD,
        "technical_quality_gate_run_id": QUALITY_RUN,
        "final_quality_gate_run_id": QUALITY_RUN,
        "specialized_quality_gate_run_id": ENCRYPTED_RUN,
        "local_profile_regression_quality_gate_run_id": LOCAL_RUN,
        "runtime_bundle_regression_quality_gate_run_id": RUNTIME_RUN,
        "squash_merge": SQUASH_MERGE,
    }
    data["next_repository_step"] = {
        "number": 10,
        "name": "Certification and multi-device",
        "status": "planned",
    }
    accepted_steps = data["accepted_steps"]
    if any(step["number"] == 9 for step in accepted_steps):
        raise SystemExit("status accepted_steps already contains Step 9")
    accepted_steps.append(
        {
            "number": 9,
            "name": "Encrypted cloud generations",
            "accepted_source_head": SOURCE_HEAD,
            "quality_gate_run_id": QUALITY_RUN,
            "specialized_quality_gate_run_id": ENCRYPTED_RUN,
            "local_profile_regression_quality_gate_run_id": LOCAL_RUN,
            "runtime_bundle_regression_quality_gate_run_id": RUNTIME_RUN,
            "squash_merge": SQUASH_MERGE,
        }
    )
    implementation = data["implementation"]
    implementation["encrypted_cloud_generations"] = (
        "accepted_step_9_synthetic_cryptographic_review_candidate"
    )
    evidence = data["evidence"]
    evidence.update(
        {
            "encrypted_cloud_generations": (
                "accepted_synthetic_container_lifecycle_run_31072625852"
            ),
            "encrypted_generation_exact_crypto_pins": (
                "accepted_rust_1_97_1_run_31072625808"
            ),
            "encrypted_generation_container": (
                "accepted_xchacha20poly1305_chunk_and_final_record_run_31072625852"
            ),
            "encrypted_generation_test_vector": (
                "accepted_deterministic_sha256_vector_run_31072625852"
            ),
            "encrypted_generation_tamper_detection": (
                "accepted_metadata_chunk_final_truncation_reorder_run_31072625852"
            ),
            "encrypted_generation_strict_parser": (
                "accepted_magic_version_oversize_trailing_rejection_run_31072625852"
            ),
            "encrypted_generation_nonce_reuse": (
                "accepted_actual_dek_domain_and_key_id_alias_run_31072625852"
            ),
            "encrypted_generation_sensitive_memory": (
                "accepted_zeroizing_non_debug_results_run_31072625852"
            ),
            "encrypted_generation_immutable_objects": (
                "accepted_idempotent_and_conflict_behavior_run_31072625852"
            ),
            "encrypted_generation_pointer_cas": (
                "accepted_commit_and_rollback_versioning_run_31072625852"
            ),
            "encrypted_generation_quarantine": (
                "accepted_corruption_quarantine_wrong_key_dos_protection_run_31072625852"
            ),
            "encrypted_generation_orphan_planning": (
                "accepted_current_and_rollback_protection_run_31072625852"
            ),
            "encrypted_generation_support_summary": (
                "accepted_metadata_only_run_31072625852"
            ),
            "encrypted_generation_windows": (
                "accepted_lifecycle_tests_run_31072625852"
            ),
            "encrypted_generation_wasm": (
                "accepted_pure_crate_workers_target_run_31072625852"
            ),
            "production_encrypted_generation_remote_atomicity": "not_proven",
            "production_entropy_and_key_wrapping": "not_proven",
            "production_clean_environment_key_restore": "blocked_external_gate",
            "independent_cryptographic_review": "not_proven",
        }
    )
    data["decisions"]["adr_0006"] = "proposed_blocks_production"
    data["production_ready"] = False
    path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def sync_index() -> None:
    path = ROOT / "docs/TEST_EVIDENCE_INDEX.md"
    text = path.read_text(encoding="utf-8")
    row8 = (
        "| Repository Step 8 / PR #27 | accepted | marked opaque local materialization, atomic Bridge lock-file protocol, deterministic inventory, clone-only recovery, explicit dirty/recovery states, safe quota and metadata-only support evidence | kernel advisory locks, real browser/database recovery, real legacy profiles, encrypted cloud generations, trusted signing, multi-device or production readiness |\n"
    )
    row9 = row8 + (
        "| Repository Step 9 / PR #30 | accepted bounded synthetic evidence | exact-pinned XChaCha20-Poly1305/SHA-256 container, authenticated chunk/final records, immutable lifecycle, strict pointer CAS/rollback/quarantine/orphan planning, DEK-bound nonce reuse, zeroizing plaintext boundaries and Linux/Windows/WASM gates | production entropy/key wrapping, remote R2/D1 atomicity, escrow/account-loss restore, independent cryptographic review, physical multi-device or production readiness |\n"
    )
    text = replace_once(text, row8, row9, "evidence index Step 9 table row")

    section = f"""### Repository Step 9 Evidence

- baseline: `{BASELINE}`;
- accepted source head: `{SOURCE_HEAD}`;
- exact-head Quality Gate run: `{QUALITY_RUN}`, conclusion `success`;
- exact-head Encrypted Generation Gate run: `{ENCRYPTED_RUN}`, conclusion `success`;
- exact-head Local Profile regression run: `{LOCAL_RUN}`, conclusion `success`;
- exact-head Runtime Bundle regression run: `{RUNTIME_RUN}`, conclusion `success`;
- squash merge: `{SQUASH_MERGE}`;
- exact RustCrypto XChaCha20-Poly1305, SHA-256 and zeroization pins: passed;
- canonical authenticated metadata, ordered chunks and mandatory final record: passed;
- deterministic SHA-256 container regression vector: passed;
- metadata/chunk/final tamper, truncation, reorder and identity mismatch: rejected;
- invalid magic/version, oversized metadata and trailing bytes: rejected;
- same DEK and nonce prefix across different key IDs: rejected as nonce reuse;
- DEK and nonce-domain memory is non-printable and zeroized on drop;
- plaintext-bearing results are non-`Debug` and use `Zeroizing` buffers;
- restore grows plaintext only after authenticated records;
- immutable conflict, stale pointer, invalid rollback and corrupt promotion: rejected;
- wrong-key restore cannot quarantine an unchanged digest-matching object;
- orphan planning protects current and rollback generations;
- deliberate sensitive-output fixture: rejected as required;
- Linux, Windows and Workers WASM dedicated jobs passed; Profile Bridge, Runtime
  Bundle and Cloudflare Worker release regressions remained green;
- detailed report:
  [`evidence/2026-08-06-repository-step-9-encrypted-cloud-generations.md`](evidence/2026-08-06-repository-step-9-encrypted-cloud-generations.md);
- production credentials, remote resources, real profiles, production keys or
  physical multi-device evidence involved: no;
- ADR-0006 and production readiness remain unaccepted.

"""
    text = replace_once(
        text,
        "## 2. Required Permanent CI Evidence\n",
        section + "## 2. Required Permanent CI Evidence\n",
        "evidence index Step 9 section",
    )
    path.write_text(text, encoding="utf-8")


def main() -> None:
    sync_readme()
    sync_status()
    sync_index()


if __name__ == "__main__":
    main()
