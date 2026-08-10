#!/usr/bin/env bash
set -euo pipefail

if [[ "${GITHUB_EVENT_NAME:-}" != "pull_request" ]]; then
  echo "Accepted contract and phase-history baselines are enforced on pull requests."
  exit 0
fi

base_ref="${GITHUB_BASE_REF:?GITHUB_BASE_REF is required for pull requests}"
git fetch --no-tags --depth=1 origin "${base_ref}"

baseline_marker="contracts/baseline/openapi/v1/openapi.json"
if git cat-file -e "origin/${base_ref}:${baseline_marker}" 2>/dev/null; then
  if ! git diff --quiet "origin/${base_ref}" -- contracts/baseline; then
    echo "Accepted v1 contract baseline is immutable." >&2
    echo "Introduce a new major contract root or a separately governed migration instead." >&2
    git diff --stat "origin/${base_ref}" -- contracts/baseline >&2
    exit 1
  fi
  echo "Accepted v1 contract baseline is unchanged."
else
  echo "No accepted v1 baseline exists on ${base_ref}; initial establishment is allowed."
fi

phase_ledger="architecture/accepted-phases.json"
if git cat-file -e "origin/${base_ref}:${phase_ledger}" 2>/dev/null; then
  if [[ ! -f "${phase_ledger}" ]]; then
    echo "Accepted phase provenance ledger cannot be removed." >&2
    exit 1
  fi

  base_ledger="$(mktemp)"
  trap 'rm -f "${base_ledger}"' EXIT
  git show "origin/${base_ref}:${phase_ledger}" > "${base_ledger}"

  python - "${base_ledger}" "${phase_ledger}" <<'PY'
import json
import sys
from pathlib import Path

base_path = Path(sys.argv[1])
current_path = Path(sys.argv[2])
base = json.loads(base_path.read_text(encoding="utf-8"))
current = json.loads(current_path.read_text(encoding="utf-8"))

if current.get("schema_version") != base.get("schema_version"):
    raise SystemExit("accepted phase provenance schema cannot change inside an ordinary PR")

base_phases = base.get("accepted_phases")
current_phases = current.get("accepted_phases")
if not isinstance(base_phases, list) or not isinstance(current_phases, list):
    raise SystemExit("accepted phase provenance must contain an accepted_phases list")
if len(current_phases) < len(base_phases):
    raise SystemExit("accepted phase provenance history cannot shrink")
if current_phases[: len(base_phases)] != base_phases:
    raise SystemExit("accepted phase provenance history is immutable; only append new accepted phases")

print(f"Accepted phase provenance prefix is immutable ({len(base_phases)} preserved records).")
PY
else
  echo "No accepted phase provenance ledger exists on ${base_ref}; initial establishment is allowed."
fi
