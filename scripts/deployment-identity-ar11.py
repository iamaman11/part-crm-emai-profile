#!/usr/bin/env python3
"""Extract the current Release Set v2/Profile identity from saved Worker metadata.

Legacy Release Set markers fail closed: pre-production v1 is not interpreted as fresh state.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any, Iterable

V2 = re.compile(r"release_set=(release-set-v2-sha256-[0-9a-f]{64})\s+profile=([a-z0-9-]+)")
LEGACY = re.compile(r"release_set=release-set-v1-sha256-[0-9a-f]{64}")


def strings(value: Any) -> Iterable[str]:
    if isinstance(value, str):
        yield value
    elif isinstance(value, list):
        for child in value:
            yield from strings(child)
    elif isinstance(value, dict):
        for child in value.values():
            yield from strings(child)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--status", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        value = json.loads(args.status.read_text(encoding="utf-8"))
        observed_strings = list(strings(value))
        if any(LEGACY.search(text) for text in observed_strings):
            raise ValueError(
                "unsupported pre-production Release Set v1 is still deployed; clean staging before v2 promotion"
            )
        found = {(match.group(1), match.group(2)) for text in observed_strings for match in V2.finditer(text)}
        if len(found) > 1:
            raise ValueError(f"ambiguous deployment identity: {sorted(found)}")
        release_id, profile_id = next(iter(found)) if found else (None, None)
        if args.output.exists():
            raise ValueError(f"output already exists: {args.output}")
        args.output.write_text(
            json.dumps(
                {
                    "schema_version": 2,
                    "kind": "DEPLOYMENT_IDENTITY_OBSERVATION",
                    "release_set_id": release_id,
                    "capability_profile_id": profile_id,
                },
                sort_keys=True,
                indent=2,
            )
            + "\n",
            encoding="utf-8",
        )
        return 0
    except (OSError, json.JSONDecodeError, ValueError) as error:
        print(f"AR-11 deployment identity v2 error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
