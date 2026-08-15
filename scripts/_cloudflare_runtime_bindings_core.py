#!/usr/bin/env python3
"""Prove the canonical Wrangler binding inventory matches accepted runtime source constants."""

from __future__ import annotations

import copy
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CONFIG = ROOT / "deploy" / "cloudflare" / "wrangler.jsonc"
RUNTIME_RUST_ROOTS = (
    ROOT / "apps" / "control-plane-worker" / "src",
    ROOT / "crates" / "cloudflare-adapters" / "src",
    ROOT / "crates" / "control-plane-contract" / "src",
)

SOURCE_CONSTANTS = {
    "asset": (
        "crates/control-plane-contract/src/lib.rs",
        "STATIC_ASSETS_BINDING",
    ),
    "d1": (
        "crates/control-plane-contract/src/lib.rs",
        "D1_CATALOG_BINDING",
    ),
    "r2": (
        "crates/control-plane-contract/src/lib.rs",
        "R2_PROFILES_BINDING",
    ),
    "verification_queue": (
        "crates/control-plane-contract/src/lib.rs",
        "VERIFICATION_QUEUE_BINDING",
    ),
    "profile_coordinator": (
        "crates/control-plane-contract/src/lib.rs",
        "PROFILE_COORDINATOR_BINDING",
    ),
    "integration_events_queue": (
        "apps/control-plane-worker/src/integration_events.rs",
        "INTEGRATION_EVENTS_QUEUE_BINDING",
    ),
    "mailbox_jobs_queue": (
        "apps/control-plane-worker/src/mailbox_scheduling.rs",
        "MAILBOX_JOBS_QUEUE_BINDING",
    ),
    "notification_hub": (
        "apps/control-plane-worker/src/realtime_notifications.rs",
        "NOTIFICATION_HUB_BINDING",
    ),
    "mailbox_secret_resolver": (
        "crates/cloudflare-adapters/src/cloud_mailbox_secrets.rs",
        "MAILBOX_SECRET_RESOLVER_BINDING",
    ),
    "access_issuer": (
        "apps/control-plane-worker/src/access_session.rs",
        "ACCESS_ISSUER_VAR",
    ),
    "access_audience": (
        "apps/control-plane-worker/src/access_session.rs",
        "ACCESS_AUDIENCE_VAR",
    ),
    "contact_keyring": (
        "apps/control-plane-worker/src/composition.rs",
        "CLIENT_CONTACT_PROTECTION_KEYRING_BINDING",
    ),
    "mailbox_resolver_caller_auth": (
        "crates/cloudflare-adapters/src/resolver_request.rs",
        "CALLER_AUTH_SECRET",
    ),
    "r2_account_id": (
        "apps/control-plane-worker/src/composition.rs",
        "R2_GENERATION_ACCOUNT_ID_BINDING",
    ),
    "r2_bucket_name": (
        "apps/control-plane-worker/src/composition.rs",
        "R2_GENERATION_BUCKET_NAME_BINDING",
    ),
    "r2_access_key_id": (
        "apps/control-plane-worker/src/composition.rs",
        "R2_GENERATION_ACCESS_KEY_ID_BINDING",
    ),
    "r2_secret_access_key": (
        "apps/control-plane-worker/src/composition.rs",
        "R2_GENERATION_SECRET_ACCESS_KEY_BINDING",
    ),
}


class BindingInventoryError(ValueError):
    pass


def production_rust_text() -> str:
    files = sorted(path for root in RUNTIME_RUST_ROOTS for path in root.rglob("*.rs"))
    if not files:
        raise BindingInventoryError("runtime Rust source inventory is empty")
    return "\n".join(path.read_text(encoding="utf-8") for path in files)


def rust_constant(relative_path: str, symbol: str, runtime_text: str) -> str:
    path = ROOT / relative_path
    text = path.read_text(encoding="utf-8")
    match = re.search(
        rf"\b(?:pub\s+)?const\s+{re.escape(symbol)}\s*:\s*&str\s*=\s*\"([^\"]+)\"\s*;",
        text,
    )
    if match is None:
        raise BindingInventoryError(f"runtime binding constant {symbol} is missing from {relative_path}")
    value = match.group(1)
    if runtime_text.count(symbol) < 2:
        raise BindingInventoryError(f"runtime binding constant {symbol} is defined but no longer used")
    return value


def source_inventory() -> dict[str, str]:
    runtime_text = production_rust_text()
    values = {
        key: rust_constant(path, symbol, runtime_text)
        for key, (path, symbol) in SOURCE_CONSTANTS.items()
    }
    if len(values) != len(set(values.values())):
        duplicates = sorted(
            value for value in set(values.values()) if list(values.values()).count(value) > 1
        )
        raise BindingInventoryError(f"runtime binding constants unexpectedly collide: {duplicates}")

    profile_source = (ROOT / "apps/control-plane-worker/src/profile_coordinator.rs").read_text(
        encoding="utf-8"
    )
    notification_source = (
        ROOT / "apps/control-plane-worker/src/realtime_notifications.rs"
    ).read_text(encoding="utf-8")
    if "pub struct ProfileCoordinator" not in profile_source:
        raise BindingInventoryError("ProfileCoordinator Durable Object class export is missing")
    if "pub struct NotificationHub" not in notification_source:
        raise BindingInventoryError("NotificationHub Durable Object class export is missing")
    return values


def object_value(value: object, label: str) -> dict[str, object]:
    if not isinstance(value, dict):
        raise BindingInventoryError(f"{label} must be an object")
    return value


def array_value(value: object, label: str) -> list[object]:
    if not isinstance(value, list):
        raise BindingInventoryError(f"{label} must be an array")
    return value


def binding_names(items: object, label: str) -> set[str]:
    names: set[str] = set()
    for item in array_value(items, label):
        binding = object_value(item, f"{label} entry").get("binding")
        if not isinstance(binding, str) or not binding:
            raise BindingInventoryError(f"{label} entry has no binding")
        if binding in names:
            raise BindingInventoryError(f"{label} contains duplicate binding {binding}")
        names.add(binding)
    return names


def validate_config(document: dict[str, object], source: dict[str, str]) -> None:
    assets = object_value(document.get("assets"), "assets")
    if assets.get("binding") != source["asset"]:
        raise BindingInventoryError("Workers Static Assets binding drifted from runtime source")

    envs = object_value(document.get("env"), "env")
    if set(envs) != {"staging", "production"}:
        raise BindingInventoryError("runtime binding proof requires exactly staging and production")

    expected_vars = {
        source["access_issuer"],
        source["access_audience"],
        source["r2_account_id"],
        source["r2_bucket_name"],
    }
    expected_secrets = {
        source["contact_keyring"],
        source["mailbox_resolver_caller_auth"],
        source["r2_access_key_id"],
        source["r2_secret_access_key"],
    }
    expected_queues = {
        source["verification_queue"],
        source["integration_events_queue"],
        source["mailbox_jobs_queue"],
    }
    expected_durable = {
        source["profile_coordinator"]: "ProfileCoordinator",
        source["notification_hub"]: "NotificationHub",
    }

    for environment in ("staging", "production"):
        config = object_value(envs.get(environment), f"env.{environment}")
        variables = object_value(config.get("vars"), f"env.{environment}.vars")
        if set(variables) != expected_vars:
            raise BindingInventoryError(f"{environment} vars do not match runtime source")

        secrets = object_value(config.get("secrets"), f"env.{environment}.secrets")
        required = array_value(secrets.get("required"), f"env.{environment}.secrets.required")
        if set(required) != expected_secrets or len(required) != len(expected_secrets):
            raise BindingInventoryError(f"{environment} secret-name inventory does not match runtime source")

        if binding_names(config.get("d1_databases"), f"env.{environment}.d1_databases") != {source["d1"]}:
            raise BindingInventoryError(f"{environment} D1 binding does not match runtime source")
        if binding_names(config.get("r2_buckets"), f"env.{environment}.r2_buckets") != {source["r2"]}:
            raise BindingInventoryError(f"{environment} R2 binding does not match runtime source")

        queues = object_value(config.get("queues"), f"env.{environment}.queues")
        if binding_names(queues.get("producers"), f"env.{environment}.queues.producers") != expected_queues:
            raise BindingInventoryError(f"{environment} queue producers do not match runtime source")
        if binding_names(config.get("services"), f"env.{environment}.services") != {source["mailbox_secret_resolver"]}:
            raise BindingInventoryError(f"{environment} service binding does not match runtime source")

        durable = object_value(config.get("durable_objects"), f"env.{environment}.durable_objects")
        observed: dict[str, str] = {}
        for item in array_value(durable.get("bindings"), f"env.{environment}.durable_objects.bindings"):
            entry = object_value(item, "Durable Object binding")
            name = entry.get("name")
            class_name = entry.get("class_name")
            if not isinstance(name, str) or not isinstance(class_name, str):
                raise BindingInventoryError("Durable Object binding must have string name/class_name")
            if name in observed:
                raise BindingInventoryError(f"duplicate Durable Object binding {name}")
            observed[name] = class_name
        if observed != expected_durable:
            raise BindingInventoryError(f"{environment} Durable Object bindings do not match runtime source")


def main() -> int:
    try:
        source = source_inventory()
        document = object_value(json.loads(CONFIG.read_text(encoding="utf-8")), "wrangler config")
        validate_config(document, source)

        tampered = copy.deepcopy(document)
        production = object_value(object_value(tampered["env"], "env")["production"], "production")
        durable = object_value(production["durable_objects"], "durable_objects")
        bindings = array_value(durable["bindings"], "durable bindings")
        durable["bindings"] = [
            entry
            for entry in bindings
            if object_value(entry, "durable binding").get("name") != source["notification_hub"]
        ]
        try:
            validate_config(tampered, source)
        except BindingInventoryError:
            print("Cloudflare runtime binding inventory and negative drift fixture passed.")
            return 0
        raise BindingInventoryError("missing NotificationHub negative fixture unexpectedly passed")
    except (OSError, json.JSONDecodeError, BindingInventoryError, KeyError) as error:
        print(f"cloudflare runtime binding inventory error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
