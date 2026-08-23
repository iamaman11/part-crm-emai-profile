#!/usr/bin/env python3
"""Disposable N1 repository-wide caller audit. Never transplant this file."""
from __future__ import annotations

import copy
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TERMS = (
    "runtime-topology-ar2.json",
    "runtime_topology_decision",
    "RUNTIME_TOPOLOGY",
    '"topology_source"',
    '"runtime_resources"',
    '"resource_refs"',
    '"topology_decision_source"',
    '"runtime_topology_projection_owner"',
)


def replace_once(text: str, old: str, new: str = "") -> str:
    if text.count(old) != 1:
        raise SystemExit(f"scratch transform expected exactly one match: {old[:80]!r}")
    return text.replace(old, new, 1)


def transform() -> dict[str, object]:
    topology_path = ROOT / "architecture/runtime-topology-ar2.json"
    topology = json.loads(topology_path.read_text(encoding="utf-8"))

    ar3 = ROOT / "scripts/_ar3_application_architecture.py"
    text = ar3.read_text(encoding="utf-8")
    text = replace_once(text,
        "projection is architecture/inventory.json. Accepted runtime-resource decisions are consumed\nfrom architecture/runtime-topology-ar2.json without being re-decided here.",
        "projection is architecture/inventory.json. Runtime/resource topology is owned by provider-native\nconfiguration and Product Rust rather than consumed from the historical AR-2 decision artifact.")
    text = replace_once(text, "import copy\nimport json\n", "import copy\n")
    text = replace_once(text, 'RUNTIME_TOPOLOGY = "architecture/runtime-topology-ar2.json"\n')
    text = replace_once(text, '''        "resource_refs": [\n            "control_plane_worker",\n            "static_assets",\n            "catalog_d1",\n            "profile_objects",\n            "profile_coordinator",\n            "notification_hub",\n            "integration_events",\n            "mailbox_jobs",\n            "mailbox_jobs_dlq",\n            "generation_verification",\n            "mailbox_secret_resolver_service",\n            "control_plane_schedule",\n        ],\n''')
    text = replace_once(text, '''        "resource_refs": [\n            "mailbox_secret_resolver_worker",\n            "resolver_d1",\n            "resolver_reconciliation_schedule",\n        ],\n''')
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
    ar3.write_text(text, encoding="utf-8", newline="\n")

    engine = ROOT / "scripts/generate-architecture-inventory-engine.py"
    text = engine.read_text(encoding="utf-8")
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
    engine.write_text(text, encoding="utf-8", newline="\n")

    status_path = ROOT / "docs/status.json"
    status = json.loads(status_path.read_text(encoding="utf-8"))
    if status["current"]["architecture_program"].pop("runtime_topology_decision", None) is None:
        raise SystemExit("scratch status transform lost expected runtime_topology_decision")
    status_path.write_text(json.dumps(status, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")

    topology_path.unlink()
    subprocess.run([sys.executable, "scripts/generate-architecture-inventory.py", "--write"], cwd=ROOT, check=True)
    return topology


def tracked_paths() -> list[Path]:
    proc = subprocess.run(["git", "ls-files", "-z"], cwd=ROOT, check=True, capture_output=True)
    return [ROOT / value.decode("utf-8") for value in proc.stdout.split(b"\0") if value]


def scan() -> int:
    matches: list[tuple[str, int, str, str]] = []
    for path in tracked_paths():
        if not path.is_file():
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        relative = path.relative_to(ROOT).as_posix()
        for number, line in enumerate(text.splitlines(), 1):
            for term in TERMS:
                if term in line:
                    matches.append((relative, number, term, line.strip()))
    print(f"N1_AUDIT_MATCH_COUNT {len(matches)}")
    for relative, number, term, line in matches:
        print(f"N1_AUDIT_MATCH {relative}:{number} term={term} :: {line}")
    return len(matches)


def main() -> int:
    topology = transform()
    resources = topology.get("resources")
    if not isinstance(resources, list):
        raise SystemExit("old topology resources missing")
    print(f"N1_OLD_RESOURCE_DECISION_COUNT {len(resources)}")
    for item in resources:
        if isinstance(item, dict):
            print(f"N1_OLD_DECISION id={item.get('id')} decision={item.get('decision')} lane={item.get('lane')}")
    count = scan()
    print("N1_SCRATCH_AUDIT_COMPLETE")
    return 97 if count >= 0 else 98


if __name__ == "__main__":
    raise SystemExit(main())
