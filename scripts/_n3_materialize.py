#!/usr/bin/env python3
from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
NEW_CONFIG = "architecture/github-governance.json"


def replace_once(path: str, old: str, new: str) -> None:
    target = ROOT / path
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected exactly one replacement target, found {count}")
    target.write_text(text.replace(old, new), encoding="utf-8", newline="\n")


# Collapse AR-7 baseline + AR-10 overlay workflow wiring to one current path.
replace_once(
    ".github/workflows/github-governance-gate.yml",
    '''      - name: Validate AR-7 governance contract
        run: node .github/scripts/github-governance.mjs contract

      - name: Prove AR-7 governance drift fails closed
        run: node .github/scripts/github-governance.mjs self-test

      - name: Validate AR-10 additive governance extension
        run: node .github/scripts/ar10-github-governance-extension.mjs contract

      - name: Prove AR-10 governance weakening fails closed
        run: node .github/scripts/ar10-github-governance-extension.mjs self-test
''',
    '''      - name: Validate current GitHub governance desired state
        run: node .github/scripts/github-governance.mjs contract

      - name: Prove GitHub governance weakening fails closed
        run: node .github/scripts/github-governance.mjs self-test
''',
)
replace_once(
    ".github/workflows/github-governance-gate.yml",
    '''      - name: Audit accepted AR-7 live hosted GitHub governance baseline
        run: node .github/scripts/github-governance.mjs live

      - name: Audit AR-10 additive live hosted governance
        run: node .github/scripts/ar10-github-governance-extension.mjs live
''',
    '''      - name: Audit current live hosted GitHub governance
        run: node .github/scripts/github-governance.mjs live
''',
)

# Retarget the generated architecture projection to the current desired config.
engine_path = ROOT / "scripts/generate-architecture-inventory-engine.py"
engine = engine_path.read_text(encoding="utf-8")
old_constant = 'GOVERNANCE_CONTRACT = "architecture/github-governance-ar7.json"'
if engine.count(old_constant) != 1:
    raise SystemExit("inventory engine governance constant is not the expected predecessor")
engine = engine.replace(old_constant, f'GOVERNANCE_CONTRACT = "{NEW_CONFIG}"')
old_status = '{"path": GOVERNANCE_CONTRACT, "status": "STABLE_AUTHORITY", "scope": "accepted_ar7_github_governance_contract"},'
new_status = '{"path": GOVERNANCE_CONTRACT, "status": "STABLE_AUTHORITY", "scope": "current_github_governance_desired_configuration"},'
if engine.count(old_status) != 1:
    raise SystemExit("inventory engine governance document-status predecessor not found exactly once")
engine = engine.replace(old_status, new_status)
needle = "    credential_projection = build_credential_projection(credential_authority)\n"
if engine.count(needle) != 1:
    raise SystemExit("inventory engine credential projection insertion point drifted")
engine = engine.replace(
    needle,
    needle
    + "    governance_desired = json.loads((ROOT / GOVERNANCE_CONTRACT).read_text(encoding=\"utf-8\"))\n"
    + "    if not isinstance(governance_desired, dict):\n"
    + "        raise SystemExit(\"current GitHub governance desired state must be one JSON object\")\n"
    + "    governance_main = governance_desired.get(\"main\")\n"
    + "    if not isinstance(governance_main, dict) or not isinstance(governance_main.get(\"required_checks\"), list):\n"
    + "        raise SystemExit(\"current GitHub governance desired state is missing main.required_checks\")\n",
)
start_marker = '        "github_governance_authority": {\n'
end_marker = '        "credential_authority": credential_projection,\n'
start = engine.find(start_marker)
end = engine.find(end_marker, start)
if start < 0 or end < 0:
    raise SystemExit("inventory engine historical governance projection block not found")
replacement = '''        "github_governance_authority": {
            "schema_version": int(governance_desired["schema_version"]),
            "kind": str(governance_desired["kind"]),
            "desired_configuration": GOVERNANCE_CONTRACT,
            "validator": ".github/scripts/github-governance.mjs",
            "workflow": ".github/workflows/github-governance-gate.yml",
            "required_check_count": len(governance_main["required_checks"]),
            "observation_mode": str(governance_desired["observation_mode"]),
            "production_mutation": governance_desired.get("evaluation", {}).get("production_mutation") is True,
        },
'''
engine = engine[:start] + replacement + engine[end:]
engine_path.write_text(engine, encoding="utf-8", newline="\n")

# Current status points at the current config; AR-7 acceptance remains immutable provenance.
status_path = ROOT / "docs/status.json"
status = json.loads(status_path.read_text(encoding="utf-8"))
program = status["current"]["architecture_program"]
if program.get("github_governance_contract") != "architecture/github-governance-ar7.json":
    raise SystemExit("docs/status.json predecessor governance path drifted")
program["github_governance_contract"] = NEW_CONFIG
status_path.write_text(json.dumps(status, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")

# Transition is projection only: replace copied AR-7 authority ledger with a bounded current projection.
transition_path = ROOT / "architecture/architecture-rebaseline-v3-transition.json"
transition = json.loads(transition_path.read_text(encoding="utf-8"))
old_projection = transition.get("github_governance_authority")
if not isinstance(old_projection, dict) or old_projection.get("contract") != "architecture/github-governance-ar7.json":
    raise SystemExit("transition governance predecessor projection drifted")
desired = json.loads((ROOT / NEW_CONFIG).read_text(encoding="utf-8"))
transition["github_governance_authority"] = {
    "schema_version": desired["schema_version"],
    "kind": desired["kind"],
    "desired_configuration": NEW_CONFIG,
    "validator": ".github/scripts/github-governance.mjs",
    "workflow": ".github/workflows/github-governance-gate.yml",
    "required_check_count": len(desired["main"]["required_checks"]),
    "observation_mode": desired["observation_mode"],
    "production_mutation": desired["evaluation"]["production_mutation"],
}
transition_path.write_text(json.dumps(transition, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")

for path in (
    ".github/workflows/github-governance-gate.yml",
    "scripts/generate-architecture-inventory-engine.py",
    "docs/status.json",
    "architecture/architecture-rebaseline-v3-transition.json",
):
    text = (ROOT / path).read_text(encoding="utf-8")
    if "ar10-github-governance-extension.mjs" in text:
        raise SystemExit(f"{path}: AR-10 governance overlay caller survived materialization")
    if "architecture/github-governance-ar7.json" in text:
        raise SystemExit(f"{path}: historical AR-7 governance contract survived as a current caller")

print("N3 materialization inputs normalized.")
