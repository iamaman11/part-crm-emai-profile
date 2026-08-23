#!/usr/bin/env python3
"""Disposable exact-candidate N1 caller/invariant audit. Never transplant."""
from __future__ import annotations

import subprocess

TARGET = "f40fd5f0b3425206b09eed5558c8f7af44b83db5"
PATTERNS = [
    r"runtime-topology-ar2\.json",
    r"runtime_topology_decision",
    r"topology_source",
    r"runtime_resources",
    r"resource_refs",
    r"EXPECTED_RESOURCE_DECISIONS",
    r"validate_ar2_topology",
    r"RUNTIME_TOPOLOGY",
]
SEMANTIC_ANCHORS = [
    r"GENERATION_VERIFICATION",
    r"generation-verification",
    r"resolver.*isolat|isolat.*resolver",
    r"EXTEND_CANONICAL_INVENTORY_DO_NOT_CREATE_COMPETING_REGISTRY",
    r"production_mutation",
    r"production_core_gate",
    r"PC-1_AFTER_AR-17",
    r"AR-11 Release Set|AR11_CHECKER|release-operational-ar11",
]


def grep(label: str, pattern: str) -> None:
    completed = subprocess.run(
        ["git", "grep", "-n", "-E", pattern, TARGET, "--", "."],
        text=True,
        capture_output=True,
        check=False,
    )
    if completed.returncode not in (0, 1):
        raise SystemExit(completed.stderr or f"git grep failed for {pattern}")
    print(f"N1_AUDIT_BEGIN {label} {pattern}")
    if completed.stdout:
        print(completed.stdout, end="")
    else:
        print("<NO_MATCHES>")
    print(f"N1_AUDIT_END {label} {pattern}")


def main() -> int:
    actual = subprocess.check_output(["git", "rev-parse", f"{TARGET}^{{commit}}"], text=True).strip()
    if actual != TARGET:
        raise SystemExit(f"candidate identity mismatch: {actual}")
    for pattern in PATTERNS:
        grep("OLD_CALLER_PATTERN", pattern)
    for pattern in SEMANTIC_ANCHORS:
        grep("CURRENT_INVARIANT_ANCHOR", pattern)
    print(f"N1_EXACT_CANDIDATE_AUDIT_COMPLETE {TARGET}")
    return 97


if __name__ == "__main__":
    raise SystemExit(main())
