#!/usr/bin/env python3
"""Project saved Cloudflare observations into an AR-11 DeploymentSnapshot.

This adapter is observation-only. It does not authorize deployment, infer compatibility,
create resources, read secret values, or maintain hidden release state. Current Release Set
identity is read only from Worker version/deployment annotations written by the AR-11 executor.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any, Iterable

RELEASE_RE = re.compile(r"release_set=(release-set-v1-sha256-[0-9a-f]{64})\s+profile=([a-z0-9-]+)")
CORE_BINDINGS = {
    "ASSETS",
    "CATALOG_DB",
    "PROFILE_OBJECTS",
    "PROFILE_COORDINATOR",
    "NOTIFICATION_HUB",
    "INTEGRATION_EVENTS",
}
CORE_CREDENTIALS = {
    "CLIENT_CONTACT_PROTECTION_KEYRING",
    "R2_GENERATION_ACCESS_KEY_ID",
    "R2_GENERATION_SECRET_ACCESS_KEY",
}


class SnapshotError(ValueError):
    pass


def fail(message: str) -> None:
    raise SnapshotError(message)


def load(path: Path, label: str) -> Any:
    if path.is_symlink() or not path.is_file():
        fail(f"{label} must be a regular file: {path}")
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        raise SnapshotError(f"{label} is invalid JSON: {error}") from error


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def strings(value: Any) -> Iterable[str]:
    if isinstance(value, str):
        yield value
    elif isinstance(value, list):
        for child in value:
            yield from strings(child)
    elif isinstance(value, dict):
        for key, child in value.items():
            yield str(key)
            yield from strings(child)


def contains_exact_string(value: Any, expected: str) -> bool:
    return any(item == expected for item in strings(value))


def current_identity(deployment_status: Any) -> tuple[str | None, str | None]:
    observed: set[tuple[str, str]] = set()
    for value in strings(deployment_status):
        for match in RELEASE_RE.finditer(value):
            observed.add((match.group(1), match.group(2)))
    if len(observed) > 1:
        fail(f"provider deployment state contains ambiguous Release Set identities: {sorted(observed)}")
    if not observed:
        return None, None
    return next(iter(observed))


def object_value(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        fail(f"{label} must be a JSON object")
    return value


def rendered_core(config: dict[str, Any], environment: str) -> dict[str, Any]:
    if "build" in config:
        fail("rendered promotion config unexpectedly contains build authority")
    envs = object_value(config.get("env"), "rendered config env")
    if set(envs) != {environment}:
        fail("rendered config must contain exactly the selected environment")
    selected = object_value(envs[environment], f"rendered env.{environment}")
    serialized = json.dumps(config, sort_keys=True)
    if "MAILBOX_JOBS" in serialized or "MAILBOX_SECRET_RESOLVER" in serialized:
        fail("Mail operational bindings unexpectedly entered Core rendered config")
    if "${" in serialized:
        fail("rendered selected Core config contains unresolved placeholders")
    return selected


def binding_inventory(config: dict[str, Any], selected: dict[str, Any]) -> set[str]:
    observed: set[str] = set()
    assets = config.get("assets")
    if isinstance(assets, dict) and isinstance(assets.get("binding"), str):
        observed.add(assets["binding"])
    for field in ("d1_databases", "r2_buckets"):
        values = selected.get(field, [])
        if isinstance(values, list):
            for item in values:
                if isinstance(item, dict) and isinstance(item.get("binding"), str):
                    observed.add(item["binding"])
    queues = selected.get("queues")
    if isinstance(queues, dict):
        for item in queues.get("producers", []):
            if isinstance(item, dict) and isinstance(item.get("binding"), str):
                observed.add(item["binding"])
    durable = selected.get("durable_objects")
    if isinstance(durable, dict):
        for item in durable.get("bindings", []):
            if isinstance(item, dict) and isinstance(item.get("name"), str):
                observed.add(item["name"])
    if observed != CORE_BINDINGS:
        fail(f"rendered Core binding inventory drifted: {sorted(observed)}")
    return observed


def secret_names(secret_list: Any) -> set[str]:
    observed = {
        value
        for value in strings(secret_list)
        if value in CORE_CREDENTIALS
    }
    unknown_sensitive = {
        value
        for value in strings(secret_list)
        if value.startswith("MAILBOX_") or value in {"GOOGLE_OAUTH_CLIENT_SECRET", "MICROSOFT_OAUTH_CLIENT_SECRET"}
    }
    if unknown_sensitive:
        fail(f"Mail/resolver credentials unexpectedly present in Core secret observation: {sorted(unknown_sensitive)}")
    return observed


def current_components(path: Path | None, release_set_id: str | None) -> dict[str, str]:
    if release_set_id is None:
        if path is not None:
            fail("current Release Set manifest supplied while provider reports no current Release Set")
        return {}
    if path is None:
        fail("provider reports a current Release Set but its immutable manifest was not supplied")
    value = object_value(load(path, "current Release Set manifest"), "current Release Set manifest")
    if value.get("release_set_id") != release_set_id:
        fail("current Release Set manifest does not match provider deployment annotation")
    components = object_value(value.get("components"), "current Release Set components")
    result: dict[str, str] = {}
    for name, row in components.items():
        entry = object_value(row, f"current component {name}")
        release_id = entry.get("release_id")
        if not isinstance(release_id, str) or not release_id:
            fail(f"current component {name} has no release_id")
        result[name] = release_id
    return result


def build(args: argparse.Namespace) -> dict[str, Any]:
    if args.environment not in {"rehearsal", "staging"}:
        fail("AR-11 snapshot adapter is pre-production only")
    deployment_status = load(args.deployment_status, "deployment status")
    catalog_ledger = load(args.catalog_ledger, "Catalog D1 ledger")
    r2_list = load(args.r2_list, "R2 bucket list")
    queue_list = load(args.queue_list, "Queue list")
    secret_list = load(args.secret_list, "secret-name list")
    config = object_value(load(args.rendered_config, "rendered Core config"), "rendered Core config")
    deploy_manifest = object_value(load(args.deploy_manifest, "deploy manifest"), "deploy manifest")
    control = deploy_manifest.get("control_plane", deploy_manifest)
    control = object_value(control, "control_plane deploy manifest")
    selected = rendered_core(config, "staging")

    release_set_id, profile_id = current_identity(deployment_status)
    components = current_components(args.current_release_set, release_set_id)
    logical_resources: set[str] = set()
    if deployment_status not in ({}, [], None):
        logical_resources.update({"control_plane_worker", "profile_coordinator", "notification_hub", "control_plane_schedule"})
    if catalog_ledger not in ({}, [], None):
        logical_resources.add("catalog_d1")
    r2_name = control.get("r2_bucket_name")
    if isinstance(r2_name, str) and contains_exact_string(r2_list, r2_name):
        logical_resources.add("profile_objects")
    queue_name = control.get("integration_events_queue")
    if isinstance(queue_name, str) and contains_exact_string(queue_list, queue_name):
        logical_resources.add("integration_events")

    bindings = binding_inventory(config, selected)
    credentials = secret_names(secret_list)
    d1_id = control.get("d1_database_id")
    if not isinstance(d1_id, str) or not d1_id:
        fail("control_plane deploy manifest lacks d1_database_id")

    return {
        "schema_version": 1,
        "kind": "DEPLOYMENT_SNAPSHOT",
        "environment": args.environment,
        "collected_at": args.collected_at,
        "release_set_id": release_set_id,
        "capability_profile_id": profile_id,
        "component_release_ids": components,
        "workers": [{"observed": deployment_status not in ({}, [], None)}],
        "d1": [{
            "component": "catalog",
            "binding": "CATALOG_DB",
            "database_id": d1_id,
            "ledger_sha256": sha256_file(args.catalog_ledger),
        }],
        "r2": [{"bucket_name": r2_name, "observed": "profile_objects" in logical_resources}],
        "queues": [{"queue_name": queue_name, "observed": "integration_events" in logical_resources}],
        "dlqs": [],
        "durable_objects": [{"name": "PROFILE_COORDINATOR"}, {"name": "NOTIFICATION_HUB"}],
        "service_bindings": [],
        "routes": [str(control.get("custom_domain", ""))],
        "schedules": ["* * * * *"],
        "credential_metadata": [{"name": name} for name in sorted(credentials)],
        "observed_logical_resources": sorted(logical_resources),
        "observed_logical_bindings": sorted(bindings),
        "observed_logical_credentials": sorted(credentials),
    }


def self_test() -> None:
    first = {"annotations": {"workers/message": "release_set=release-set-v1-sha256-" + "a" * 64 + " profile=rehearsal-core-v1"}}
    second = {"message": "release_set=release-set-v1-sha256-" + "b" * 64 + " profile=rehearsal-core-v1"}
    if current_identity(first)[0] is None:
        fail("current Release Set marker fixture was not detected")
    try:
        current_identity([first, second])
    except SnapshotError:
        print("AR-11 DeploymentSnapshot adapter self-test passed.")
        return
    fail("ambiguous deployment identity fixture unexpectedly passed")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--environment")
    parser.add_argument("--collected-at")
    parser.add_argument("--deployment-status", type=Path)
    parser.add_argument("--catalog-ledger", type=Path)
    parser.add_argument("--r2-list", type=Path)
    parser.add_argument("--queue-list", type=Path)
    parser.add_argument("--secret-list", type=Path)
    parser.add_argument("--rendered-config", type=Path)
    parser.add_argument("--deploy-manifest", type=Path)
    parser.add_argument("--current-release-set", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test:
            self_test()
            return 0
        required = [
            args.environment,
            args.collected_at,
            args.deployment_status,
            args.catalog_ledger,
            args.r2_list,
            args.queue_list,
            args.secret_list,
            args.rendered_config,
            args.deploy_manifest,
            args.output,
        ]
        if any(value is None for value in required):
            fail("all observation inputs except current-release-set are required")
        result = build(args)
        if args.output.exists():
            fail(f"snapshot output already exists: {args.output}")
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n", encoding="utf-8")
        return 0
    except (OSError, SnapshotError) as error:
        print(f"AR-11 DeploymentSnapshot error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
