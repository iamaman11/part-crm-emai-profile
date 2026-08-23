#!/usr/bin/env python3
"""Disposable N1 current-projection surgery. Never transplant this helper."""
from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def replace_once(text: str, old: str, new: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected one match, got {count}: {old!r}")
    return text.replace(old, new, 1)


def main() -> int:
    transition_path = ROOT / "architecture/architecture-rebaseline-v3-transition.json"
    transition = transition_path.read_text(encoding="utf-8")
    transition = replace_once(
        transition,
        '    "decision_authority": "architecture/runtime-topology-ar2.json",\n'
        '    "evidence": "docs/ARCHITECTURE_REBASELINE_V3_AR2.md",\n'
        '    "canonical_inventory_projection_owner": "AR-3",\n',
        '    "decision_authority": "RETIRED_CURRENT_SEMANTIC_AUTHORITY",\n'
        '    "historical_decision_evidence": "docs/ARCHITECTURE_REBASELINE_V3_AR2.md",\n'
        '    "current_resource_topology_authority": "WRANGLER_PROVIDER_NATIVE_CONFIGURATION_AND_PRODUCT_RUST",\n'
        '    "current_runtime_workload_authority": "PRODUCT_RUST",\n'
        '    "current_anti_regression_authority": "scripts/check-cloudflare-runtime-bindings.py",\n'
        '    "canonical_inventory_projection_owner": "AR-3_APPLICATION_PROCESS_AND_CAPABILITY_OWNERSHIP_ONLY",\n',
    )
    transition = replace_once(
        transition,
        '    "ar2_decision_input": "architecture/runtime-topology-ar2.json",\n',
        '    "ar2_decision_provenance": "docs/ARCHITECTURE_REBASELINE_V3_AR2.md",\n',
    )
    json.loads(transition)
    transition_path.write_text(transition, encoding="utf-8", newline="\n")

    readme_path = ROOT / "architecture/README.md"
    readme = readme_path.read_text(encoding="utf-8")
    readme = replace_once(
        readme,
        '- `runtime-topology-ar2.json`, `python-estate-ar6.json`, `github-governance-ar7.json` — accepted architecture artifacts whose phase-qualified names are preserved because they are accepted provenance/decision inputs rather than newly mutable AR-8 completion authorities.\n',
        '- `python-estate-ar6.json`, `github-governance-ar7.json` — accepted architecture artifacts whose phase-qualified names remain tracked provenance/decision inputs.\n'
        '- AR-2 runtime-topology authority is retired from the tracked current architecture: provider-native Wrangler configuration and Product Rust own executable topology/workloads, bounded current fitness rules prevent regression, and AR-2 provenance remains in `docs/ARCHITECTURE_REBASELINE_V3_AR2.md` plus Git history.\n',
    )
    readme_path.write_text(readme, encoding="utf-8", newline="\n")

    print("N1_CURRENT_PROJECTION_SURGERY_COMPLETE")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
