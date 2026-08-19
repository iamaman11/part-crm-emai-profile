#!/usr/bin/env python3
"""Fail-closed AR-10 real-runtime and executable-retirement policy gate."""

from __future__ import annotations

import argparse
import ast
import hashlib
import importlib.util
import json
import re
import sys
from pathlib import Path
from types import ModuleType
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
REAL_RUNTIME = Path("runtime/camouhost/real.py")
SYNTHETIC_RUNTIME = Path("runtime/camouhost/main.py")
RUNTIME_LOCK = Path("runtime/camouhost/runtime-lock.json")
OPSCTL_SOURCE = Path("tools/opsctl/src")
ADR = Path("docs/adr/ADR-0001-fingerprint-stability-policy.md")
AR10_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR10.md")
AR10_AUTHORITY = Path("architecture/runtime-cutover-ar10.json")
PYTHON_ESTATE_BASELINE = Path("architecture/python-estate-ar6.json")
PYTHON_ESTATE_OVERLAY = Path("architecture/python-estate-ar10.json")
PYTHON_ESTATE_GENERATOR = Path("scripts/python-estate-ar6.py")
LEGACY_EXECUTABLES = {
    "check_mail.py",
    "profile_manager.py",
    "test_fingerprint_consistency.py",
    "tools/cloud_profile_smoke.py",
    "tools/fingerprint_certify.py",
    "tools/profile_browser.py",
}
EXPECTED_LOCK: dict[str, Any] = {
    "browser": {
        "release_commit": "0583c3ec94f5a9df5cb2d09553fbfe80589b6e2d",
        "repository": "daijro/camoufox",
        "version": "152.0.4-beta.28",
    },
    "camouhost_ipc_version": 1,
    "components": {
        "browserforge": "1.2.4",
        "camoufox_python": "0.5.5",
        "playwright": "1.60.0",
    },
    "fingerprint_config_schema": "camoufox-canonical-config-v1",
    "fingerprint_policy_version": "profile-stability-v1",
    "python": "3.12",
    "python_source": {
        "commit": "cd83f7fd2fdf631dfde0c7eb53bd3d30f102ec4a",
        "repository": "daijro/camoufox",
    },
    "runtime_role": "real_camoufox",
    "schema_version": 1,
}


class GateError(ValueError):
    pass


def fail(message: str) -> None:
    raise GateError(message)


def read_regular(root: Path, relative: Path) -> str:
    path = root / relative
    if path.is_symlink() or not path.is_file():
        fail(f"required AR-10 file is missing/not regular: {relative.as_posix()}")
    return path.read_text(encoding="utf-8")


def read_regular_bytes(root: Path, relative: Path) -> bytes:
    path = root / relative
    if path.is_symlink() or not path.is_file():
        fail(f"required AR-10 file is missing/not regular: {relative.as_posix()}")
    return path.read_bytes()


def canonical_json_bytes(value: object) -> bytes:
    return (
        json.dumps(value, ensure_ascii=True, separators=(",", ":"), sort_keys=True) + "\n"
    ).encode("utf-8")


def function_node(tree: ast.Module, name: str) -> ast.FunctionDef:
    for node in tree.body:
        if isinstance(node, ast.FunctionDef) and node.name == name:
            return node
    fail(f"real Camouhost function is missing: {name}")


def called_names(node: ast.AST) -> set[str]:
    names: set[str] = set()
    for child in ast.walk(node):
        if not isinstance(child, ast.Call):
            continue
        target = child.func
        if isinstance(target, ast.Name):
            names.add(target.id)
        elif isinstance(target, ast.Attribute):
            names.add(target.attr)
    return names


def validate_runtime_lock(root: Path) -> None:
    raw = read_regular_bytes(root, RUNTIME_LOCK)
    try:
        parsed = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        fail(f"runtime lock is invalid JSON: {error}")
    if parsed != EXPECTED_LOCK:
        fail("runtime lock drifted from the exact AR-10 candidate component identity")
    if canonical_json_bytes(parsed) != raw:
        fail("runtime lock must use canonical JSON encoding")


def validate_real_runtime(root: Path) -> None:
    text = read_regular(root, REAL_RUNTIME)
    try:
        tree = ast.parse(text)
    except SyntaxError as error:
        fail(f"real Camouhost source does not parse: {error}")

    for marker in (
        'CONFIG_NAME = "camoufox-config.json"',
        'USER_DATA_NAME = "user_data"',
        'BRIDGE_LOCK_NAME = ".profile-platform.lock"',
        'EXPECTED_RUNTIME_LOCK_SHA256_ENV',
        'EXPECTED_CONFIG_SHA256_ENV',
        'EXPECTED_PROBE_SHA256_ENV',
        'persistent_context": True',
        '"browser": browser["version"]',
        '"i_know_what_im_doing": True',
        "profile-stable fingerprint drift detected",
        "fingerprint config digest mismatch",
        "Bridge writer ownership evidence is missing",
        "materialize_candidate_identity",
        'emit(f"ready|{active_session}")',
        'emit(f"closed|{active_session}|true")',
    ):
        if marker not in text:
            fail(f"real Camouhost lost required AR-10 invariant: {marker}")

    ipc = function_node(tree, "run_ipc")
    materializer = function_node(tree, "materialize_candidate_identity")
    launch = function_node(tree, "launch_verified_context")
    if "launch_options" in called_names(ipc):
        fail("normal active-generation launch may not regenerate BrowserForge identity")
    if "launch_options" not in called_names(materializer):
        fail("candidate identity materializer must explicitly create the initial exact config")
    if "stable_probe_digest" not in called_names(materializer):
        fail("candidate identity materializer must bind a profile-stable probe")
    if "stable_probe_digest" not in called_names(launch):
        fail("normal launch must verify profile-stable identity before ready")

    forbidden = (
        "print(config",
        "print(proxy",
        "print(os.environ",
        "requests.get(",
        "urllib.request",
        "subprocess.run(",
        "pip install",
    )
    for marker in forbidden:
        if marker in text:
            fail(f"real Camouhost contains forbidden authority/leak marker: {marker}")


def validate_synthetic_runtime(root: Path) -> None:
    text = read_regular(root, SYNTHETIC_RUNTIME)
    if "Deterministic fake Camouhost process" not in text:
        fail("synthetic Camouhost must remain mechanically identified as fake")
    for marker in ("from camoufox", "import camoufox", "persistent_context"):
        if marker in text:
            fail("synthetic Camouhost must never become real runtime authority")


def production_rust(text: str) -> str:
    return text.split("#[cfg(test)]", 1)[0]


def validate_opsctl(root: Path) -> None:
    source_root = root / OPSCTL_SOURCE
    if source_root.is_symlink() or not source_root.is_dir():
        fail("opsctl source root is missing")
    production = "\n".join(
        production_rust(path.read_text(encoding="utf-8"))
        for path in sorted(source_root.rglob("*.rs"))
        if path.is_file() and not path.is_symlink()
    )
    if "Command::new(" in production or "std::process::Command" in production:
        fail("AR-10 requires zero opsctl child-process spawn authority")
    for marker in (
        "reqwest::",
        "ureq::",
        "std::net::",
        "TcpStream",
        "worker::",
        "cloudflare::",
    ):
        if marker in production:
            fail(f"opsctl acquired forbidden provider/network execution authority: {marker}")


def git_blob_sha1(raw: bytes) -> str:
    header = f"blob {len(raw)}\0".encode("ascii")
    return hashlib.sha1(header + raw, usedforsecurity=False).hexdigest()


def load_python_estate_generator(root: Path) -> ModuleType:
    path = root / PYTHON_ESTATE_GENERATOR
    spec = importlib.util.spec_from_file_location("python_estate_ar6", path)
    if spec is None or spec.loader is None:
        fail("cannot load frozen AR-6 Python-estate classifier")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def validate_python_estate_overlay(root: Path) -> None:
    baseline_raw = read_regular_bytes(root, PYTHON_ESTATE_BASELINE)
    baseline = json.loads(baseline_raw.decode("utf-8"))
    overlay = json.loads(read_regular(root, PYTHON_ESTATE_OVERLAY))
    if baseline.get("status") != "AR6_ACCEPTED_PYTHON_ESTATE" or baseline.get(
        "accepted_program_checkpoint"
    ) != "AR-6":
        fail("AR-6 Python-estate baseline identity drifted")
    if overlay.get("schema_version") != 1 or overlay.get("status") != "AR10_CURRENT_PYTHON_ESTATE_OVERLAY":
        fail("AR-10 Python-estate overlay identity drifted")
    baseline_contract = overlay.get("baseline")
    if not isinstance(baseline_contract, dict):
        fail("AR-10 Python-estate overlay lacks frozen baseline contract")
    if baseline_contract.get("path") != PYTHON_ESTATE_BASELINE.as_posix() or baseline_contract.get(
        "immutable"
    ) is not True:
        fail("AR-10 Python-estate baseline must remain immutable")
    expected_blob = baseline_contract.get("git_blob_sha1")
    if not isinstance(expected_blob, str) or git_blob_sha1(baseline_raw) != expected_blob:
        fail("accepted AR-6 Python-estate baseline bytes changed")

    baseline_rows = baseline.get("files")
    additions = overlay.get("additions")
    retirements = overlay.get("retirements")
    if not isinstance(baseline_rows, list) or not isinstance(additions, list) or not isinstance(
        retirements, list
    ):
        fail("AR-10 Python-estate overlay collections are malformed")
    if any(not isinstance(path, str) or not path for path in retirements):
        fail("AR-10 Python-estate retirements must be non-empty path strings")
    if len(retirements) != len(set(retirements)):
        fail("AR-10 Python-estate retirements contain duplicates")

    baseline_by_path = {
        row["path"]: row
        for row in baseline_rows
        if isinstance(row, dict) and isinstance(row.get("path"), str)
    }
    if len(baseline_by_path) != len(baseline_rows):
        fail("accepted AR-6 Python-estate baseline contains malformed/duplicate rows")
    addition_by_path: dict[str, dict[str, Any]] = {}
    for row in additions:
        if not isinstance(row, dict) or not isinstance(row.get("path"), str):
            fail("AR-10 Python-estate addition row is malformed")
        path = row["path"]
        if path in addition_by_path or path in baseline_by_path:
            fail(f"AR-10 Python-estate addition is duplicate/already in baseline: {path}")
        if row.get("classification") != "KEEP_PYTHON" or not isinstance(row.get("reason"), str):
            fail(f"AR-10 Python-estate addition must be reviewed KEEP_PYTHON: {path}")
        addition_by_path[path] = row

    for path in retirements:
        baseline_row = baseline_by_path.get(path)
        if baseline_row is None:
            fail(f"AR-10 Python-estate retirement is not in frozen AR-6 baseline: {path}")
        if baseline_row.get("classification") != "DELETE_AFTER_SEQUENCE" or baseline_row.get(
            "cutover_slice"
        ) != "AR-10":
            fail(f"AR-10 may retire only its accepted DELETE_AFTER_SEQUENCE cohort: {path}")

    generator = load_python_estate_generator(root)
    current = generator.build_inventory(root)
    current_rows = current.get("files")
    if not isinstance(current_rows, list):
        fail("current Python-estate classifier returned malformed rows")
    current_by_path = {
        row["path"]: row
        for row in current_rows
        if isinstance(row, dict) and isinstance(row.get("path"), str)
    }
    if len(current_by_path) != len(current_rows):
        fail("current Python-estate classifier returned malformed/duplicate paths")

    expected_paths = (set(baseline_by_path) - set(retirements)) | set(addition_by_path)
    if set(current_by_path) != expected_paths:
        missing = sorted(expected_paths - set(current_by_path))
        unexpected = sorted(set(current_by_path) - expected_paths)
        fail(f"AR-10 Python-estate delta drifted: missing={missing}, unexpected={unexpected}")

    for path, baseline_row in baseline_by_path.items():
        if path in retirements:
            if path in current_by_path:
                fail(f"retired AR-10 Python executable is still tracked: {path}")
            continue
        if current_by_path.get(path) != baseline_row:
            fail(f"retained AR-6 Python classification changed during AR-10: {path}")
    for path, overlay_row in addition_by_path.items():
        current_row = current_by_path.get(path)
        if current_row is None or current_row.get("classification") != overlay_row["classification"]:
            fail(f"AR-10 Python addition classification drifted: {path}")

    if current.get("summary") != overlay.get("current_summary"):
        fail("AR-10 Python-estate current summary drifted")


def validate_legacy_retirement(root: Path) -> None:
    present = sorted(relative for relative in LEGACY_EXECUTABLES if (root / relative).exists())
    if present:
        fail(f"AR-6 DELETE_AFTER_SEQUENCE executable retirement is incomplete: {present}")

    active_roots = [root / ".github/workflows", root / "apps", root / "crates", root / "runtime"]
    for scan_root in active_roots:
        if not scan_root.exists():
            continue
        for path in scan_root.rglob("*"):
            if not path.is_file() or path.is_symlink():
                continue
            if path.suffix.lower() not in {".py", ".rs", ".yml", ".yaml", ".toml", ".json", ".md"}:
                continue
            text = path.read_text(encoding="utf-8", errors="strict")
            for legacy in LEGACY_EXECUTABLES:
                if legacy in text:
                    fail(
                        f"active runtime/workflow still references retired executable {legacy}: "
                        f"{path.relative_to(root)}"
                    )


def validate_acceptance_projection(root: Path) -> None:
    adr = read_regular(root, ADR)
    if "**Статус:** accepted" not in adr:
        fail("ADR-0001 must be accepted only with AR-10 executable evidence")
    for marker in (
        "Profile-Stable",
        "Origin-Deterministic",
        "Network-Bound",
        "Session-Dynamic",
        "candidate generation",
    ):
        if marker not in adr:
            fail(f"ADR-0001 lost required policy class/upgrade invariant: {marker}")

    authority = json.loads(read_regular(root, AR10_AUTHORITY))
    if authority.get("schema_version") != 1 or authority.get("status") != "AR10_IMPLEMENTED_PENDING_ACCEPTANCE":
        fail("AR-10 runtime-cutover machine authority has invalid state")
    if authority.get("production_mutation") is not False or authority.get("production_ready") is not False:
        fail("AR-10 must remain production fail-closed")
    if authority.get("legacy_executables_remaining") != 0:
        fail("AR-10 authority must project zero historical direct executables")
    if authority.get("real_runtime", {}).get("production_certified") is not False:
        fail("repository integration must not masquerade as external production certification")
    read_regular(root, AR10_EVIDENCE)


def validate_preflight(root: Path) -> None:
    """Validate successor runtime before parity/retirement is allowed to run."""
    validate_runtime_lock(root)
    validate_real_runtime(root)
    validate_synthetic_runtime(root)
    validate_opsctl(root)
    validate_python_estate_overlay(root)


def validate_closeout(root: Path) -> None:
    """Validate final AR-10 state only after successor parity has already been proved."""
    validate_preflight(root)
    validate_legacy_retirement(root)
    validate_acceptance_projection(root)


def self_test() -> None:
    mutated = json.loads(json.dumps(EXPECTED_LOCK))
    mutated["components"]["browserforge"] = "latest"
    if mutated == EXPECTED_LOCK:
        fail("runtime-lock negative self-test failed")
    source = "def run_ipc():\n    return launch_options()\n"
    node = function_node(ast.parse(source), "run_ipc")
    if "launch_options" not in called_names(node):
        fail("normal-launch regeneration negative self-test failed")
    if re.fullmatch(r"[0-9a-f]{64}", "0" * 64) is None:
        fail("digest self-test failed")
    if LEGACY_EXECUTABLES != {
        "check_mail.py",
        "profile_manager.py",
        "test_fingerprint_consistency.py",
        "tools/cloud_profile_smoke.py",
        "tools/fingerprint_certify.py",
        "tools/profile_browser.py",
    }:
        fail("legacy retirement cohort self-test drifted")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--closeout", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def main() -> int:
    arguments = parse_args()
    try:
        if arguments.self_test:
            self_test()
            print("AR-10 runtime cutover policy negative self-test passed.")
        elif arguments.closeout:
            validate_closeout(arguments.root.resolve())
            print("AR-10 real runtime, identity, opsctl, Python estate and executable-retirement closeout policy passed.")
        else:
            validate_preflight(arguments.root.resolve())
            print("AR-10 successor runtime and Python-estate preflight policy passed; parity may run before retirement.")
    except (GateError, OSError, UnicodeError, json.JSONDecodeError, ValueError) as error:
        print(f"AR-10 runtime cutover policy failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
