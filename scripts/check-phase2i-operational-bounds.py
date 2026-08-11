#!/usr/bin/env python3
"""Validate Phase 2I metadata-safe operational indicators and source-backed capacity bounds."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

POLICY = Path("tests/operations/phase2i-operational-bounds.json")
ALLOWED_KINDS = {"counter", "gauge", "histogram"}
ALLOWED_LABELS = {
    "route_class",
    "method_class",
    "result_class",
    "dependency_class",
    "operation_class",
    "status_class",
    "reason_class",
    "resource_class",
    "provider_class",
    "runtime_class",
}
FORBIDDEN_HIGH_CARDINALITY = re.compile(
    r"(?:tenant|actor|identity|client|profile|mailbox|message|device|job|email|subject|sender|recipient|body|content|credential|token|cookie|secret|correlation|access_subject).*(?:id|value|text|address)?$",
    re.IGNORECASE,
)
REQUIRED_SLO_INVARIANTS = {
    "sensitive_payload_technical_channel_violations",
    "authorization_after_projection_violations",
    "stale_fence_acceptance_violations",
    "unverified_generation_activation_violations",
    "recovery_integrity_violations",
}
REQUIRED_CAPACITY_BOUNDS = {
    "queryPageMax",
    "claimableDeviceJobsMax",
    "realtimeAudiencePageMax",
    "mailboxJobAttemptsMax",
    "mailboxRetryDelayMaxMs",
}
REQUIRED_EXTERNAL_CALIBRATION = {
    "production_latency_percentiles",
    "production_error_budget",
    "provider_rate_limit_envelope",
    "production_cloudflare_cost_baseline",
    "production_device_capacity",
}


def parse_rust_integer_constant(source: str, constant: str) -> int:
    pattern = re.compile(
        rf"\b(?:pub\s+)?const\s+{re.escape(constant)}\s*:\s*[A-Za-z0-9_]+\s*=\s*([0-9][0-9_]*)\s*;"
    )
    match = pattern.search(source)
    if match is None:
        raise ValueError(f"source-backed capacity constant not found: {constant}")
    return int(match.group(1).replace("_", ""))


def validate_indicator(indicator: object, index: int) -> list[str]:
    label = f"indicators[{index}]"
    if not isinstance(indicator, dict):
        return [f"{label} must be an object"]
    if set(indicator) != {"name", "kind", "labels"}:
        return [f"{label} must contain exactly name, kind and labels"]

    errors: list[str] = []
    name = indicator.get("name")
    kind = indicator.get("kind")
    labels = indicator.get("labels")
    if not isinstance(name, str) or not re.fullmatch(r"[a-z][a-z0-9_]{2,79}", name):
        errors.append(f"{label}.name must be a bounded snake_case metric name")
    if kind not in ALLOWED_KINDS:
        errors.append(f"{label}.kind is not allowlisted: {kind}")
    if not isinstance(labels, list) or len(labels) > 4 or len(labels) != len(set(labels)):
        errors.append(f"{label}.labels must be unique and contain at most four labels")
    elif any(item not in ALLOWED_LABELS for item in labels):
        errors.append(f"{label}.labels contain a non-allowlisted dimension: {labels}")
    elif any(FORBIDDEN_HIGH_CARDINALITY.search(item) for item in labels):
        errors.append(f"{label}.labels contain high-cardinality/sensitive dimensions: {labels}")
    return errors


def validate_policy(root: Path, policy: object) -> list[str]:
    if not isinstance(policy, dict):
        return ["Phase 2I operational policy must be an object"]
    errors: list[str] = []
    for key, expected in {
        "schemaVersion": 1,
        "phase": "Phase 2I",
        "scope": "repository-local-release-candidate",
        "productionReady": False,
    }.items():
        if policy.get(key) != expected:
            errors.append(f"operational policy {key} changed unexpectedly")

    indicators = policy.get("indicators")
    if not isinstance(indicators, list) or not indicators:
        errors.append("operational policy must define indicators")
    else:
        names: list[str] = []
        for index, indicator in enumerate(indicators):
            errors.extend(validate_indicator(indicator, index))
            if isinstance(indicator, dict) and isinstance(indicator.get("name"), str):
                names.append(indicator["name"])
        if len(names) != len(set(names)):
            errors.append("operational indicator names must be unique")

    invariants = policy.get("repositoryLocalSloInvariants")
    if not isinstance(invariants, list):
        errors.append("repositoryLocalSloInvariants must be a list")
    else:
        found: set[str] = set()
        for item in invariants:
            if not isinstance(item, dict) or set(item) != {"name", "objective"}:
                errors.append("every repository-local SLO invariant must contain exactly name/objective")
                continue
            if item.get("objective") != "zero":
                errors.append(f"repository-local invariant is not zero-tolerance: {item.get('name')}")
            name = item.get("name")
            if isinstance(name, str):
                found.add(name)
        if found != REQUIRED_SLO_INVARIANTS:
            errors.append(
                f"repository-local SLO invariant set changed: expected={sorted(REQUIRED_SLO_INVARIANTS)} actual={sorted(found)}"
            )

    bounds = policy.get("capacityBounds")
    if not isinstance(bounds, dict) or set(bounds) != REQUIRED_CAPACITY_BOUNDS:
        actual = sorted(bounds) if isinstance(bounds, dict) else []
        errors.append(
            f"capacity bound set changed: expected={sorted(REQUIRED_CAPACITY_BOUNDS)} actual={actual}"
        )
    else:
        for name, spec in bounds.items():
            if not isinstance(spec, dict) or set(spec) != {"source", "constant", "value"}:
                errors.append(f"capacityBounds.{name} must contain exactly source/constant/value")
                continue
            source = spec.get("source")
            constant = spec.get("constant")
            value = spec.get("value")
            if not isinstance(source, str) or not isinstance(constant, str) or not isinstance(value, int):
                errors.append(f"capacityBounds.{name} fields have invalid types")
                continue
            path = Path(source)
            if path.is_absolute() or ".." in path.parts or not (root / path).is_file():
                errors.append(f"capacityBounds.{name} source is missing or unsafe: {source}")
                continue
            try:
                actual = parse_rust_integer_constant((root / path).read_text(encoding="utf-8"), constant)
            except ValueError as error:
                errors.append(f"capacityBounds.{name}: {error}")
                continue
            if actual != value:
                errors.append(f"capacityBounds.{name} drifted: policy={value} source={actual}")
            if actual <= 0:
                errors.append(f"capacityBounds.{name} must be positive")

    evidence = policy.get("queryPlanEvidence")
    if not isinstance(evidence, list) or not evidence:
        errors.append("queryPlanEvidence must contain executable repository-local evidence")
    else:
        for relative in evidence:
            if not isinstance(relative, str):
                errors.append("queryPlanEvidence entries must be paths")
                continue
            path = Path(relative)
            if path.is_absolute() or ".." in path.parts or not (root / path).is_file():
                errors.append(f"queryPlanEvidence path is missing or unsafe: {relative}")

    calibration = policy.get("externalCalibrationRequired")
    if not isinstance(calibration, list) or set(calibration) != REQUIRED_EXTERNAL_CALIBRATION:
        errors.append("production/External calibration exclusions changed unexpectedly")
    return errors


def self_test(root: Path, policy: dict[str, object]) -> None:
    fixtures: list[tuple[str, callable]] = [
        (
            "tenant identifier metric label",
            lambda value: value["indicators"][0]["labels"].append("tenant_id"),
        ),
        (
            "capacity bound drift",
            lambda value: value["capacityBounds"]["queryPageMax"].__setitem__("value", 999),
        ),
        (
            "non-zero correctness SLO",
            lambda value: value["repositoryLocalSloInvariants"][0].__setitem__("objective", "best_effort"),
        ),
        (
            "production readiness promotion",
            lambda value: value.__setitem__("productionReady", True),
        ),
    ]
    for label, mutate in fixtures:
        candidate = json.loads(json.dumps(policy))
        mutate(candidate)
        if not validate_policy(root, candidate):
            raise ValueError(f"operational negative fixture unexpectedly passed: {label}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    path = args.root / POLICY
    try:
        policy = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        print(f"cannot load Phase 2I operational policy: {error}")
        return 1

    errors = validate_policy(args.root, policy)
    if errors:
        for error in errors:
            print(error)
        return 1
    if args.self_test:
        try:
            self_test(args.root, policy)
        except (KeyError, TypeError, ValueError) as error:
            print(error)
            return 1
        print("Phase 2I operational negative fixtures rejected as expected.")
        return 0

    print("Phase 2I metadata-safe operational indicators and capacity bounds passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
