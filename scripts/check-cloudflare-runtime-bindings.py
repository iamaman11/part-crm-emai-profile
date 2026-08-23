#!/usr/bin/env python3
"""Canonical Cloudflare runtime-binding, topology and AR-11 successor fitness gate.

Current runtime proof is profile-aware and provider-native: Wrangler configuration and
Product Rust own the executable resource/workload topology. Historical D3 promotion is
proved from immutable Git history by the AR-8D checker; current promotion authority is
the Rust-backed AR-11 Release Set workflow.
"""

from __future__ import annotations

import copy
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

import _cloudflare_runtime_bindings_core as core

ROOT = Path(__file__).resolve().parents[1]
CONTROL_CONFIG = ROOT / "deploy/cloudflare/wrangler.jsonc"
RESOLVER_CONFIG = ROOT / "deploy/cloudflare/mailbox-secret-resolver.wrangler.jsonc"
QUEUE_ENTRYPOINT = ROOT / "apps/control-plane-worker/src/lib.rs"
QUEUE_ENVELOPE = ROOT / "crates/cloudflare-adapters/src/control_plane_queue.rs"
CONTROL_PLANE_CONTRACT = ROOT / "crates/control-plane-contract/src/lib.rs"
GENERATION_ROUTE = ROOT / "apps/control-plane-worker/src/profile_generations.rs"
AR8D_CHECKER = ROOT / ".github/scripts/ar8-d-secret-transport-successor.mjs"
AR11_CHECKER = ROOT / ".github/scripts/release-operational-ar11.mjs"


class RuntimeTopologyError(ValueError):
    pass


def fail(message: str) -> None:
    raise RuntimeTopologyError(message)


def load(path: Path, label: str) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        fail(f"{label} must be an object")
    return value


def object_value(value: object, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        fail(f"{label} must be an object")
    return value


def array_value(value: object, label: str) -> list[Any]:
    if not isinstance(value, list):
        fail(f"{label} must be an array")
    return value


def queue_consumers(control: dict[str, Any], environment: str) -> set[str]:
    selected = object_value(object_value(control["env"], "env")[environment], environment)
    queues = object_value(selected.get("queues"), f"{environment}.queues")
    result: set[str] = set()
    for item in array_value(queues.get("consumers"), f"{environment}.consumers"):
        entry = object_value(item, "queue consumer")
        queue = entry.get("queue")
        if not isinstance(queue, str) or not queue or queue in result:
            fail(f"{environment} queue consumer inventory is invalid")
        result.add(queue)
    return result


def queue_producers(control: dict[str, Any], environment: str) -> set[str]:
    selected = object_value(object_value(control["env"], "env")[environment], environment)
    queues = object_value(selected.get("queues"), f"{environment}.queues")
    result: set[str] = set()
    for item in array_value(queues.get("producers"), f"{environment}.producers"):
        entry = object_value(item, "queue producer")
        binding = entry.get("binding")
        if not isinstance(binding, str) or not binding or binding in result:
            fail(f"{environment} queue producer inventory is invalid")
        result.add(binding)
    return result


def validate_core_queue_closure(control: dict[str, Any]) -> None:
    expected_consumers = {
        "staging": {"${STAGING_INTEGRATION_EVENTS_QUEUE}"},
        "production": {"${PRODUCTION_INTEGRATION_EVENTS_QUEUE}"},
    }
    for environment in ("staging", "production"):
        consumers = queue_consumers(control, environment)
        producers = queue_producers(control, environment)
        if consumers != expected_consumers[environment]:
            fail(f"{environment} Core queue consumer closure drifted: {sorted(consumers)}")
        if producers != {"INTEGRATION_EVENTS"}:
            fail(f"{environment} Core queue producer closure drifted: {sorted(producers)}")


def validate_generation_verification_runtime(control: dict[str, Any]) -> None:
    envelope = QUEUE_ENVELOPE.read_text(encoding="utf-8")
    entrypoint = QUEUE_ENTRYPOINT.read_text(encoding="utf-8")
    contract = CONTROL_PLANE_CONTRACT.read_text(encoding="utf-8")
    route = GENERATION_ROUTE.read_text(encoding="utf-8")
    for environment in ("staging", "production"):
        if "GENERATION_VERIFICATION" in queue_producers(control, environment):
            fail(f"{environment} unexpectedly restored GENERATION_VERIFICATION")
    if "VERIFICATION_QUEUE_BINDING" in contract or "GENERATION_VERIFICATION" in contract:
        fail("legacy generation-verification contract authority still exists")
    if "VERIFICATION_QUEUE_BINDING" in entrypoint or 'env.queue("GENERATION_VERIFICATION")' in entrypoint:
        fail("runtime probes deleted generation-verification Queue")
    enum_match = re.search(r"pub enum ControlPlaneQueueMessage\s*\{(?P<body>.*?)\n\}", envelope, re.S)
    if enum_match is None:
        fail("control-plane Queue envelope enum is missing")
    variants = re.findall(r"^\s*([A-Z][A-Za-z0-9_]*)\(", enum_match.group("body"), re.M)
    if variants != ["IntegrationEvent", "MailboxJob"]:
        fail(f"control-plane Queue envelope drifted: {variants}")
    if "RouteClass::ProfileGenerationVerifyApi" not in route or "execute_verify_generation(" not in route:
        fail("synchronous profile-generation verification authority disappeared")
    if "VERIFICATION_QUEUE_BINDING" in route or "GENERATION_VERIFICATION" in route:
        fail("profile-generation verification depends on deleted Queue binding")


def validate_resolver_source_isolation(resolver: dict[str, Any]) -> None:
    if resolver.get("name") != "mailbox-secret-resolver" or resolver.get("workers_dev") is not False:
        fail("mailbox secret resolver source topology drifted")
    if resolver.get("triggers") != {"crons": ["17 * * * *"]}:
        fail("resolver reconciliation schedule drifted")
    envs = object_value(resolver.get("env"), "resolver env")
    if set(envs) != {"staging", "production"}:
        fail("resolver must define exactly staging and production")
    database_ids: set[str] = set()
    for environment in ("staging", "production"):
        selected = object_value(envs[environment], f"resolver {environment}")
        if selected.get("workers_dev") is not False or selected.get("routes") != []:
            fail(f"resolver {environment} must remain private/no-route")
        databases = array_value(selected.get("d1_databases"), f"resolver {environment} D1")
        if len(databases) != 1:
            fail(f"resolver {environment} must own exactly one D1")
        database = object_value(databases[0], "resolver D1")
        if database.get("binding") != "RESOLVER_DB" or database.get("migrations_dir") != "../../migrations/resolver-d1":
            fail(f"resolver {environment} D1 boundary drifted")
        identity = database.get("database_id")
        if not isinstance(identity, str):
            fail(f"resolver {environment} D1 identity is missing")
        database_ids.add(identity)
    if len(database_ids) != 2:
        fail("resolver staging/production D1 identities must remain isolated")


def run_checker(path: Path, *args: str) -> None:
    completed = subprocess.run(
        ["node", path.relative_to(ROOT).as_posix(), *args],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if completed.returncode != 0:
        details = "\n".join(value.strip() for value in (completed.stdout, completed.stderr) if value.strip())
        fail(f"{path.name} failed:\n{details}")


def self_test(control: dict[str, Any], resolver: dict[str, Any]) -> None:
    restored = copy.deepcopy(control)
    production = object_value(object_value(restored["env"], "env")["production"], "production")
    queues = object_value(production["queues"], "queues")
    array_value(queues["producers"], "producers").append(
        {"binding": "GENERATION_VERIFICATION", "queue": "legacy-generation-verification"}
    )
    try:
        validate_core_queue_closure(restored)
    except RuntimeTopologyError:
        pass
    else:
        fail("legacy Queue restoration negative fixture unexpectedly passed")

    exposed_resolver = copy.deepcopy(resolver)
    resolver_production = object_value(
        object_value(exposed_resolver["env"], "resolver env")["production"],
        "resolver production",
    )
    resolver_production["routes"] = ["https://resolver.invalid/*"]
    try:
        validate_resolver_source_isolation(exposed_resolver)
    except RuntimeTopologyError:
        pass
    else:
        fail("public resolver route negative fixture unexpectedly passed")

    shared_database = copy.deepcopy(resolver)
    resolver_env = object_value(shared_database["env"], "resolver env")
    staging_d1 = object_value(
        array_value(object_value(resolver_env["staging"], "resolver staging")["d1_databases"], "staging D1")[0],
        "staging D1",
    )
    production_d1 = object_value(
        array_value(object_value(resolver_env["production"], "resolver production")["d1_databases"], "production D1")[0],
        "production D1",
    )
    production_d1["database_id"] = staging_d1["database_id"]
    try:
        validate_resolver_source_isolation(shared_database)
    except RuntimeTopologyError:
        return
    fail("shared resolver D1 identity negative fixture unexpectedly passed")


def main() -> int:
    if core.main() != 0:
        return 1
    try:
        control = load(CONTROL_CONFIG, "control-plane Wrangler config")
        resolver = load(RESOLVER_CONFIG, "resolver Wrangler config")
        validate_core_queue_closure(control)
        validate_generation_verification_runtime(control)
        validate_resolver_source_isolation(resolver)
        run_checker(AR8D_CHECKER)
        run_checker(AR11_CHECKER)
        self_test(control, resolver)
        print(
            "Cloudflare provider-native Core topology, generation-verification retirement, resolver isolation, "
            "historical D3 evidence, and AR-11 successor checks passed."
        )
        return 0
    except (OSError, json.JSONDecodeError, RuntimeTopologyError, KeyError) as error:
        print(f"Cloudflare runtime topology error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
