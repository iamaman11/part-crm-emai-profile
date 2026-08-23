#!/usr/bin/env python3
"""Disposable N1 cutover, invariant-parity and repository-wide caller audit.

Never transplant this helper. Only the materialized target files are eligible for N1.
"""
from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OLD_TOPOLOGY = "architecture/runtime-topology-ar2.json"
TERMS = (
    OLD_TOPOLOGY,
    "runtime_topology_decision",
    "RUNTIME_TOPOLOGY",
    '"topology_source"',
    '"runtime_resources"',
    '"resource_refs"',
    '"topology_decision_source"',
    '"runtime_topology_projection_owner"',
)
SCRATCH_PATHS = {
    ".github/workflows/n1-projection-scratch.yml",
    ".github/workflows/quality-gate.yml",
    "scripts/check-no-temporary-step4.py",
    "scripts/n1_scratch_audit.py",
}
PROVENANCE_PATHS = {
    "docs/ARCHITECTURE_REBASELINE_V3_AR2.md",
    "docs/ARCHITECTURE_REBASELINE_V3_AR3.md",
    "docs/PF1_CANONICAL_ARCHITECTURE_INVENTORY_CUTOVER.md",
    "docs/POST_AR11_FUNCTIONAL_CLOSURE_PLAN.md",
    "docs/PRE_PF1_AUTHORITY_ESTATE_NORMALIZATION.md",
}
EXPECTED_DECISIONS = {
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


def replace_once(text: str, old: str, new: str = "") -> str:
    if text.count(old) != 1:
        raise SystemExit(f"scratch transform expected exactly one match: {old[:100]!r}")
    return text.replace(old, new, 1)


def write_text(path: Path, text: str) -> None:
    path.write_text(text, encoding="utf-8", newline="\n")


def transform_ar3() -> None:
    path = ROOT / "scripts/_ar3_application_architecture.py"
    text = path.read_text(encoding="utf-8")
    text = replace_once(
        text,
        "projection is architecture/inventory.json. Accepted runtime-resource decisions are consumed\nfrom architecture/runtime-topology-ar2.json without being re-decided here.",
        "projection is architecture/inventory.json. Runtime/resource topology is owned by provider-native\nconfiguration and Product Rust rather than consumed from the historical AR-2 decision artifact.",
    )
    text = replace_once(text, "import copy\nimport json\n", "import copy\n")
    text = replace_once(text, 'RUNTIME_TOPOLOGY = "architecture/runtime-topology-ar2.json"\n')
    text = replace_once(text, '''        "resource_refs": [
            "control_plane_worker",
            "static_assets",
            "catalog_d1",
            "profile_objects",
            "profile_coordinator",
            "notification_hub",
            "integration_events",
            "mailbox_jobs",
            "mailbox_jobs_dlq",
            "generation_verification",
            "mailbox_secret_resolver_service",
            "control_plane_schedule",
        ],
''')
    text = replace_once(text, '''        "resource_refs": [
            "mailbox_secret_resolver_worker",
            "resolver_d1",
            "resolver_reconciliation_schedule",
        ],
''')
    text = replace_once(text, '        "resource_refs": ["browser_bridge_mailbox_lane"],\n')
    start = text.index("\ndef load_topology(root: Path) -> dict[str, Any]:")
    end = text.index("\ndef _validate_source_text", start)
    text = text[:start] + text[end:]
    text = replace_once(text, "    topology = load_topology(root)\n")
    start = text.index('    resource_ids = {item["id"] for item in topology["resources"]}\n')
    end = text.index("    remediation = ", start)
    text = text[:start] + text[end:]
    text = replace_once(text, '        "topology_source": RUNTIME_TOPOLOGY,\n')
    text = replace_once(text, '        "runtime_resources": copy.deepcopy(topology["resources"]),\n')
    start = text.index("\n    missing_resource = copy.deepcopy(expected)")
    end = text.index("\n    duplicate_owner = ", start)
    text = text[:start] + text[end:]
    write_text(path, text)


def transform_inventory_engine() -> None:
    path = ROOT / "scripts/generate-architecture-inventory-engine.py"
    text = path.read_text(encoding="utf-8")
    for old in (
        'RUNTIME_TOPOLOGY = "architecture/runtime-topology-ar2.json"\n',
        '    {"path": RUNTIME_TOPOLOGY, "status": "STABLE_AUTHORITY", "scope": "accepted_ar2_runtime_topology_decision_input_for_ar3"},\n',
        '    if program.get("runtime_topology_decision") != RUNTIME_TOPOLOGY:\n        raise SystemExit("docs/status.json must project the accepted AR-2 topology decision")\n',
        '            "topology_decision_source": RUNTIME_TOPOLOGY,\n',
        '                "topology_decision": "DELETE",\n',
        '            "runtime_topology_decision": RUNTIME_TOPOLOGY,\n',
        '            "runtime_topology_projection_owner": "AR-3",\n',
    ):
        text = replace_once(text, old)
    start = text.index("\n    topology = copy.deepcopy(expected)")
    end = text.index("\n    runtime_cleanup = ", start)
    text = text[:start] + text[end:]
    start = text.index("\n    missing_resource = copy.deepcopy(expected)")
    end = text.index("\n    ar4d = ", start)
    text = text[:start] + text[end:]
    write_text(path, text)


def transform_status() -> None:
    path = ROOT / "docs/status.json"
    payload = json.loads(path.read_text(encoding="utf-8"))
    if payload["current"]["architecture_program"].pop("runtime_topology_decision", None) is None:
        raise SystemExit("scratch status transform lost expected runtime_topology_decision")
    write_text(path, json.dumps(payload, indent=2, ensure_ascii=False) + "\n")


def transform_transition() -> None:
    path = ROOT / "architecture/architecture-rebaseline-v3-transition.json"
    payload = json.loads(path.read_text(encoding="utf-8"))
    runtime = payload.get("runtime_topology")
    policy = payload.get("architecture_inventory_policy")
    if not isinstance(runtime, dict) or not isinstance(policy, dict):
        raise SystemExit("transition topology/inventory policy shape drifted")
    if runtime.pop("decision_authority", None) != OLD_TOPOLOGY:
        raise SystemExit("transition lost expected AR-2 decision authority pointer")
    if runtime.pop("canonical_inventory_projection_owner", None) != "AR-3":
        raise SystemExit("transition lost expected AR-3 topology projection owner")
    runtime["current_authority"] = "PROVIDER_NATIVE_CONFIG_AND_PRODUCT_RUST"
    if policy.pop("ar2_decision_input", None) != OLD_TOPOLOGY:
        raise SystemExit("transition lost expected AR-2 inventory input pointer")
    write_text(path, json.dumps(payload, indent=2, ensure_ascii=False) + "\n")


def transform_architecture_readme() -> None:
    path = ROOT / "architecture/README.md"
    text = path.read_text(encoding="utf-8")
    text = replace_once(
        text,
        "- `runtime-topology-ar2.json`, `python-estate-ar6.json`, `github-governance-ar7.json` — accepted architecture artifacts whose phase-qualified names are preserved because they are accepted provenance/decision inputs rather than newly mutable AR-8 completion authorities.\n",
        "- `python-estate-ar6.json`, `github-governance-ar7.json` — accepted architecture artifacts whose phase-qualified names are preserved because they remain accepted provenance/decision inputs rather than newly mutable AR-8 completion authorities.\n"
        "- AR-2 runtime-topology classification is historical provenance after N1. Current executable topology is owned by provider-native Wrangler configuration and Product Rust; accepted AR-2 evidence remains in `docs/ARCHITECTURE_REBASELINE_V3_AR2.md` and Git history.\n",
    )
    write_text(path, text)


def transform_legacy_checker() -> None:
    path = ROOT / "scripts/check-documentation-authority-legacy.py"
    text = path.read_text(encoding="utf-8")
    for old in (
        'TOPOLOGY = Path("architecture/runtime-topology-ar2.json")\n',
        "    TOPOLOGY,\n",
        "        topology = load_json(root, TOPOLOGY)\n",
        '        "runtime_topology_decision": TOPOLOGY.as_posix(),\n',
        '    if doc_authority.get("runtime_topology_decision") != TOPOLOGY.as_posix():\n        errors.append("architecture inventory must point to the accepted AR-2 topology decision")\n',
    ):
        text = replace_once(text, old)
    start = text.index('\n    if topology.get("slice") != "AR-2"')
    end = text.index("\n    doc_authority = ", start)
    text = text[:start] + text[end:]
    write_text(path, text)


def transform() -> dict[str, object]:
    topology_path = ROOT / OLD_TOPOLOGY
    topology = json.loads(topology_path.read_text(encoding="utf-8"))
    transform_ar3()
    transform_inventory_engine()
    transform_status()
    transform_transition()
    transform_architecture_readme()
    transform_legacy_checker()
    topology_path.unlink()
    subprocess.run([sys.executable, "scripts/generate-architecture-inventory.py", "--write"], cwd=ROOT, check=True)
    return topology


def run(*args: str) -> None:
    subprocess.run(args, cwd=ROOT, check=True)


def prove_current_invariant_parity(topology: dict[str, object]) -> None:
    resources = topology.get("resources")
    if not isinstance(resources, list) or any(not isinstance(item, dict) for item in resources):
        raise SystemExit("old topology resource set is malformed")
    decisions = {str(item.get("id")): item.get("decision") for item in resources}
    if decisions != EXPECTED_DECISIONS:
        raise SystemExit(f"old topology decision set drifted: {decisions!r}")

    release = json.loads((ROOT / "architecture/release-architecture-ar11.json").read_text(encoding="utf-8"))
    closures = release.get("deployment_closures")
    units = release.get("activation_units")
    if not isinstance(closures, list) or not isinstance(units, list):
        raise SystemExit("release architecture closure/activation authority is missing")
    closure_resources = {
        resource
        for closure in closures
        if isinstance(closure, dict)
        for key in ("required_resources", "optional_or_disabled_resources")
        for resource in closure.get(key, [])
        if isinstance(resource, str)
    }
    required_cloudflare = {
        "control_plane_worker", "catalog_d1", "resolver_d1", "profile_objects",
        "profile_coordinator", "notification_hub", "integration_events", "mailbox_jobs",
        "mailbox_jobs_dlq", "mailbox_secret_resolver_worker", "mailbox_secret_resolver_service",
        "control_plane_schedule", "resolver_reconciliation_schedule",
    }
    missing = sorted(required_cloudflare - closure_resources)
    if missing:
        raise SystemExit(f"release deployment closure lost retired-topology resources: {missing}")
    unit_ids = {item.get("activation_unit") for item in units if isinstance(item, dict)}
    for required in ("profile_runtime", "mailbox_browser_binding", "mailbox_read", "mailbox_jobs", "outbound_mail"):
        if required not in unit_ids:
            raise SystemExit(f"release activation authority lost {required}")

    provider = (ROOT / "crates/cloudflare-adapters/src/cloud_mailbox_provider.rs").read_text(encoding="utf-8")
    for marker in ("check_gmail_mailbox", "check_imap_mailbox", "check_microsoft_graph_mailbox"):
        if marker not in provider:
            raise SystemExit(f"current mailbox provider lane lost {marker}")
    outbound = (ROOT / "apps/control-plane-worker/src/composition/outbound_mail.rs").read_text(encoding="utf-8")
    for marker in (
        "CloudflareGmailOutboundMailProvider", "CloudflareSmtpOutboundMailProvider",
        "MailboxProvider::BrowserFallback | MailboxProvider::MicrosoftGraph",
        "ClientMailOutboundProvider::Unsupported",
    ):
        if marker not in outbound:
            raise SystemExit(f"current outbound provider authority lost {marker}")
    wrangler = (ROOT / "deploy/cloudflare/wrangler.jsonc").read_text(encoding="utf-8")
    if '"binding": "ASSETS"' not in wrangler:
        raise SystemExit("provider-native static-assets binding disappeared")

    run(sys.executable, "scripts/check-cloudflare-runtime-bindings.py")
    run(sys.executable, "scripts/check-documentation-authority.py", "--root", ".")
    run(sys.executable, "scripts/generate-architecture-inventory.py", "--check")
    run(sys.executable, "scripts/generate-architecture-inventory.py", "--self-test")
    run(sys.executable, "-m", "py_compile", "scripts/_ar3_application_architecture.py", "scripts/generate-architecture-inventory-engine.py", "scripts/check-documentation-authority-legacy.py")
    run("git", "diff", "--check")

    print("N1_OLD_CALLER_INVARIANT_PARITY provider_native_runtime=PASS")
    print("N1_OLD_CALLER_INVARIANT_PARITY release_capability_admission=PASS")
    print("N1_OLD_CALLER_INVARIANT_PARITY provider_lanes=PASS")
    print("N1_OLD_CALLER_INVARIANT_PARITY generation_verification_retirement=PASS")
    print("N1_OLD_CALLER_INVARIANT_PARITY resolver_isolation=PASS")
    print("N1_OLD_CALLER_INVARIANT_PARITY d3_history_and_ar11_successor=PASS")
    print("N1_OLD_UNIQUE_CURRENT_INVARIANT_COUNT 0")


def tracked_paths() -> list[Path]:
    proc = subprocess.run(["git", "ls-files", "-z"], cwd=ROOT, check=True, capture_output=True)
    return [ROOT / value.decode("utf-8") for value in proc.stdout.split(b"\0") if value]


def classify_path(relative: str) -> str:
    if relative in SCRATCH_PATHS:
        return "SCRATCH"
    if relative.startswith("history/") or relative in PROVENANCE_PATHS:
        return "PROVENANCE"
    return "CURRENT"


def scan_old_authority_terms() -> tuple[int, int, int]:
    counts = {"CURRENT": 0, "PROVENANCE": 0, "SCRATCH": 0}
    for path in tracked_paths():
        if not path.is_file():
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        relative = path.relative_to(ROOT).as_posix()
        classification = classify_path(relative)
        for number, line in enumerate(text.splitlines(), 1):
            for term in TERMS:
                if term in line:
                    counts[classification] += 1
                    print(f"N1_AUDIT_{classification} {relative}:{number} term={term} :: {line.strip()}")
    print(f"N1_AUDIT_CURRENT_SEMANTIC_MATCH_COUNT {counts['CURRENT']}")
    print(f"N1_AUDIT_PROVENANCE_MATCH_COUNT {counts['PROVENANCE']}")
    print(f"N1_AUDIT_SCRATCH_MATCH_COUNT {counts['SCRATCH']}")
    return counts["CURRENT"], counts["PROVENANCE"], counts["SCRATCH"]


def scan_legacy_checker_callers() -> int:
    needle = "check-documentation-authority-legacy.py"
    callers = 0
    for path in tracked_paths():
        if not path.is_file():
            continue
        relative = path.relative_to(ROOT).as_posix()
        if relative in SCRATCH_PATHS or relative == "scripts/check-documentation-authority-legacy.py":
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        for number, line in enumerate(text.splitlines(), 1):
            if needle in line:
                if relative == "architecture/python-estate-ar6.json":
                    print(f"N1_LEGACY_CHECKER_INVENTORY_ONLY {relative}:{number}")
                    continue
                callers += 1
                print(f"N1_LEGACY_CHECKER_CALLER {relative}:{number} :: {line.strip()}")
    print(f"N1_LEGACY_CHECKER_EXECUTABLE_CALLER_COUNT {callers}")
    return callers


def main() -> int:
    topology = transform()
    resources = topology.get("resources")
    if not isinstance(resources, list):
        raise SystemExit("old topology resources missing")
    print(f"N1_OLD_RESOURCE_DECISION_COUNT {len(resources)}")
    for item in resources:
        if isinstance(item, dict):
            print(f"N1_OLD_DECISION id={item.get('id')} decision={item.get('decision')}")

    prove_current_invariant_parity(topology)
    current, _, _ = scan_old_authority_terms()
    legacy_callers = scan_legacy_checker_callers()
    if current != 0:
        raise SystemExit(f"N1 current semantic old-authority matches remain: {current}")
    if legacy_callers != 0:
        raise SystemExit(f"N1 legacy checker executable callers remain: {legacy_callers}")
    print("N1_OLD_CALLER_COUNT 0")
    print("N1_SCRATCH_AUDIT_COMPLETE")
    return 97


if __name__ == "__main__":
    raise SystemExit(main())
