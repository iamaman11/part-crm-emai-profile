#!/usr/bin/env python3
"""Disposable N1 projection emitter. Never transplant this helper."""
from __future__ import annotations

import base64
import gzip
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def replace_once(text: str, old: str, new: str = "") -> str:
    if text.count(old) != 1:
        raise SystemExit(f"scratch transform expected exactly one match: {old[:80]!r}")
    return text.replace(old, new, 1)


def transform() -> None:
    ar3 = ROOT / "scripts/_ar3_application_architecture.py"
    text = ar3.read_text(encoding="utf-8")
    text = replace_once(text,
        "projection is architecture/inventory.json. Accepted runtime-resource decisions are consumed\nfrom architecture/runtime-topology-ar2.json without being re-decided here.",
        "projection is architecture/inventory.json. Runtime/resource topology is owned by provider-native\nconfiguration and Product Rust rather than consumed from the historical AR-2 decision artifact.")
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

    topology = ROOT / "architecture/runtime-topology-ar2.json"
    if not topology.is_file():
        raise SystemExit("scratch expected old topology file before deletion")
    topology.unlink()


def emit(path: str) -> None:
    raw = (ROOT / path).read_bytes()
    payload = base64.b64encode(gzip.compress(raw, compresslevel=9, mtime=0)).decode("ascii")
    print(f"N1_BLOB_BEGIN {path} {len(raw)} {len(payload)}")
    for index in range(0, len(payload), 3000):
        print(f"N1_BLOB_CHUNK {path} {index // 3000:04d} {payload[index:index+3000]}")
    print(f"N1_BLOB_END {path}")


def main() -> int:
    transform()
    subprocess.run([sys.executable, "scripts/generate-architecture-inventory.py", "--write"], cwd=ROOT, check=True)
    subprocess.run([sys.executable, "-m", "json.tool", "architecture/inventory.json"], cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
    for path in (
        "scripts/_ar3_application_architecture.py",
        "scripts/generate-architecture-inventory-engine.py",
        "architecture/inventory.json",
        "docs/status.json",
    ):
        emit(path)
    print("N1_SCRATCH_GENERATION_COMPLETE")
    return 97


if __name__ == "__main__":
    raise SystemExit(main())
