#!/usr/bin/env python3
"""Read-only D1 proof: only CURRENT's exact journal-linked VERIFIED predecessor is visible.

This is a candidate descriptor, NOT a durable Camoufox WORKING/LKG certificate,
old-key availability proof, signed R2 capability or prune permission.
"""

from __future__ import annotations

import re
import runpy
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ADAPTER = ROOT / "crates/cloudflare-adapters/src/d1_profile_generation_successor.rs"
FIXTURE = ROOT / "scripts/test-device-generation-commit-d1.py"


def predecessor_query() -> str:
    # Execute the actual production D1 adapter query, not a separately
    # paraphrased SQL test. The fixture's D1 mutations are governed by the
    # canonical migrations and existing trigger-owned successor command.
    source = ADAPTER.read_text(encoding="utf-8")
    match = re.search(
        r'const LOAD_VERIFIED_PREDECESSOR_OF_ACTIVE: &str = r#"(.*?)"#;',
        source,
        flags=re.DOTALL,
    )
    if match is None:
        raise AssertionError("canonical D1 historical candidate reader is missing")
    return match.group(1)


def main() -> int:
    fixture = runpy.run_path(str(FIXTURE))
    tenant = fixture["TENANT"]
    profile = fixture["PROFILE"]
    base = fixture["BASE"]
    current = fixture["CANDIDATE"]
    winner = fixture["WINNER"]
    query = predecessor_query()
    connection = fixture["seed"]()
    try:
        def find(target: str, *, tenant_id: str = tenant):
            return connection.execute(query, (tenant_id, profile, target)).fetchone()

        # A VERIFIED base without a committed N+1 does not invent a previous LKG.
        assert find(base) is None
        fixture["insert_device_commit"](connection, fixture["command_values"]())
        connection.commit()
        row = find(current)
        assert row is not None
        assert row["generation_id"] == base
        assert row["object_key"] == fixture["canonical_key"](base)
        assert row["metadata_digest"] == "a" * 64
        assert row["container_digest"] == "b" * 64
        assert find(base) is None
        assert find(current, tenant_id="tenant_foreign_history") is None

        # Even a previously valid historical descriptor becomes ineligible
        # if CURRENT advances by an independent governed activation. Never
        # scan by age or silently step back more than one generation.
        fixture["owner_register"](connection, winner, 110)
        fixture["owner_verify"](connection, winner, 115)
        fixture["owner_activate"](connection, winner, 3, 120)
        connection.commit()
        assert find(current) is None
        assert find(winner) is None

        # The reader never grants WORKING, changes the pointer or deletes R2.
        assert connection.execute(
            "SELECT COUNT(*) FROM profile_generation_successor_commands"
        ).fetchone()[0] == 1
        print("K2 D1 exact historical predecessor is read-only and fail-closed.")
    finally:
        connection.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
