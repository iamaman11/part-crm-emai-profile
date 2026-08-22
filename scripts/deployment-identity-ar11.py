#!/usr/bin/env python3
"""Extract the current supported Release Set/Profile identity from saved Worker metadata.

This is a bounded provider-observation adapter. It recognizes exact immutable Release Set v2/v3
annotations but does not interpret Release Set document semantics. Unsupported, malformed, or
ambiguous Release Set markers fail closed.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any, Iterable

RELEASE_ID = re.compile(r"release-set-v(?:2|3)-sha256-[0-9a-f]{64}")
PROFILE_ID = re.compile(r"[a-z0-9-]+")
ANNOTATION = re.compile(r"release_set=([^\s]+)\s+profile=([^\s]+)")
RELEASE_TOKEN = re.compile(r"release_set=([^\s]+)")


def strings(value: Any) -> Iterable[str]:
    if isinstance(value, str):
        yield value
    elif isinstance(value, list):
        for child in value:
            yield from strings(child)
    elif isinstance(value, dict):
        for child in value.values():
            yield from strings(child)


def extract_identity(value: Any) -> tuple[str | None, str | None]:
    found: set[tuple[str, str]] = set()
    for text in strings(value):
        release_tokens = RELEASE_TOKEN.findall(text)
        annotations = ANNOTATION.findall(text)
        if len(annotations) != len(release_tokens):
            raise ValueError("malformed deployment Release Set annotation")
        for release_id, profile_id in annotations:
            if RELEASE_ID.fullmatch(release_id) is None:
                raise ValueError(f"unsupported or malformed deployed Release Set identity: {release_id}")
            if PROFILE_ID.fullmatch(profile_id) is None:
                raise ValueError(f"malformed deployed capability profile identity: {profile_id}")
            found.add((release_id, profile_id))
    if len(found) > 1:
        raise ValueError(f"ambiguous deployment identity: {sorted(found)}")
    return next(iter(found)) if found else (None, None)


def self_test() -> None:
    v2 = "release-set-v2-sha256-" + "a" * 64
    v3 = "release-set-v3-sha256-" + "b" * 64
    profile = "rehearsal-core-v1"
    if extract_identity({"message": f"release_set={v2} profile={profile}"}) != (v2, profile):
        raise ValueError("historical Release Set v2 identity was not observed")
    if extract_identity({"message": f"release_set={v3} profile={profile}"}) != (v3, profile):
        raise ValueError("current Release Set v3 identity was not observed")
    for invalid in (
        "release-set-v1-sha256-" + "a" * 64,
        "release-set-v4-sha256-" + "a" * 64,
        "release-set-v3-sha256-deadbeef",
    ):
        try:
            extract_identity({"message": f"release_set={invalid} profile={profile}"})
        except ValueError:
            continue
        raise ValueError(f"unsupported/malformed Release Set identity unexpectedly accepted: {invalid}")
    try:
        extract_identity(
            [
                {"message": f"release_set={v2} profile={profile}"},
                {"message": f"release_set={v3} profile={profile}"},
            ]
        )
    except ValueError:
        print("AR-11 deployment identity adapter self-test passed.")
        return
    raise ValueError("ambiguous deployment identity unexpectedly accepted")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--status", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test:
            self_test()
            return 0
        if args.status is None or args.output is None:
            raise ValueError("status and output are required")
        value = json.loads(args.status.read_text(encoding="utf-8"))
        release_id, profile_id = extract_identity(value)
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
        print(f"AR-11 deployment identity adapter error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
