#!/usr/bin/env bash
set -euo pipefail

if [[ "${GITHUB_EVENT_NAME:-}" != "pull_request" ]]; then
  echo "Phase 2I release-candidate freeze is enforced on pull requests."
  exit 0
fi

base_ref="${GITHUB_BASE_REF:?GITHUB_BASE_REF is required for pull requests}"
git fetch --no-tags --depth=1 origin "${base_ref}"

hard_frozen_roots=(
  "proto"
  "contracts/baseline"
)

if ! git diff --quiet "origin/${base_ref}" -- "${hard_frozen_roots[@]}"; then
  echo "Phase 2I accepted hard-frozen contract authority was violated." >&2
  echo "Frozen contract roots: ${hard_frozen_roots[*]}" >&2
  git diff --stat "origin/${base_ref}" -- "${hard_frozen_roots[@]}" >&2
  exit 1
fi

echo "Phase 2I accepted baseline/protobuf roots are unchanged."

python scripts/check-pre2j-b4-contract-authority.py --self-test
python scripts/check-pre2j-b4-contract-authority.py --authority-only --base-ref "origin/${base_ref}"
python scripts/check-pre2j-c2-contract-authority.py --self-test
python scripts/check-pre2j-c2-contract-authority.py --authority-only --base-ref "origin/${base_ref}"
python scripts/check-pre2j-c3-contract-authority.py --self-test
python scripts/check-pre2j-c3-contract-authority.py --authority-only --base-ref "origin/${base_ref}"
python scripts/check-pre2j-c3g-contract-authority.py --self-test
python scripts/check-pre2j-c3g-contract-authority.py --base-ref "origin/${base_ref}"
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

  # The governed D3 dispatcher owns historical materialization. In AR-11 the
  # legacy promotion workflow may be intentionally absent from the current tree;
  # replay must come from the predecessor Git object instead of restoring a
  # retired executable authority into current source.
  python scripts/check-pre2j-d3-resolver-bootstrap-implementation.py \
    --base-ref "origin/${base_ref}"
  python scripts/check-pre2j-d3-resolver-bootstrap-implementation.py --self-test
fi

if git diff --quiet "origin/${base_ref}" -- migrations/d1; then
  echo "Accepted D1 migration history is unchanged."
else
  python - "origin/${base_ref}" <<'PY'
from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

base_ref = sys.argv[1]
root = Path.cwd()
status_path = root / "docs/status.json"
if not status_path.is_file():
    raise SystemExit("docs/status.json is required before D1 migration remediation can be evaluated")

status = json.loads(status_path.read_text(encoding="utf-8"))
remediation = status.get("current", {}).get("pre2j_product_readiness_remediation", {})
if not (
    remediation.get("status") == "active_blocking"
    and remediation.get("tracking_issue") == 203
    and remediation.get("plan") == "docs/PRE2J_PRODUCT_READINESS_REMEDIATION_PLAN.md"
):
    raise SystemExit(
        "D1 migration root remains frozen unless the canonical pre-2J product-readiness remediation is active"
    )

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
            "accepted D1 migration history is immutable during remediation; only new migration files may be appended: "
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
        "all remediation D1 changes must be a contiguous append-only suffix; "
        f"added={sorted(added)!r}, expected={current_files[len(base_files):]!r}"
    )
for path in added:
    if not (root / path).read_text(encoding="utf-8").strip():
        raise SystemExit(f"appended D1 migration must not be empty: {path}")

print(
    "Canonical pre-2J remediation permits only this append-only D1 migration suffix: "
    + ", ".join(added)
)
PY
fi

# AR-8 completion lifecycle is accepted history and still checks the exact D3-era
# routine-promotion surface. AR-11 deliberately retires that executable from the
# current tree, so materialize the predecessor bytes only inside this bounded
# historical replay and remove them again before returning to current-source checks.
(
  routine_promotion=".github/workflows/mailbox-secret-resolver-promotion.yml"
  if [[ ! -f "$routine_promotion" ]]; then
    readarray -t predecessor < <(python - <<'PY'
import json
from pathlib import Path

authority = json.loads(
    Path("architecture/ar8-d-secret-transport-successor.json").read_text(encoding="utf-8")
)
predecessor = authority["predecessor"]
print(predecessor["transition_base_main"])
print(predecessor["promotion_workflow"])
PY
)
    predecessor_ref="${predecessor[0]}"
    predecessor_workflow="${predecessor[1]}"
    cleanup_historical_promotion() {
      rm -f "$routine_promotion"
    }
    trap cleanup_historical_promotion EXIT
    git show "${predecessor_ref}:${predecessor_workflow}" > "$routine_promotion"
  fi
  node .github/scripts/ar8-completion-lifecycle.mjs
  node .github/scripts/ar8-completion-lifecycle.mjs --self-test
)

test ! -e .github/workflows/mailbox-secret-resolver-promotion.yml
python scripts/check-contract-compatibility.py
python scripts/test-d1-schema.py

echo "Phase 2I public contract freeze, immutable consumed B4/C2/C3 authorities, governed Pre-2J D3-to-AR-8D transition, AR-8D/E completion lifecycle overlay, pending C3G governed provider migration authority, and immutable-prefix D1 migration policy are valid."
