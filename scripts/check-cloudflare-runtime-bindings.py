#!/usr/bin/env python3
"""Canonical Cloudflare runtime-binding and accepted-topology fitness gate.

The shared validator in `_cloudflare_runtime_bindings_core.py` proves the current
source-to-Wrangler binding inventory. This entrypoint preserves the accepted
AR-2 topology, resolver-isolation and D3 compatibility invariants and applies
AR-5's accepted deletion authority for the legacy GENERATION_VERIFICATION Queue.
When AR-8D introduces the governed secret-transport successor, the accepted D3
promotion ceremony remains checked against its immutable predecessor workflow
while the current workflow is checked by the AR-8D successor authority.
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
PROMOTION_WORKFLOW = ROOT / ".github/workflows/mailbox-secret-resolver-promotion.yml"
PROMOTION_ENTRYPOINT = ROOT / "scripts/mailbox-secret-resolver-promotion.py"
PROMOTION_CORE = ROOT / "scripts/_mailbox_secret_resolver_promotion_core.py"
AR8D_SECRET_TRANSPORT_SUCCESSOR = ROOT / "architecture/ar8-d-secret-transport-successor.json"
AR8D_SECRET_TRANSPORT_CHECKER = ROOT / ".github/scripts/ar8-d-secret-transport-successor.mjs"
QUEUE_ENTRYPOINT = ROOT / "apps/control-plane-worker/src/lib.rs"
QUEUE_ENVELOPE = ROOT / "crates/cloudflare-adapters/src/control_plane_queue.rs"
CONTROL_PLANE_CONTRACT = ROOT / "crates/control-plane-contract/src/lib.rs"
GENERATION_ROUTE = ROOT / "apps/control-plane-worker/src/profile_generations.rs"

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


class Ar2TopologyError(ValueError):
    pass


def fail(message: str) -> None:
    raise Ar2TopologyError(message)


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


def consumer_queues(control: dict[str, Any], environment: str) -> set[str]:
    envs = object_value(control.get("env"), "control env")
    selected = object_value(envs.get(environment), f"control env.{environment}")
    queues = object_value(selected.get("queues"), f"control env.{environment}.queues")
    consumers = array_value(
        queues.get("consumers"), f"control env.{environment}.queues.consumers"
    )
    observed: set[str] = set()
    for item in consumers:
        entry = object_value(item, "queue consumer")
        queue = entry.get("queue")
        if not isinstance(queue, str) or not queue:
            fail(f"control env.{environment} consumer lacks queue")
        if queue in observed:
            fail(f"control env.{environment} duplicates queue consumer {queue}")
        observed.add(queue)
    return observed


def producer_bindings(control: dict[str, Any], environment: str) -> set[str]:
    envs = object_value(control.get("env"), "control env")
    selected = object_value(envs.get(environment), f"control env.{environment}")
    queues = object_value(selected.get("queues"), f"control env.{environment}.queues")
    producers = array_value(
        queues.get("producers"), f"control env.{environment}.queues.producers"
    )
    observed: set[str] = set()
    for item in producers:
        entry = object_value(item, "queue producer")
        binding = entry.get("binding")
        if not isinstance(binding, str) or not binding:
            fail(f"control env.{environment} producer lacks binding")
        if binding in observed:
            fail(f"control env.{environment} duplicates Queue producer binding {binding}")
        observed.add(binding)
    return observed


def validate_queue_consumers(control: dict[str, Any]) -> None:
    expected = {
        "staging": {
            "${STAGING_INTEGRATION_EVENTS_QUEUE}",
            "${STAGING_MAILBOX_JOBS_QUEUE}",
        },
        "production": {
            "${PRODUCTION_INTEGRATION_EVENTS_QUEUE}",
            "${PRODUCTION_MAILBOX_JOBS_QUEUE}",
        },
    }
    for environment, wanted in expected.items():
        observed = consumer_queues(control, environment)
        if observed != wanted:
            fail(
                f"{environment} Queue consumers must be exactly integration-events + mailbox-jobs; "
                f"observed={sorted(observed)!r}"
            )


def validate_queue_producers(control: dict[str, Any]) -> None:
    wanted = {"INTEGRATION_EVENTS", "MAILBOX_JOBS"}
    for environment in ("staging", "production"):
        observed = producer_bindings(control, environment)
        if observed != wanted:
            fail(
                f"{environment} Queue producer bindings must be exactly integration-events + mailbox-jobs; "
                f"observed={sorted(observed)!r}"
            )


def validate_generation_verification_runtime(control: dict[str, Any]) -> None:
    envelope = QUEUE_ENVELOPE.read_text(encoding="utf-8")
    entrypoint = QUEUE_ENTRYPOINT.read_text(encoding="utf-8")
    contract = CONTROL_PLANE_CONTRACT.read_text(encoding="utf-8")
    route = GENERATION_ROUTE.read_text(encoding="utf-8")

    for environment in ("staging", "production"):
        if "GENERATION_VERIFICATION" in producer_bindings(control, environment):
            fail(f"{environment} unexpectedly restored GENERATION_VERIFICATION producer binding")

    if "VERIFICATION_QUEUE_BINDING" in contract or "GENERATION_VERIFICATION" in contract:
        fail("legacy generation-verification Queue contract authority still exists")
    if "VERIFICATION_QUEUE_BINDING" in entrypoint or 'env.queue("GENERATION_VERIFICATION")' in entrypoint:
        fail("control-plane runtime still probes the deleted generation-verification Queue")

    enum_match = re.search(
        r"pub enum ControlPlaneQueueMessage\s*\{(?P<body>.*?)\n\}", envelope, re.S
    )
    if enum_match is None:
        fail("control-plane Queue envelope enum is missing")
    variants = re.findall(
        r"^\s*([A-Z][A-Za-z0-9_]*)\(", enum_match.group("body"), re.M
    )
    if variants != ["IntegrationEvent", "MailboxJob"]:
        fail(f"control-plane Queue envelope drifted: {variants}")

    parts = entrypoint.split("pub async fn control_plane_queue", 1)
    if len(parts) != 2:
        fail("control-plane Queue entrypoint is missing")
    handler = parts[1].split("#[event(scheduled)]", 1)[0]
    for marker in (
        "ControlPlaneQueueMessage::IntegrationEvent",
        "ControlPlaneQueueMessage::MailboxJob",
    ):
        if marker not in handler:
            fail(f"control-plane Queue handler lost {marker}")
    if "GENERATION_VERIFICATION" in handler or "GenerationVerification" in envelope or "GenerationVerification" in handler:
        fail("generation verification unexpectedly gained an async Queue workload")

    if (
        "RouteClass::ProfileGenerationVerifyApi" not in route
        or "execute_verify_generation(" not in route
    ):
        fail("synchronous profile-generation verification authority disappeared")
    if "VERIFICATION_QUEUE_BINDING" in route or "GENERATION_VERIFICATION" in route:
        fail("profile-generation verification must not depend on the deleted Queue binding")


def validate_resolver_isolation(resolver: dict[str, Any]) -> None:
    if resolver.get("name") != "mailbox-secret-resolver" or resolver.get("workers_dev") is not False:
        fail("mailbox secret resolver must remain the private canonical Worker")
    if resolver.get("triggers") != {"crons": ["17 * * * *"]}:
        fail("resolver key-reconciliation schedule drifted")
    envs = object_value(resolver.get("env"), "resolver env")
    if set(envs) != {"staging", "production"}:
        fail("resolver must define exactly staging and production")
    identities: set[str] = set()
    for environment in ("staging", "production"):
        selected = object_value(envs.get(environment), f"resolver env.{environment}")
        if selected.get("workers_dev") is not False or selected.get("routes") != []:
            fail(f"resolver {environment} must remain private/no-route")
        databases = array_value(
            selected.get("d1_databases"), f"resolver env.{environment}.d1_databases"
        )
        if len(databases) != 1:
            fail(f"resolver {environment} must own exactly one dedicated D1")
        database = object_value(databases[0], "resolver D1")
        if (
            database.get("binding") != "RESOLVER_DB"
            or database.get("migrations_dir") != "../../migrations/resolver-d1"
        ):
            fail(f"resolver {environment} dedicated D1 boundary drifted")
        identity = database.get("database_id")
        if not isinstance(identity, str):
            fail(f"resolver {environment} D1 identity is missing")
        identities.add(identity)
    if len(identities) != 2:
        fail("resolver staging and production D1 identities must remain isolated")


def validate_topology(topology: dict[str, Any]) -> None:
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
            fail(f"AR-2 topology {key} must be {expected!r}")

    policies = object_value(topology.get("policies"), "AR-2 policies")
    for key in (
        "resource_decisions_are_architecture_input_not_provider_mutation",
        "wrangler_source_binding_changes_deferred_to_ar5",
        "runtime_simplification_execution_deferred_to_ar10_when_needed",
        "production_resource_creation_update_delete_forbidden_during_ar",
        "new_parallel_runtime_registry_forbidden",
    ):
        if policies.get(key) is not True:
            fail(f"AR-2 policy {key} must remain true")
    if (
        policies.get("production_promotion_authority")
        != "PC-1_AFTER_AR-17_USING_AR-11_RELEASE_SET"
    ):
        fail("production promotion authority drifted")

    generation = object_value(
        topology.get("generation_verification"), "generation verification decision"
    )
    if (
        generation.get("binding") != "GENERATION_VERIFICATION"
        or generation.get("decision") != "DELETE"
        or generation.get("source_binding_removal_slice") != "AR-5"
        or generation.get("production_resource_policy")
        != "DO_NOT_PROVISION_IN_PC-1; NO_AR_PRODUCTION_MUTATION"
    ):
        fail("GENERATION_VERIFICATION AR-2 decision drifted")

    d3 = object_value(topology.get("d3_compatibility"), "D3 compatibility")
    expected_d3 = {
        "predecessor_issue": 251,
        "state_verified_at_ar2_start": "open",
        "repository_side_promotion_machinery": "KEEP",
        "resolver_isolation": "KEEP",
        "staging_same_bits_lane": "KEEP_PREPRODUCTION_FOUNDATION",
        "legacy_d3_production_lane": "DISABLE_FORWARD_EXECUTION",
        "generalize_release_semantics_in": "AR-11",
        "issue_disposition_after_ar2_acceptance": (
            "CLOSE_NOT_PLANNED_SUPERSEDED_FORWARD_SEQUENCE_PRESERVE_HISTORY"
        ),
    }
    for key, expected in expected_d3.items():
        if d3.get(key) != expected:
            fail(f"D3 compatibility {key} must be {expected!r}")

    observed: dict[str, str] = {}
    for item in array_value(topology.get("resources"), "AR-2 resources"):
        row = object_value(item, "AR-2 resource")
        resource_id = row.get("id")
        decision = row.get("decision")
        evidence = row.get("evidence")
        if (
            not isinstance(resource_id, str)
            or not isinstance(decision, str)
            or not isinstance(evidence, list)
            or not evidence
        ):
            fail("AR-2 resource row requires id, decision, and repository evidence")
        if resource_id in observed:
            fail(f"duplicate AR-2 resource {resource_id}")
        observed[resource_id] = decision
        for relative in evidence:
            if not isinstance(relative, str) or not (ROOT / relative).exists():
                fail(f"AR-2 resource {resource_id} references missing evidence {relative!r}")
    if observed != EXPECTED_RESOURCE_DECISIONS:
        fail("AR-2 runtime resource decision inventory drifted")


def validate_d3_gate(workflow: str) -> None:
    wrapper = PROMOTION_ENTRYPOINT.read_text(encoding="utf-8")
    if not PROMOTION_CORE.is_file():
        fail("accepted pre-AR-2 D3 promotion core is not preserved")
    for marker in (
        "AR2_LEGACY_PRODUCTION_DISABLED = True",
        'AR2_PRODUCTION_AUTHORITY = "PC-1_AFTER_AR-17_USING_AR-11_RELEASE_SET"',
        "enforce_ar2_environment_gate",
        'environment == "production"',
        "import _mailbox_secret_resolver_promotion_core as core",
    ):
        if marker not in wrapper:
            fail(f"canonical D3 compatibility entrypoint lost {marker!r}")

    for marker in (
        "github-preflight",
        "needs: [preflight]",
        "validate-release-identities",
        "verify-remote-d1",
        "deploy --dry-run",
        "--experimental-autoconfig=false",
        "validate-secrets",
        "deployments status",
        "attest",
    ):
        if marker not in workflow:
            fail(f"accepted D3 preproduction foundation lost {marker!r}")

    preflight = workflow.find("  preflight:")
    gate = workflow.find(
        "python scripts/mailbox-secret-resolver-promotion.py github-preflight"
    )
    mutation = workflow.find("  promote-same-bits:")
    if not (0 <= preflight < gate < mutation):
        fail("canonical AR-2 D3 preflight does not precede the mutation job")
    if "needs: [preflight]" not in workflow[mutation:]:
        fail("D3 mutation job can bypass the canonical AR-2 preflight")


def governed_d3_workflow(current_workflow: str) -> str:
    if not AR8D_SECRET_TRANSPORT_SUCCESSOR.is_file():
        return current_workflow
    if not AR8D_SECRET_TRANSPORT_CHECKER.is_file():
        fail("AR-8D secret-transport successor authority exists without its permanent checker")

    successor_check = subprocess.run(
        [
            "node",
            "--input-type=module",
            "--eval",
            "await import('./.github/scripts/ar8-d-secret-transport-successor.mjs')",
        ],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if successor_check.returncode != 0:
        details = "\n".join(
            value.strip()
            for value in (successor_check.stdout, successor_check.stderr)
            if value.strip()
        )
        fail(f"AR-8D secret-transport successor gate failed:\n{details}")

    authority = load(AR8D_SECRET_TRANSPORT_SUCCESSOR, "AR-8D secret-transport successor")
    predecessor = object_value(authority.get("predecessor"), "AR-8D predecessor")
    transition_base = predecessor.get("transition_base_main")
    workflow_path = predecessor.get("promotion_workflow")
    expected_path = PROMOTION_WORKFLOW.relative_to(ROOT).as_posix()
    if (
        not isinstance(transition_base, str)
        or re.fullmatch(r"[0-9a-f]{40}", transition_base) is None
        or workflow_path != expected_path
    ):
        fail("AR-8D successor does not bind an exact historical D3 promotion workflow")

    historical = subprocess.run(
        ["git", "show", f"{transition_base}:{workflow_path}"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if historical.returncode != 0 or not historical.stdout:
        details = historical.stderr.strip()
        fail(
            "accepted D3 predecessor workflow is unavailable at the governed AR-8D transition base"
            + (f": {details}" if details else "")
        )
    return historical.stdout


def self_test(control: dict[str, Any], topology: dict[str, Any], workflow: str) -> None:
    restored_producer = copy.deepcopy(control)
    production = object_value(
        object_value(restored_producer["env"], "env")["production"], "production"
    )
    queues = object_value(production["queues"], "queues")
    array_value(queues["producers"], "producers").append(
        {
            "binding": "GENERATION_VERIFICATION",
            "queue": "${PRODUCTION_GENERATION_VERIFICATION_QUEUE}",
        }
    )
    try:
        validate_queue_producers(restored_producer)
    except Ar2TopologyError:
        pass
    else:
        fail("GENERATION_VERIFICATION producer restoration negative fixture unexpectedly passed")

    restored_consumer = copy.deepcopy(control)
    production = object_value(
        object_value(restored_consumer["env"], "env")["production"], "production"
    )
    queues = object_value(production["queues"], "queues")
    array_value(queues["consumers"], "consumers").append(
        {
            "queue": "${PRODUCTION_GENERATION_VERIFICATION_QUEUE}",
            "max_batch_size": 10,
            "max_batch_timeout": 5,
        }
    )
    try:
        validate_queue_consumers(restored_consumer)
    except Ar2TopologyError:
        pass
    else:
        fail("GENERATION_VERIFICATION consumer negative fixture unexpectedly passed")

    drifted_topology = copy.deepcopy(topology)
    for row in drifted_topology["resources"]:
        if row["id"] == "generation_verification":
            row["decision"] = "KEEP"
            break
    try:
        validate_topology(drifted_topology)
    except Ar2TopologyError:
        pass
    else:
        fail("GENERATION_VERIFICATION decision negative fixture unexpectedly passed")

    candidate_status = copy.deepcopy(topology)
    candidate_status["status"] = "AR2_ACCEPTED_CANDIDATE"
    try:
        validate_topology(candidate_status)
    except Ar2TopologyError:
        pass
    else:
        fail("post-merge AR-2 candidate-status negative fixture unexpectedly passed")

    bypass = workflow.replace("needs: [preflight]", "needs: []", 1)
    try:
        validate_d3_gate(bypass)
    except Ar2TopologyError:
        pass
    else:
        fail("D3 preflight-bypass negative fixture unexpectedly passed")


def main() -> int:
    if core.main() != 0:
        return 1
    try:
        control = load(CONTROL_CONFIG, "control-plane Wrangler config")
        resolver = load(RESOLVER_CONFIG, "resolver Wrangler config")
        topology = load(TOPOLOGY, "AR-2 runtime topology")
        current_workflow = PROMOTION_WORKFLOW.read_text(encoding="utf-8")
        d3_workflow = governed_d3_workflow(current_workflow)
        validate_queue_consumers(control)
        validate_queue_producers(control)
        validate_generation_verification_runtime(control)
        validate_resolver_isolation(resolver)
        validate_topology(topology)
        validate_d3_gate(d3_workflow)
        self_test(control, topology, d3_workflow)
        print(
            "AR-5 runtime authority cleanup, accepted AR-2 topology, resolver isolation, "
            "and governed D3-to-AR-8D production fail-closed compatibility checks passed."
        )
        return 0
    except (OSError, json.JSONDecodeError, Ar2TopologyError, KeyError) as error:
        print(f"Cloudflare runtime topology error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
