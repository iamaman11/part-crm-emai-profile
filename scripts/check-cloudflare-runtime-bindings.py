#!/usr/bin/env python3
"""Canonical Cloudflare runtime-binding, topology and AR-11 successor fitness gate.

Current runtime proof is profile-aware: source-present Mail resources remain part of the
accepted AR-2 architecture, but the Core Wrangler closure contains only Core bindings.
Historical D3 promotion is proved from immutable Git history by the AR-8D checker; current
promotion authority is the Rust-backed AR-11 Release Set workflow.
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
TOPOLOGY = ROOT / "architecture/runtime-topology-ar2.json"
QUEUE_ENTRYPOINT = ROOT / "apps/control-plane-worker/src/lib.rs"
QUEUE_ENVELOPE = ROOT / "crates/cloudflare-adapters/src/control_plane_queue.rs"
CONTROL_PLANE_CONTRACT = ROOT / "crates/control-plane-contract/src/lib.rs"
GENERATION_ROUTE = ROOT / "apps/control-plane-worker/src/profile_generations.rs"
AR8D_CHECKER = ROOT / ".github/scripts/ar8-d-secret-transport-successor.mjs"
AR11_CHECKER = ROOT / ".github/scripts/release-operational-ar11.mjs"

EXPECTED_RESOURCE_DECISIONS = {
    "control_plane_worker": "KEEP",
    "static_assets": "KEEP",
    "mailbox_secret_resolver_worker": "KEEP",
    "catalog_d1": "KEEP",
    "resolver_d1": "KEEP",
    "profile_objects": "KEEP",
    "profile_coordinator": "KEEP",
    "notification_hub": "KEEP",
    "integration_events": "KEEP",
    "mailbox_jobs": "KEEP",
    "mailbox_jobs_dlq": "KEEP",
    "generation_verification": "DELETE",
    "mailbox_secret_resolver_service": "KEEP",
    "control_plane_schedule": "KEEP",
    "resolver_reconciliation_schedule": "KEEP",
    "gmail_api": "KEEP",
    "imap_read": "KEEP",
    "smtp_send": "KEEP",
    "microsoft_graph_oauth_read_delta": "KEEP",
    "microsoft_graph_mail_send": "DEFER",
    "browser_bridge_mailbox_lane": "KEEP",
}


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


def validate_ar2_topology(topology: dict[str, Any]) -> None:
    exact = {
        "schema_version": 1,
        "status": "ACCEPTED_AR2_DECISION",
        "program": "Architecture Re-baseline v3",
        "tracking_issue": 266,
        "slice": "AR-2",
        "decision_base": "5d4a0d4a653539c6ae2aaff7d0ee38d2ecb79dbf",
        "production_mutation": False,
        "next_slice_after_acceptance": "AR-3",
    }
    for key, expected in exact.items():
        if topology.get(key) != expected:
            fail(f"AR-2 topology {key} drifted")
    policies = object_value(topology.get("policies"), "AR-2 policies")
    for key in (
        "resource_decisions_are_architecture_input_not_provider_mutation",
        "wrangler_source_binding_changes_deferred_to_ar5",
        "runtime_simplification_execution_deferred_to_ar10_when_needed",
        "production_resource_creation_update_delete_forbidden_during_ar",
        "new_parallel_runtime_registry_forbidden",
    ):
        if policies.get(key) is not True:
            fail(f"AR-2 policy {key} drifted")
    if policies.get("production_promotion_authority") != "PC-1_AFTER_AR-17_USING_AR-11_RELEASE_SET":
        fail("AR-2 production promotion authority drifted")
    generation = object_value(topology.get("generation_verification"), "generation verification")
    if (
        generation.get("binding") != "GENERATION_VERIFICATION"
        or generation.get("decision") != "DELETE"
        or generation.get("source_binding_removal_slice") != "AR-5"
    ):
        fail("generation-verification AR-2 decision drifted")
    d3 = object_value(topology.get("d3_compatibility"), "D3 compatibility")
    if (
        d3.get("staging_same_bits_lane") != "KEEP_PREPRODUCTION_FOUNDATION"
        or d3.get("legacy_d3_production_lane") != "DISABLE_FORWARD_EXECUTION"
        or d3.get("generalize_release_semantics_in") != "AR-11"
    ):
        fail("D3 compatibility handoff decision drifted")
    observed: dict[str, str] = {}
    for item in array_value(topology.get("resources"), "AR-2 resources"):
        row = object_value(item, "AR-2 resource")
        resource_id = row.get("id")
        decision = row.get("decision")
        evidence = row.get("evidence")
        if not isinstance(resource_id, str) or not isinstance(decision, str) or not isinstance(evidence, list) or not evidence:
            fail("AR-2 resource row is invalid")
        if resource_id in observed:
            fail(f"duplicate AR-2 resource: {resource_id}")
        observed[resource_id] = decision
        for relative in evidence:
            if not isinstance(relative, str) or not (ROOT / relative).exists():
                fail(f"AR-2 resource {resource_id} references missing evidence {relative!r}")
    if observed != EXPECTED_RESOURCE_DECISIONS:
        fail("AR-2 resource decision inventory drifted")


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


def self_test(control: dict[str, Any], topology: dict[str, Any]) -> None:
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
    drifted = copy.deepcopy(topology)
    drifted["production_mutation"] = True
    try:
        validate_ar2_topology(drifted)
    except RuntimeTopologyError:
        return
    fail("AR-2 production mutation negative fixture unexpectedly passed")


def main() -> int:
    if core.main() != 0:
        return 1
    try:
        control = load(CONTROL_CONFIG, "control-plane Wrangler config")
        resolver = load(RESOLVER_CONFIG, "resolver Wrangler config")
        topology = load(TOPOLOGY, "AR-2 runtime topology")
        validate_core_queue_closure(control)
        validate_generation_verification_runtime(control)
        validate_resolver_source_isolation(resolver)
        validate_ar2_topology(topology)
        run_checker(AR8D_CHECKER)
        run_checker(AR11_CHECKER, "--pre-cutover")
        self_test(control, topology)
        print(
            "Cloudflare profile-aware Core topology, historical D3 evidence, and AR-11 successor checks passed."
        )
        return 0
    except (OSError, json.JSONDecodeError, RuntimeTopologyError, KeyError) as error:
        print(f"Cloudflare runtime topology error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
