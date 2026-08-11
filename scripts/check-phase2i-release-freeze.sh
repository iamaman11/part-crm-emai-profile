#!/usr/bin/env bash
set -euo pipefail

if [[ "${GITHUB_EVENT_NAME:-}" != "pull_request" ]]; then
  echo "Phase 2I release-candidate freeze is enforced on pull requests."
  exit 0
fi

base_ref="${GITHUB_BASE_REF:?GITHUB_BASE_REF is required for pull requests}"
git fetch --no-tags --depth=1 origin "${base_ref}"

frozen_roots=(
  "openapi/v1"
  "proto"
  "contracts/baseline"
  "migrations/d1"
)

if ! git diff --quiet "origin/${base_ref}" -- "${frozen_roots[@]}"; then
  echo "Phase 2I release-candidate contract/migration freeze was violated." >&2
  echo "Frozen roots: ${frozen_roots[*]}" >&2
  git diff --stat "origin/${base_ref}" -- "${frozen_roots[@]}" >&2
  exit 1
fi

python scripts/check-contract-compatibility.py
python scripts/test-d1-schema.py

echo "Phase 2I release-candidate contract and D1 migration roots are frozen and valid."
