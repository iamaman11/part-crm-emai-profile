#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

pattern='BEGIN (RSA|EC|OPENSSH) PRIVATE KEY|AKIA[0-9A-Z]{16}|gh[pousr]_[A-Za-z0-9_]{20,}|github_pat_[A-Za-z0-9_]{20,}|-----BEGIN PRIVATE KEY-----'
set +e
matches="$(git grep -n -E "$pattern" -- . ':(exclude)scripts/check-tracked-secrets.sh' 2>&1)"
status=$?
set -e
if [[ $status -eq 0 ]]; then
  echo 'Potential tracked credential material detected:' >&2
  printf '%s\n' "$matches" >&2
  exit 1
elif [[ $status -ne 1 ]]; then
  echo 'git grep credential scan failed:' >&2
  printf '%s\n' "$matches" >&2
  exit "$status"
fi

# Credential semantics stay with their bounded owner; this shell only adds an
# independent high-confidence tracked-byte scan.
python scripts/credential_authority.py --self-test

echo 'No high-confidence tracked credential patterns found; AR-8B portable credential authority and negative matrix are consistent.'
