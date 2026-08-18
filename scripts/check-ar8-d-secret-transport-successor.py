#!/usr/bin/env python3
"""Fail-closed checks for the governed D3 -> AR-8D Worker-secret transport transition."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path
from typing import Any

ROOT = Path.cwd()
AUTHORITY = Path("architecture/ar8-d-secret-transport-successor.json")
PROMOTION = Path(".github/workflows/mailbox-secret-resolver-promotion.yml")
BINDING_HELPER = Path(".github/scripts/worker-secret-bindings.mjs")
D3_AUTHORITY = Path("architecture/pre2j-d3-resolver-bootstrap-authority.json")
D3_MARKER = Path("architecture/pre2j-d3-resolver-bootstrap-implementation.json")

EXPECTED_AUTHORITY: dict[str, Any] = {
    "schema_version": 1,
    "kind": "POLICY_TRANSITION",
    "status": "candidate",
    "tracking_issue": 361,
    "parent_issue": 308,
    "canonical_inventory": "architecture/inventory.json",
    "predecessor": {
        "policy": "Pre-2J D3 resolver/bootstrap secret-bundle transport",
        "d3_authority": str(D3_AUTHORITY),
        "d3_authority_commit": "6a7dad9f74a25ccfd77cdd1a76216d8a46694e10",
        "d3_implementation_marker": str(D3_MARKER),
        "d3_implementation_merge_commit": "25bf15887fc835a6109c34ce21c083f3c307c455",
        "transition_base_main": "9635ef21aafa0e2ff04551ef4cecf9497cbc87d5",
        "promotion_workflow": str(PROMOTION),
        "promotion_workflow_git_blob_sha": "85fd78557c97c96c179ff5d45f338bf12e639305",
    },
    "successor": {
        "policy": "AR-8D steady-state Worker secret binding verification",
        "promotion_workflow": str(PROMOTION),
        "binding_metadata_helper": str(BINDING_HELPER),
        "worker_secret_value_authority": "Cloudflare Worker secret store",
        "required_binding_contract": "env.<environment>.secrets.required",
        "verification_command": "wrangler secret list --format json",
        "routine_deploy_secret_value_transport": False,
        "routine_deploy_secret_mutation": False,
        "rotation_lifecycle": "separate_explicit_rotation_authority",
    },
    "invariants": [
        "historical D3 secret-bundle transport remains mechanically provable at the transition base",
        "routine promotion verifies exact Worker secret binding names using metadata only",
        "routine promotion never receives Worker runtime secret bundles",
        "routine promotion never mutates Worker secret values",
        "secret rotation remains a separate governed lifecycle",
        "AR-8 performs no production credential mutation",
    ],
}


def git(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args], cwd=ROOT, check=check, text=True, capture_output=True
    )


def read(path: Path) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def load(path: Path) -> dict[str, Any]:
    value = json.loads(read(path))
    if not isinstance(value, dict):
        raise ValueError(f"{path}: root must be an object")
    return value


def ensure_commit(ref: str) -> list[str]:
    if git("cat-file", "-e", f"{ref}^{{commit}}", check=False).returncode == 0:
        return []
    fetched = git("fetch", "--no-tags", "--depth=1", "origin", ref, check=False)
    if fetched.returncode != 0 or git(
        "cat-file", "-e", f"{ref}^{{commit}}", check=False
    ).returncode != 0:
        return [f"governed predecessor commit is unavailable: {ref}"]
    return []


def predecessor_errors(authority: dict[str, Any]) -> list[str]:
    predecessor = authority.get("predecessor")
    if not isinstance(predecessor, dict):
        return ["AR-8D transition predecessor metadata is missing"]
    ref = str(predecessor.get("transition_base_main", ""))
    errors = ensure_commit(ref)
    if errors:
        return errors
    expected_blob = predecessor.get("promotion_workflow_git_blob_sha")
    actual_blob = git("rev-parse", f"{ref}:{PROMOTION}", check=False)
    if actual_blob.returncode != 0 or actual_blob.stdout.strip() != expected_blob:
        errors.append("historical D3 promotion workflow blob drifted from the governed transition base")
    for path in (D3_AUTHORITY, D3_MARKER):
        historical = git("show", f"{ref}:{path}", check=False)
        if historical.returncode != 0 or historical.stdout != read(path):
            errors.append(f"historical D3 authority changed after the AR-8D transition: {path}")
    return errors


def promotion_errors(promotion: str) -> list[str]:
    errors: list[str] = []
    required = (
        "workflow_dispatch:",
        "github-preflight",
        "mailbox-secret-resolver-promotion.py download-raw-artifact",
        "--expected-digest",
        "--expected-name",
        "validate-release-identities",
        "validate-staging-evidence",
        "verify-remote-d1",
        "deployments status",
        "attest",
        "CLOUDFLARE_API_TOKEN",
        "CLOUDFLARE_ACCESS_CLIENT_ID",
        "CLOUDFLARE_ACCESS_CLIENT_SECRET",
        "CLOUDFLARE_DEPLOY_MANIFEST_JSON",
        "worker-secret-bindings.mjs --normalize",
        "wrangler@4.94.0 secret list",
        "--format json",
        "--secret-list",
        "deploy --dry-run",
        "--strict",
        "--experimental-autoconfig=false",
    )
    for marker in required:
        if marker not in promotion:
            errors.append(f"AR-8D routine promotion is missing {marker!r}")

    forbidden = (
        "CLOUDFLARE_RESOLVER_SECRETS_JSON",
        "CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON",
        "--secrets-file",
        "validate-secrets",
        " secret put ",
        " secret bulk ",
        " secret delete ",
        " secrets put ",
        " secrets bulk ",
        " secrets delete ",
        "worker-build --release",
        "cargo build",
        "wrangler d1 create",
        "wrangler r2 bucket create",
        "wrangler queues create",
        "BOOTSTRAP_API_TOKEN",
    )
    lowered = f" {promotion.lower()} "
    for marker in forbidden:
        if marker.lower() in lowered:
            errors.append(f"AR-8D routine promotion contains forbidden secret/rebuild authority: {marker}")

    if promotion.count("worker-secret-bindings.mjs --normalize") != 2:
        errors.append("resolver and control-plane configs must each restore secrets.required exactly once")
    if promotion.count("wrangler@4.94.0 secret list") != 2:
        errors.append("resolver and control-plane Workers must each expose one metadata-only secret inventory")
    if promotion.count("--secret-list") != 2:
        errors.append("resolver and control-plane secret inventories must each be validated exactly once")
    if promotion.count("mailbox-secret-resolver-promotion.py download-raw-artifact") != 4:
        errors.append("immutable resolver/control-plane raw artifacts must still be acquired four times")
    if promotion.count("--expected-digest") != 4 or promotion.count("--expected-name") != 4:
        errors.append("all immutable raw artifact acquisitions must remain digest/name bound")
    if promotion.count("validate-release-identities") != 2:
        errors.append("preflight and deployment must both bind exact release identities")
    if promotion.count("validate-staging-evidence") != 1:
        errors.append("production must validate exactly one immutable staging evidence artifact")
    if promotion.count("deploy --dry-run") != 2:
        errors.append("both immutable Worker artifacts must still pass Wrangler dry-run")
    if promotion.count("--strict") != 4 or promotion.count("--experimental-autoconfig=false") != 4:
        errors.append("both dry-runs and both deploys must retain strict/autoconfig-off validation")

    normalize_end = promotion.rfind("worker-secret-bindings.mjs --normalize")
    secret_list_start = promotion.find("wrangler@4.94.0 secret list")
    if normalize_end < 0 or secret_list_start < 0 or normalize_end >= secret_list_start:
        errors.append("declarative secrets.required must be restored before remote binding metadata is read")
    first_validation = promotion.find("--secret-list")
    first_dry_run = promotion.find("deploy --dry-run")
    if first_validation < 0 or first_dry_run < 0 or first_validation >= first_dry_run:
        errors.append("exact Worker secret binding validation must complete before immutable artifact dry-run")
    resolver_config = promotion.find('--config "$resolver_config"', secret_list_start)
    control_config = promotion.find('--config "$control_config"', secret_list_start)
    if resolver_config < 0 or control_config < 0 or resolver_config >= control_config:
        errors.append("resolver verification/deployment must remain ordered before the control plane")
    return errors


def helper_errors(helper: str) -> list[str]:
    errors: list[str] = []
    for marker in (
        "FORBIDDEN_VALUE_KEYS",
        "rejectValueShapedFields(secretList)",
        "secrets.required",
        "ALLOWED_SECRET_TYPES",
        "secret_text",
        "JSON.stringify(actual) !== JSON.stringify(expected)",
        "--normalize",
        "mode: 0o600",
        "selfTest()",
    ):
        if marker not in helper:
            errors.append(f"Worker secret binding helper is missing {marker!r}")
    return errors


def diff_errors(base_ref: str) -> list[str]:
    errors: list[str] = []
    if git("cat-file", "-e", f"{base_ref}^{{commit}}", check=False).returncode != 0:
        return [f"AR-8D base ref is unavailable: {base_ref}"]
    for path in (D3_AUTHORITY, D3_MARKER):
        if git("diff", "--quiet", base_ref, "--", str(path), check=False).returncode != 0:
            errors.append(f"AR-8D successor must not edit historical D3 authority: {path}")
    return errors


def current_errors() -> list[str]:
    errors: list[str] = []
    try:
        authority = load(AUTHORITY)
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        return [f"AR-8D transition authority is unreadable: {exc}"]
    if authority != EXPECTED_AUTHORITY:
        errors.append("AR-8D secret-transport successor authority drifted from its exact governed contract")
    errors.extend(predecessor_errors(authority))
    errors.extend(promotion_errors(read(PROMOTION)))
    errors.extend(helper_errors(read(BINDING_HELPER)))
    return errors


def self_test() -> None:
    errors = current_errors()
    if errors:
        raise AssertionError(errors)
    promotion = read(PROMOTION)
    helper = read(BINDING_HELPER)
    assert promotion_errors(promotion + "\nCLOUDFLARE_RESOLVER_SECRETS_JSON")
    assert promotion_errors(promotion + "\n--secrets-file forbidden.json")
    assert promotion_errors(promotion + "\nnpx wrangler secret put NAME")
    assert promotion_errors(promotion.replace("wrangler@4.94.0 secret list", "wrangler@4.94.0 secret get", 1))
    assert helper_errors(helper.replace("rejectValueShapedFields(secretList);", "", 1))
    node = subprocess.run(
        ["node", str(BINDING_HELPER), "--self-test"],
        cwd=ROOT,
        check=False,
        text=True,
        capture_output=True,
    )
    if node.returncode != 0:
        raise AssertionError(node.stderr.strip() or node.stdout.strip())
    print("AR-8D secret-transport successor negative policy self-test passed.")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-ref")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    errors = current_errors()
    if args.base_ref:
        errors.extend(diff_errors(args.base_ref))
    if errors:
        raise SystemExit("\n".join(errors))
    print("AR-8D governed D3 -> steady-state secret-transport transition is valid.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
