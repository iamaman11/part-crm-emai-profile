#!/usr/bin/env python3
"""Fail-closed traceability checker for the 37 mandatory AR-11 negative behaviours."""

from __future__ import annotations

import argparse
import json
from pathlib import Path, PurePosixPath
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
MATRIX = ROOT / "architecture" / "ar11-behavioural-certification.json"
EXPECTED_CASES = tuple(range(1, 38))
ALLOWED_KINDS = {
    "rust_test",
    "node_negative_fixture",
    "node_gate",
    "workflow_gate",
    "typed_invariant",
    "absent_paths_gate",
    "opsctl_offline_scan",
}
EXECUTABLE_KINDS = {
    "rust_test",
    "node_negative_fixture",
    "node_gate",
    "workflow_gate",
    "absent_paths_gate",
    "opsctl_offline_scan",
}
LEGACY_D3 = (
    ".github/workflows/mailbox-secret-resolver-promotion.yml",
    "scripts/mailbox-secret-resolver-promotion.py",
    "scripts/_mailbox_secret_resolver_promotion_core.py",
)
OPSCTL_FORBIDDEN = (
    "reqwest",
    "ureq",
    "hyper::",
    "std::net",
    "TcpStream",
    "UdpSocket",
    "std::process::Command",
    "tokio::process",
    "CLOUDFLARE_API_TOKEN",
    "CLOUDFLARE_DEPLOY_MANIFEST_JSON",
    "wrangler deploy",
    "wrangler d1 execute",
    "secret put",
)


class CertificationError(ValueError):
    pass


def fail(message: str) -> None:
    raise CertificationError(message)


def safe_path(value: str) -> Path:
    pure = PurePosixPath(value)
    if not value or pure.is_absolute() or any(part in {"", ".", ".."} for part in pure.parts):
        fail(f"unsafe proof path: {value!r}")
    path = ROOT.joinpath(*pure.parts)
    try:
        path.resolve(strict=True).relative_to(ROOT.resolve(strict=True))
    except (FileNotFoundError, ValueError) as error:
        raise CertificationError(f"proof path is missing/escapes repository: {value}") from error
    if path.is_symlink() or not path.is_file():
        fail(f"proof path must be a regular repository file: {value}")
    return path


def validate_opsctl_offline_boundary() -> None:
    manifest = safe_path("tools/opsctl/Cargo.toml").read_text(encoding="utf-8")
    dependencies = manifest.split("[dependencies]", 1)[1].split("[", 1)[0]
    dependency_lines = [line.strip() for line in dependencies.splitlines() if line.strip() and not line.lstrip().startswith("#")]
    if dependency_lines != ['serde_json = "=1.0.151"']:
        fail(f"opsctl dependency surface drifted: {dependency_lines}")
    for path in sorted((ROOT / "tools" / "opsctl" / "src").rglob("*.rs")):
        text = path.read_text(encoding="utf-8")
        for marker in OPSCTL_FORBIDDEN:
            if marker.lower() in text.lower():
                fail(f"opsctl gained forbidden network/provider/secret/process authority {marker!r} in {path.relative_to(ROOT)}")


def validate_absent_legacy_d3() -> None:
    present = [value for value in LEGACY_D3 if (ROOT / value).exists()]
    if present:
        fail(f"retired Python D3 operational authority became callable again: {present}")


def validate_document(document: dict[str, Any]) -> None:
    if document.get("schema_version") != 1 or document.get("kind") != "AR11_BEHAVIOURAL_CERTIFICATION_MATRIX":
        fail("behavioural certification identity/schema mismatch")
    cases = document.get("cases")
    if not isinstance(cases, list):
        fail("behavioural certification cases must be an array")
    observed_numbers = [case.get("number") for case in cases if isinstance(case, dict)]
    if tuple(observed_numbers) != EXPECTED_CASES:
        fail(f"behavioural certification must contain ordered exact cases 1..37; observed={observed_numbers}")
    ids: set[str] = set()
    for case in cases:
        if not isinstance(case, dict) or set(case) != {"number", "case_id", "requirement", "proofs"}:
            fail(f"case has invalid shape: {case!r}")
        number = case["number"]
        expected_id = f"AR11-N-{number:02d}"
        if case["case_id"] != expected_id or expected_id in ids:
            fail(f"case ID mismatch/duplicate for requirement {number}")
        ids.add(expected_id)
        if not isinstance(case["requirement"], str) or not case["requirement"].strip():
            fail(f"case {expected_id} requirement must be non-empty")
        proofs = case["proofs"]
        if not isinstance(proofs, list) or not proofs:
            fail(f"case {expected_id} has no permanent proof")
        executable = False
        for proof in proofs:
            if not isinstance(proof, dict) or set(proof) != {"proof_kind", "path", "identifier"}:
                fail(f"case {expected_id} has invalid proof shape")
            kind = proof["proof_kind"]
            identifier = proof["identifier"]
            if kind not in ALLOWED_KINDS:
                fail(f"case {expected_id} has unsupported proof kind {kind!r}")
            if not isinstance(identifier, str) or not identifier.strip():
                fail(f"case {expected_id} proof identifier must be non-empty")
            path = safe_path(proof["path"])
            text = path.read_text(encoding="utf-8")
            if kind == "rust_test" and f"fn {identifier}" not in text:
                fail(f"case {expected_id} Rust test identifier not found: {identifier}")
            elif kind not in {"opsctl_offline_scan", "absent_paths_gate"} and identifier not in text:
                fail(f"case {expected_id} proof identifier not found in {proof['path']}: {identifier}")
            if kind == "opsctl_offline_scan":
                validate_opsctl_offline_boundary()
            if kind == "absent_paths_gate":
                if identifier not in text:
                    fail(f"case {expected_id} legacy-D3 gate identifier missing")
                validate_absent_legacy_d3()
            executable = executable or kind in EXECUTABLE_KINDS
        if not executable:
            fail(f"case {expected_id} is backed only by source invariants; executable gate/test required")


def load() -> dict[str, Any]:
    try:
        value = json.loads(MATRIX.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise CertificationError(f"cannot load behavioural certification matrix: {error}") from error
    if not isinstance(value, dict):
        fail("behavioural certification root must be an object")
    return value


def self_test(document: dict[str, Any]) -> None:
    missing = json.loads(json.dumps(document))
    missing["cases"].pop()
    try:
        validate_document(missing)
    except CertificationError:
        pass
    else:
        fail("missing-case negative fixture unexpectedly passed")

    bogus = json.loads(json.dumps(document))
    bogus["cases"][0]["proofs"][0]["identifier"] = "proof_identifier_that_does_not_exist"
    try:
        validate_document(bogus)
    except CertificationError:
        pass
    else:
        fail("bogus-proof negative fixture unexpectedly passed")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        document = load()
        validate_document(document)
        if args.self_test:
            self_test(document)
            print("AR-11 37/37 behavioural certification negative self-test passed.")
        else:
            print("AR-11 behavioural certification matrix passed: 37/37 executable proofs bound.")
        return 0
    except CertificationError as error:
        print(f"AR-11 behavioural certification failed: {error}", file=__import__("sys").stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
