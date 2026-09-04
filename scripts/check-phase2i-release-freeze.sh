#!/usr/bin/env bash
set -euo pipefail

if [[ "${GITHUB_EVENT_NAME:-}" != "pull_request" ]]; then
  echo "Phase 2I accepted evidence protections are enforced on pull requests."
  exit 0
fi

base_ref="${GITHUB_BASE_REF:?GITHUB_BASE_REF is required for pull requests}"
git fetch --no-tags origin "+refs/heads/${base_ref}:refs/remotes/origin/${base_ref}"

immutable_evidence_roots=(
  "contracts/baseline"
)

if ! git diff --quiet "origin/${base_ref}" -- "${immutable_evidence_roots[@]}"; then
  echo "Accepted contract baseline evidence is immutable." >&2
  echo "Immutable evidence roots: ${immutable_evidence_roots[*]}" >&2
  git diff --stat "origin/${base_ref}" -- "${immutable_evidence_roots[@]}" >&2
  exit 1
fi

echo "Accepted contract baseline evidence is unchanged."

# Pre-2J B4/C2/C3/C3G one-shot contract authorities are retired. Current
# OpenAPI/protobuf evolution is governed by check-contract-compatibility.py.
# D3 is a separate resolver/bootstrap provenance concern and remains intact.
python scripts/check-pre2j-d3-resolver-bootstrap-authority.py --self-test
if git cat-file -e "origin/${base_ref}:architecture/pre2j-d3-resolver-bootstrap-authority.json" 2>/dev/null; then
  python scripts/check-pre2j-d3-resolver-bootstrap-authority.py \
    --authority-only \
    --base-ref "origin/${base_ref}"
else
  python scripts/check-pre2j-d3-resolver-bootstrap-authority.py --base-ref "origin/${base_ref}"
fi
if [[ -f architecture/pre2j-d3-resolver-bootstrap-implementation.json ]]; then
  if [[ -f architecture/ar8-d-secret-transport-successor.json ]]; then
    node .github/scripts/ar8-d-secret-transport-successor.mjs \
      --base-ref "origin/${base_ref}"
    node .github/scripts/ar8-d-secret-transport-successor.mjs --self-test
  fi

  # Current D3 validation is read-only with respect to historical Git objects.
  # Retired implementation may be inspected as immutable provenance, but it must
  # never be restored, imported or executed in the current working tree.
  python scripts/check-pre2j-d3-resolver-bootstrap-implementation.py \
    --base-ref "origin/${base_ref}"
  python scripts/check-pre2j-d3-resolver-bootstrap-implementation.py --self-test
fi

if git diff --quiet "origin/${base_ref}" -- migrations/d1; then
  echo "Accepted D1 migration history is unchanged."
else
  python - "origin/${base_ref}" <<'PY'
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

base_ref = sys.argv[1]
root = Path.cwd()

# Issue #203 and its docs/status.json projection are historical predecessor
# authority only. Current forward sequencing is #266 and D1 evolution is owned
# by the typed opsctl catalog plus this immutable-prefix/append-only boundary.
# Never require resurrection of the retired pre-2J blocker merely to admit a
# current governed migration.
name_status = subprocess.check_output(
    ["git", "diff", "--name-status", base_ref, "--", "migrations/d1"],
    text=True,
).splitlines()
if not name_status:
    raise SystemExit("D1 migration diff was expected but no changed paths were found")

added: list[str] = []
for line in name_status:
    fields = line.split("\t")
    if len(fields) != 2 or fields[0] != "A":
        raise SystemExit(
            "accepted D1 migration history is immutable; only new migration files may be appended: "
            + line
        )
    path = fields[1]
    if not re.fullmatch(r"migrations/d1/\d{4}_[A-Za-z0-9_]+\.sql", path):
        raise SystemExit(f"invalid appended D1 migration path: {path}")
    added.append(path)

base_listing = subprocess.check_output(
    ["git", "ls-tree", "-r", "--name-only", base_ref, "--", "migrations/d1"],
    text=True,
).splitlines()
base_files = sorted(
    path for path in base_listing if re.fullmatch(r"migrations/d1/\d{4}_[A-Za-z0-9_]+\.sql", path)
)
current_files = sorted(
    str(path.relative_to(root)).replace("\\", "/")
    for path in (root / "migrations/d1").glob("[0-9][0-9][0-9][0-9]_*.sql")
)


def versions(paths: list[str]) -> list[int]:
    return [int(Path(path).name.split("_", 1)[0]) for path in paths]


base_versions = versions(base_files)
current_versions = versions(current_files)
if base_versions != list(range(1, len(base_versions) + 1)):
    raise SystemExit(f"accepted base D1 migrations are not contiguous: {base_versions}")
if current_versions != list(range(1, len(current_versions) + 1)):
    raise SystemExit(f"current D1 migrations are not contiguous: {current_versions}")
if current_files[: len(base_files)] != base_files:
    raise SystemExit("accepted D1 migration prefix changed; existing migrations may not be edited, removed, renamed or reordered")
if sorted(added) != current_files[len(base_files) :]:
    raise SystemExit(
        "all D1 changes must be a contiguous append-only suffix; "
        f"added={sorted(added)!r}, expected={current_files[len(base_files):]!r}"
    )
for path in added:
    if not (root / path).read_text(encoding="utf-8").strip():
        raise SystemExit(f"appended D1 migration must not be empty: {path}")

print(
    "Current D1 evolution permits only this immutable-prefix append-only migration suffix: "
    + ", ".join(added)
)
PY
fi

# AR-8 completion is immutable provenance only. Its durable credential lifecycle
# rules are owned by current subject-domain authorities; this gate must never
# reconstruct or execute retired AR-8 workflow/script bytes to prove history.
test ! -e .github/workflows/mailbox-secret-resolver-promotion.yml
test ! -e .github/scripts/ar8-completion-lifecycle.mjs

# One permanent current-contract evolution verifier governs both OpenAPI and proto.
# Compatible additive current-v1 evolution is allowed; ordinary breaking change is
# rejected unless an explicit governed migration/re-baseline decision says otherwise.
python scripts/check-contract-compatibility.py
python scripts/test-d1-schema.py

echo "Immutable contract baseline evidence, permanent current OpenAPI/protobuf compatibility, static Pre-2J D3 provenance/current successor validation, current AR-8-derived lifecycle authorities, and immutable-prefix D1 migration policy are valid."
