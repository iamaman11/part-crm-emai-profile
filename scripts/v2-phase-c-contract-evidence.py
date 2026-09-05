#!/usr/bin/env python3
"""Build the exact secret-free Phase C evidence consumed by native D1 contract-transition.

The adapter is observation-only. It proves the two PAS-2 CONTRACT preconditions from exact
checked-out Product Runtime source, binds them to one immutable deployed Release Set, and
requires two stable single-version/100%-traffic Worker deployment observations.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import tempfile
from pathlib import Path
from typing import Any

SOURCE_SHA_RE = re.compile(r"^[0-9a-f]{40}$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
RELEASE_SET_RE = re.compile(r"^release-set-v(?:2|3)-sha256-[0-9a-f]{64}$")
UUID_RE = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$", re.I)
PREDECESSOR = "0031_device_binding_governance.sql"
CONTRACT = "0032_pas2_payload_fingerprint_contract.sql"
MIN_QUIESCENCE_SECONDS = 30
MAX_QUIESCENCE_SECONDS = 300


class PhaseCError(ValueError):
    pass


def fail(message: str) -> None:
    raise PhaseCError(message)


def strict_pairs(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    output: dict[str, Any] = {}
    for key, value in pairs:
        if key in output:
            fail(f"duplicate JSON key: {key}")
        output[key] = value
    return output


def load(path: Path, label: str) -> Any:
    if path.is_symlink() or not path.is_file():
        fail(f"{label} must be a regular file: {path}")
    try:
        return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=strict_pairs)
    except json.JSONDecodeError as error:
        raise PhaseCError(f"{label} is invalid JSON: {error}") from error


def object_value(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        fail(f"{label} must be one JSON object")
    return value


def production_rust(text: str) -> str:
    return text.split("#[cfg(test)]", 1)[0]


def source_preconditions(root: Path) -> tuple[dict[str, bool], dict[str, Any]]:
    worker_root = root / "apps/control-plane-worker/src"
    command_evidence = worker_root / "command_evidence.rs"
    if worker_root.is_symlink() or not worker_root.is_dir():
        fail("control-plane Worker source root is missing/not regular")
    if command_evidence.is_symlink() or not command_evidence.is_file():
        fail("command_evidence.rs is missing/not regular")
    command_text = production_rust(command_evidence.read_text(encoding="utf-8"))
    required_server_owned = (
        'const FINGERPRINT_DOMAIN: &[u8] = b"part-crm:payload-fingerprint:v1"',
        "The fingerprint is server-owned",
        "fingerprint_typed_request(request, payload)?",
        "serde_json::to_vec(payload)",
        "append_field(&mut material, method.as_bytes())?",
        "append_field(&mut material, path.as_bytes())?",
        "append_field(&mut material, &payload_bytes)?",
    )
    missing = [marker for marker in required_server_owned if marker not in command_text]
    if missing:
        fail(f"server-owned payload fingerprint source contract drifted: missing={missing}")

    runtime_files = sorted(worker_root.rglob("*.rs"))
    if not runtime_files:
        fail("control-plane Worker Rust source inventory is empty")
    request_digest_owners: list[str] = []
    digest = hashlib.sha256()
    for path in runtime_files:
        if path.is_symlink() or not path.is_file():
            fail(f"runtime source is not a regular file: {path}")
        relative = path.relative_to(root).as_posix()
        production = production_rust(path.read_text(encoding="utf-8"))
        if "request_digest" in production or "requestDigest" in production:
            request_digest_owners.append(relative)
        digest.update(relative.encode("utf-8"))
        digest.update(b"\0")
        digest.update(production.encode("utf-8"))
        digest.update(b"\0")
    if request_digest_owners:
        fail(f"request_digest production readers/writers remain: {request_digest_owners}")
    return (
        {
            "request_digest_readers_writers_retired": True,
            "server_owned_payload_fingerprint_active": True,
        },
        {
            "runtime_source_sha256": digest.hexdigest(),
            "runtime_source_file_count": len(runtime_files),
            "command_evidence_path": command_evidence.relative_to(root).as_posix(),
        },
    )


def deployment_identity(path: Path, expected_release_set: str) -> tuple[str, str]:
    value = object_value(load(path, "deployment identity"), "deployment identity")
    if value.get("schema_version") != 2 or value.get("kind") != "DEPLOYMENT_IDENTITY_OBSERVATION":
        fail("deployment identity schema/kind drifted")
    release_set_id = value.get("release_set_id")
    profile_id = value.get("capability_profile_id")
    if release_set_id != expected_release_set:
        fail("deployed Release Set identity does not equal the exact expected Release Set")
    if not isinstance(profile_id, str) or not profile_id:
        fail("deployment capability profile identity is missing")
    return release_set_id, profile_id


def deployment_state(path: Path) -> tuple[str, str, float]:
    value = object_value(load(path, "Worker deployment"), "Worker deployment")
    deployment_id = value.get("id")
    if not isinstance(deployment_id, str) or not UUID_RE.fullmatch(deployment_id):
        fail("Worker deployment id is missing/malformed")
    versions = value.get("versions")
    if not isinstance(versions, list) or len(versions) != 1 or not isinstance(versions[0], dict):
        fail("Phase C requires exactly one active Worker version")
    version_id = versions[0].get("version_id")
    percentage = versions[0].get("percentage")
    if not isinstance(version_id, str) or not UUID_RE.fullmatch(version_id):
        fail("sole active Worker version id is missing/malformed")
    if isinstance(percentage, bool) or not isinstance(percentage, (int, float)) or float(percentage) != 100.0:
        fail("sole active Worker version must serve exactly 100 percent traffic")
    return deployment_id, version_id, float(percentage)


def ledger_predecessor(path: Path) -> None:
    value = load(path, "D1 ledger")
    if not isinstance(value, list) or len(value) != 1 or not isinstance(value[0], dict):
        fail("D1 ledger must be one Wrangler result envelope")
    envelope = value[0]
    if envelope.get("success") is not True:
        fail("D1 ledger observation is unsuccessful")
    rows = envelope.get("results")
    if not isinstance(rows, list) or not rows:
        fail("D1 ledger rows are missing")
    names: list[str] = []
    for row in rows:
        if not isinstance(row, dict):
            fail("D1 ledger row is malformed")
        name = row.get("name")
        if not isinstance(name, str):
            fail("D1 ledger migration name is malformed")
        names.append(name)
    if names[-1] != PREDECESSOR or CONTRACT in names:
        fail("Phase C evidence requires exact pre-CONTRACT remote state ending at 0031")


def bindings(path: Path) -> dict[str, str]:
    value = object_value(load(path, "native contract evidence bindings"), "native contract evidence bindings")
    if value.get("schema_version") != 1 or value.get("kind") != "D1_CONTRACT_EVIDENCE_BINDINGS":
        fail("native contract evidence bindings identity/version drifted")
    if value.get("mutation_executed") is not False:
        fail("native contract evidence bindings unexpectedly report mutation")
    result: dict[str, str] = {}
    for field in ("ledger_sha256", "release_manifest_sha256", "repository_identity_sha256"):
        digest = value.get(field)
        if not isinstance(digest, str) or not SHA256_RE.fullmatch(digest):
            fail(f"native contract evidence binding {field} is malformed")
        result[field] = digest
    return result


def build(args: argparse.Namespace) -> tuple[dict[str, Any], dict[str, Any]]:
    if not SOURCE_SHA_RE.fullmatch(args.expected_source_sha):
        fail("expected source SHA is malformed")
    if not RELEASE_SET_RE.fullmatch(args.expected_release_set_id):
        fail("expected Release Set identity is malformed/unsupported")
    if args.first_observed_at_unix_seconds < 0 or args.observed_at_unix_seconds < 0:
        fail("observation timestamps must be non-negative")
    interval = args.observed_at_unix_seconds - args.first_observed_at_unix_seconds
    if interval < MIN_QUIESCENCE_SECONDS or interval > MAX_QUIESCENCE_SECONDS:
        fail(
            f"Worker quiescence interval must be {MIN_QUIESCENCE_SECONDS}..{MAX_QUIESCENCE_SECONDS}s; observed={interval}"
        )

    preconditions, source_details = source_preconditions(args.root)
    before_id, before_version, before_percentage = deployment_state(args.deployment_before)
    after_id, after_version, after_percentage = deployment_state(args.deployment_after)
    before_release, before_profile = deployment_identity(args.identity_before, args.expected_release_set_id)
    after_release, after_profile = deployment_identity(args.identity_after, args.expected_release_set_id)
    if (before_id, before_version, before_percentage) != (after_id, after_version, after_percentage):
        fail("Worker deployment changed during the Phase C quiescence window")
    if (before_release, before_profile) != (after_release, after_profile):
        fail("Worker Release Set/profile identity changed during the Phase C quiescence window")
    ledger_predecessor(args.ledger_json)
    exact_bindings = bindings(args.bindings_json)

    evidence = {
        "schema_version": 1,
        "kind": "D1_CONTRACT_TRANSITION_EVIDENCE",
        "environment": "staging",
        "component": "catalog",
        "predecessor_revision": PREDECESSOR,
        "contract_revision": CONTRACT,
        "recovery_strategy": "FAIL_FORWARD_ONLY",
        "release_manifest_sha256": exact_bindings["release_manifest_sha256"],
        "repository_identity_sha256": exact_bindings["repository_identity_sha256"],
        "ledger_sha256": exact_bindings["ledger_sha256"],
        "observed_at_unix_seconds": args.observed_at_unix_seconds,
        "preconditions": preconditions,
        "deployment": {
            "release_set_id": args.expected_release_set_id,
            "source_sha": args.expected_source_sha,
            "single_version": True,
            "quiescent": True,
            "traffic_percent": 100.0,
            "active_version_ids": [after_version],
        },
    }
    details = {
        "schema_version": 1,
        "kind": "V2_PHASE_C_MECHANICAL_PROOF_DETAILS",
        "source_sha": args.expected_source_sha,
        "release_set_id": args.expected_release_set_id,
        "deployment_id": after_id,
        "capability_profile_id": after_profile,
        "quiescence_interval_seconds": interval,
        "source_proof": source_details,
        "preconditions": preconditions,
        "provider_mutation": False,
        "production_mutation": False,
    }
    return evidence, details


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="v2-phase-c-") as directory:
        root = Path(directory)
        source = root / "apps/control-plane-worker/src"
        source.mkdir(parents=True)
        (source / "command_evidence.rs").write_text(
            'const FINGERPRINT_DOMAIN: &[u8] = b"part-crm:payload-fingerprint:v1";\n'
            '/// The fingerprint is server-owned\n'
            'fn f(request: R, payload: P) { let _ = fingerprint_typed_request(request, payload)?; '
            'let payload_bytes = serde_json::to_vec(payload); append_field(&mut material, method.as_bytes())?; '
            'append_field(&mut material, path.as_bytes())?; append_field(&mut material, &payload_bytes)?; }\n',
            encoding="utf-8",
        )
        release_set = "release-set-v3-sha256-" + "a" * 64
        deployment_id = "123e4567-e89b-42d3-a456-426614174000"
        version_id = "123e4567-e89b-42d3-a456-426614174001"
        deployment = {"id": deployment_id, "versions": [{"version_id": version_id, "percentage": 100}]}
        identity = {"schema_version": 2, "kind": "DEPLOYMENT_IDENTITY_OBSERVATION", "release_set_id": release_set, "capability_profile_id": "rehearsal-core-v2"}
        ledger = [{"success": True, "results": [{"id": 31, "name": PREDECESSOR}]}]
        binding = {"schema_version": 1, "kind": "D1_CONTRACT_EVIDENCE_BINDINGS", "ledger_sha256": "b" * 64, "release_manifest_sha256": "c" * 64, "repository_identity_sha256": "d" * 64, "mutation_executed": False}
        for name, value in (("before.json", deployment), ("after.json", deployment), ("identity-before.json", identity), ("identity-after.json", identity), ("ledger.json", ledger), ("bindings.json", binding)):
            (root / name).write_text(json.dumps(value), encoding="utf-8")
        args = argparse.Namespace(
            root=root,
            expected_source_sha="e" * 40,
            expected_release_set_id=release_set,
            first_observed_at_unix_seconds=100,
            observed_at_unix_seconds=130,
            deployment_before=root / "before.json",
            deployment_after=root / "after.json",
            identity_before=root / "identity-before.json",
            identity_after=root / "identity-after.json",
            ledger_json=root / "ledger.json",
            bindings_json=root / "bindings.json",
        )
        evidence, details = build(args)
        if evidence["preconditions"] != {
            "request_digest_readers_writers_retired": True,
            "server_owned_payload_fingerprint_active": True,
        }:
            fail("self-test did not mechanically prove both Phase C preconditions")
        if evidence["deployment"]["active_version_ids"] != [version_id] or details["quiescence_interval_seconds"] != 30:
            fail("self-test did not preserve exact single-version quiescence evidence")
        (source / "legacy.rs").write_text("fn legacy() { let request_digest = 1; }\n", encoding="utf-8")
        try:
            build(args)
        except PhaseCError:
            pass
        else:
            fail("self-test accepted a production request_digest reader/writer")
    print("V2 Phase C mechanical contract evidence self-test passed.")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    result.add_argument("--self-test", action="store_true")
    result.add_argument("--root", type=Path, default=Path("."))
    result.add_argument("--expected-source-sha")
    result.add_argument("--expected-release-set-id")
    result.add_argument("--first-observed-at-unix-seconds", type=int)
    result.add_argument("--observed-at-unix-seconds", type=int)
    result.add_argument("--deployment-before", type=Path)
    result.add_argument("--deployment-after", type=Path)
    result.add_argument("--identity-before", type=Path)
    result.add_argument("--identity-after", type=Path)
    result.add_argument("--ledger-json", type=Path)
    result.add_argument("--bindings-json", type=Path)
    result.add_argument("--output", type=Path)
    result.add_argument("--details-output", type=Path)
    return result


def main() -> int:
    args = parser().parse_args()
    try:
        if args.self_test:
            self_test()
            return 0
        required = (
            "expected_source_sha", "expected_release_set_id", "first_observed_at_unix_seconds",
            "observed_at_unix_seconds", "deployment_before", "deployment_after", "identity_before",
            "identity_after", "ledger_json", "bindings_json", "output", "details_output",
        )
        missing = [field for field in required if getattr(args, field) is None]
        if missing:
            fail(f"missing required arguments: {missing}")
        evidence, details = build(args)
        for path, value in ((args.output, evidence), (args.details_output, details)):
            if path.exists() or path.is_symlink():
                fail(f"output already exists: {path}")
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n", encoding="utf-8")
        return 0
    except (OSError, PhaseCError) as error:
        print(f"V2 Phase C evidence error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
