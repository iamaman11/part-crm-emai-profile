#!/usr/bin/env bash
set -euo pipefail

pattern='BEGIN (RSA|EC|OPENSSH) PRIVATE KEY|AKIA[0-9A-Z]{16}|gh[pousr]_[A-Za-z0-9_]{20,}|-----BEGIN PRIVATE KEY-----'

if git grep -n -E "$pattern" -- . ':!scripts/check-tracked-secrets.sh'; then
  echo "Potential credential material found in tracked files." >&2
  exit 1
fi

echo "No high-confidence credential patterns found in tracked files."
