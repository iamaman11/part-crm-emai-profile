#!/usr/bin/env bash
set -euo pipefail

if [[ "${GITHUB_EVENT_NAME:-}" != "pull_request" ]]; then
  echo "Contract baseline immutability is enforced on pull requests."
  exit 0
fi

base_ref="${GITHUB_BASE_REF:?GITHUB_BASE_REF is required for pull requests}"
git fetch --no-tags --depth=1 origin "${base_ref}"

baseline_marker="contracts/baseline/openapi/v1/openapi.json"
if ! git cat-file -e "origin/${base_ref}:${baseline_marker}" 2>/dev/null; then
  echo "No accepted v1 baseline exists on ${base_ref}; initial establishment is allowed."
  exit 0
fi

if ! git diff --quiet "origin/${base_ref}" -- contracts/baseline; then
  echo "Accepted v1 contract baseline is immutable." >&2
  echo "Introduce a new major contract root or a separately governed migration instead." >&2
  git diff --stat "origin/${base_ref}" -- contracts/baseline >&2
  exit 1
fi

echo "Accepted v1 contract baseline is unchanged."
