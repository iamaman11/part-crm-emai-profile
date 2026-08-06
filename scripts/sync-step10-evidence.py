#!/usr/bin/env python3
"""Synchronize accepted Repository Step 10 evidence and final roadmap status."""

from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASELINE = "71296404dd5ffb78faf9033cbbb6b6fa395f72cd"
SOURCE_HEAD = "7d5ba8c2a00bac256a9365a40dee7e3c28ef5b56"
SQUASH_MERGE = "3ddde2f48ddf82decf66c933ae5326a4455263e5"
QUALITY_RUN = 31074745842
CERTIFICATION_RUN = 31074745854
ENCRYPTED_RUN = 31074745859
LOCAL_RUN = 31074745880
RUNTIME_RUN = 31074745848


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
        "**Repository Steps 0–9 приняты.**",
        "**Repository Steps 0–10 приняты в ограниченном repository-local scope.**",
        "README accepted-step headline",
    )
    step9 = (
        "- Step 9 добавил exact-pinned XChaCha20-Poly1305/SHA-256 container,\n"
        "  authenticated canonical metadata и final record, immutable generation\n"
        "  lifecycle, strict pointer CAS/rollback/quarantine/orphan planning, DEK-bound\n"
        "  nonce-reuse policy, zeroizing plaintext boundaries и Linux/Windows/WASM gate.\n"
    )
    step10 = step9 + (
        "- Step 10 добавил synthetic required/optional/prohibited certification policy,\n"
        "  deterministic drift matrix, privacy-safe support summary, device-scoped\n"
        "  grant/revoke/regrant journal, exact preverified update evidence binding,\n"
        "  health/rollback state machine и Linux/Windows/WASM Certification Gate.\n"
    )
    text = replace_once(text, step9, step10, "README Step 10 bullet")

    pattern = re.compile(
        r"Accepted Step 9 source head: `[^`]+`\.\n"
        r"Exact-head Quality Gate run: `\d+`\. Encrypted Generation Gate run:\n"
        r"`\d+`\. Local Profile regression run: `\d+`\. Runtime Bundle\n"
        r"regression run: `\d+`\. Squash merge: `[^`]+`\.\n\n"
        r"Следующий этап — \*\*Repository Step 10: Certification And Multi-Device\*\*:.*?"
        r"Машиночитаемый статус: \[`docs/status\.json`\]\(docs/status\.json\)\.",
        re.DOTALL,
    )
    replacement = (
        f"Accepted Step 10 source head: `{SOURCE_HEAD}`.\n"
        f"Exact-head Quality Gate run: `{QUALITY_RUN}`. Certification Gate run:\n"
        f"`{CERTIFICATION_RUN}`. Encrypted Generation regression: `{ENCRYPTED_RUN}`.\n"
        f"Local Profile regression: `{LOCAL_RUN}`. Runtime Bundle regression:\n"
        f"`{RUNTIME_RUN}`. Squash merge: `{SQUASH_MERGE}`.\n\n"
        "Нумерованный repository roadmap завершён на Step 10. Следующий этап —\n"
        "**external production evidence gates**, без выдуманного Step 11: принятие\n"
        "ADR-0001 и ADR-0006, real Camoufox fingerprint certification, второй\n"
        "независимый Windows host, production device-key unwrap, remote R2/D1 atomicity,\n"
        "clean-environment escrow restore, trusted Windows signing и independent security\n"
        "review. Машиночитаемый статус: [`docs/status.json`](docs/status.json)."
    )
    text, count = pattern.subn(replacement, text, count=1)
    if count != 1:
        raise SystemExit(f"README Step 10 evidence block: expected one match, found {count}")

    text = replace_once(
        text,
        "- [Encrypted cloud generations boundary](docs/ENCRYPTED_CLOUD_GENERATIONS.md)\n",
        "- [Encrypted cloud generations boundary](docs/ENCRYPTED_CLOUD_GENERATIONS.md)\n"
        "- [Certification and multi-device boundary](docs/CERTIFICATION_MULTI_DEVICE.md)\n",
        "README Step 10 boundary link",
    )
    text = replace_once(
        text,
        "- [Encrypted cloud generations evidence](docs/evidence/2026-08-06-repository-step-9-encrypted-cloud-generations.md)\n",
        "- [Encrypted cloud generations evidence](docs/evidence/2026-08-06-repository-step-9-encrypted-cloud-generations.md)\n"
        "- [Certification and multi-device evidence](docs/evidence/2026-08-06-repository-step-10-certification-multi-device.md)\n",
        "README Step 10 evidence link",
    )
    old_summary = (
        "Step 9 подтвердил synthetic authenticated encrypted-generation container,\n"
        "immutable lifecycle, DEK-bound nonce protection, zeroizing plaintext memory,\n"
        "strict parsing, pointer/rollback/quarantine/orphan behavior и native/WASM\n"
        "portability. Remote staging, real Camoufox, kernel advisory\n"
        "locking, third-party redistribution, trusted signing, backup/restore, physical\n"
        "multi-device runtime и account recovery пока не считаются выполненными."
    )
    new_summary = (
        "Step 9 подтвердил synthetic authenticated encrypted-generation container,\n"
        "immutable lifecycle, DEK-bound nonce protection, zeroizing plaintext memory,\n"
        "strict parsing, pointer/rollback/quarantine/orphan behavior и native/WASM\n"
        "portability. Step 10 подтвердил synthetic certification matrix, explicit\n"
        "prohibited/incomplete/drift outcomes, immutable device-grant journal, exact\n"
        "preverified release/health identity и fail-closed rollback state machine. Remote\n"
        "staging, real Camoufox, kernel advisory locking, third-party redistribution,\n"
        "trusted signing, backup/restore, physical multi-device runtime и account recovery\n"
        "пока не считаются выполненными."
    )
    text = replace_once(text, old_summary, new_summary, "README Step 10 summary")
    path.write_text(text, encoding="utf-8")


def sync_status() -> None:
    path = ROOT / "docs/status.json"
    data = json.loads(path.read_text(encoding="utf-8"))
    data["as_of"] = "2026-08-06"
    data["repository_step"] = {
        "number": 10,
        "name": "Certification and multi-device contracts",
        "status": "completed",
        "tracking_issue": 32,
        "baseline": BASELINE,
        "technical_evidence_head": SOURCE_HEAD,
        "accepted_source_head": SOURCE_HEAD,
        "technical_quality_gate_run_id": QUALITY_RUN,
        "final_quality_gate_run_id": QUALITY_RUN,
        "specialized_quality_gate_run_id": CERTIFICATION_RUN,
        "encrypted_generation_regression_quality_gate_run_id": ENCRYPTED_RUN,
        "local_profile_regression_quality_gate_run_id": LOCAL_RUN,
        "runtime_bundle_regression_quality_gate_run_id": RUNTIME_RUN,
        "squash_merge": SQUASH_MERGE,
    }
    data["next_repository_step"] = {
        "number": None,
        "name": "External production evidence gates",
        "status": "blocked_external_gate",
    }
    if any(step["number"] == 10 for step in data["accepted_steps"]):
        raise SystemExit("status accepted_steps already contains Step 10")
    data["accepted_steps"].append(
        {
            "number": 10,
            "name": "Certification and multi-device contracts",
            "accepted_source_head": SOURCE_HEAD,
            "quality_gate_run_id": QUALITY_RUN,
            "specialized_quality_gate_run_id": CERTIFICATION_RUN,
            "encrypted_generation_regression_quality_gate_run_id": ENCRYPTED_RUN,
            "local_profile_regression_quality_gate_run_id": LOCAL_RUN,
            "runtime_bundle_regression_quality_gate_run_id": RUNTIME_RUN,
            "squash_merge": SQUASH_MERGE,
        }
    )
    data["implementation"]["certification_multi_device"] = (
        "accepted_step_10_synthetic_policy_and_state_machine_candidate"
    )
    evidence = data["evidence"]
    evidence.update(
        {
            "certification_policy": "accepted_required_optional_prohibited_run_31074745854",
            "certification_required_signal_floor": "accepted_fail_closed_run_31074745854",
            "certification_matrix": "accepted_deterministic_order_independent_run_31074745854",
            "certification_matrix_vector": "accepted_sha256_regression_vector_run_31074745854",
            "certification_distinct_outcomes": "accepted_stable_drifted_incomplete_prohibited_run_31074745854",
            "certification_support_privacy": "accepted_no_raw_values_or_matrix_digest_run_31074745854",
            "certification_negative_fixture": "accepted_raw_signal_output_rejection_run_31074745854",
            "synthetic_device_authorization": "accepted_typed_grant_revoke_regrant_run_31074745854",
            "synthetic_device_grant_journal": "accepted_immutable_event_history_run_31074745854",
            "synthetic_two_device_contract": "accepted_independent_authorization_run_31074745854",
            "preverified_update_evidence_binding": "accepted_exact_release_version_digest_run_31074745854",
            "update_health_identity": "accepted_exact_release_version_digest_run_31074745854",
            "update_rollback_state_machine": "accepted_restored_and_no_previous_outcomes_run_31074745854",
            "certification_windows": "accepted_state_machine_tests_run_31074745854",
            "certification_wasm": "accepted_pure_crate_workers_target_run_31074745854",
            "multi_device": "synthetic_contract_only_physical_multi_device_not_proven",
            "production_fingerprint_certification": "blocked_external_gate",
            "production_device_key_protection": "blocked_external_gate",
            "production_signature_verification": "not_proven",
            "trusted_windows_code_signing": "blocked_external_gate",
            "second_physical_windows_host": "blocked_external_gate",
            "independent_certification_security_review": "not_proven",
        }
    )
    data["decisions"]["adr_0001"] = "proposed_blocks_production_certification"
    data["decisions"]["adr_0006"] = "proposed_blocks_production"
    data["production_ready"] = False
    path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def sync_index() -> None:
    path = ROOT / "docs/TEST_EVIDENCE_INDEX.md"
    text = path.read_text(encoding="utf-8")
    row9 = (
        "| Repository Step 9 / PR #30 | accepted bounded synthetic evidence | exact-pinned XChaCha20-Poly1305/SHA-256 container, authenticated chunk/final records, immutable lifecycle, strict pointer CAS/rollback/quarantine/orphan planning, DEK-bound nonce reuse, zeroizing plaintext boundaries and Linux/Windows/WASM gates | production entropy/key wrapping, remote R2/D1 atomicity, escrow/account-loss restore, independent cryptographic review, physical multi-device or production readiness |\n"
    )
    row10 = row9 + (
        "| Repository Step 10 / PR #33 | accepted bounded synthetic evidence | required/optional/prohibited certification policy, deterministic matrix/vector, privacy-safe support output, typed device grant journal, exact preverified update/health identity, rollback/fail-closed state and Linux/Windows/WASM gates | real fingerprint certification, physical second device, production unwrap, cryptographic signature verification, trusted signing or production readiness |\n"
    )
    text = replace_once(text, row9, row10, "evidence index Step 10 row")

    section = f"""### Repository Step 10 Evidence

- baseline: `{BASELINE}`;
- accepted source head: `{SOURCE_HEAD}`;
- exact-head Quality Gate run: `{QUALITY_RUN}`, conclusion `success`;
- exact-head Certification Gate run: `{CERTIFICATION_RUN}`, conclusion `success`;
- Encrypted Generation regression run: `{ENCRYPTED_RUN}`, conclusion `success`;
- Local Profile regression run: `{LOCAL_RUN}`, conclusion `success`;
- Runtime Bundle regression run: `{RUNTIME_RUN}`, conclusion `success`;
- squash merge: `{SQUASH_MERGE}`;
- policy requires at least one required signal and rejects duplicate/unknown/
  prohibited misuse;
- stable, drifted, incomplete and prohibited outcomes are distinct;
- canonical matrix is input-order independent and matches the committed SHA-256
  regression vector;
- raw observations are non-`Debug`; support output excludes raw values and matrix
  digest;
- two typed synthetic devices have independent grant versions and revoke behavior;
- successful grant/revoke/regrant operations retain immutable event history;
- preverified evidence and health results bind exact release ID/version/digest;
- failed update restores the previous release; first-install failure enters explicit
  `FAILED` state with no active release;
- deliberate raw-signal-output fixture: rejected as required;
- Linux, Windows and Workers WASM jobs passed; Step 7–9 and Windows/Cloudflare
  release regressions remained green;
- detailed report:
  [`evidence/2026-08-06-repository-step-10-certification-multi-device.md`](evidence/2026-08-06-repository-step-10-certification-multi-device.md);
- real browser signals, production keys, physical devices, trusted signatures or
  user data involved: no;
- ADR-0001/ADR-0006 and production readiness remain unaccepted.

"""
    text = replace_once(
        text,
        "## 2. Required Permanent CI Evidence\n",
        section + "## 2. Required Permanent CI Evidence\n",
        "evidence index Step 10 section",
    )
    path.write_text(text, encoding="utf-8")


def main() -> None:
    sync_readme()
    sync_status()
    sync_index()


if __name__ == "__main__":
    main()
