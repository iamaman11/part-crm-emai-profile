#!/usr/bin/env python3
"""Reject raw D1 statements outside the Cloudflare adapter boundary."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

# Keep this list specific to D1 APIs. Generic method names such as `.prepare(`
# and `.batch(` also belong to cryptography, HTTP and application code and
# therefore create false positives without strengthening the D1 boundary.
RAW_D1_TOKENS = (
    "D1Database",
    "D1PreparedStatement",
    "D1Result",
    "worker::d1",
    "query!(",
)


def check(root: Path) -> list[str]:
    errors: list[str] = []
    repository_root = root.resolve() == Path.cwd().resolve()

    for path in sorted(root.rglob("*.rs")):
        relative = path.relative_to(root)
        text = path.read_text(encoding="utf-8")

        if repository_root and relative.parts[:2] == ("tests", "d1-boundary"):
            continue

        if relative.parts[:2] == ("crates", "cloudflare-adapters"):
            continue

        forbidden = [token for token in RAW_D1_TOKENS if token in text]
        if forbidden:
            if relative.parts[:3] == ("apps", "control-plane-worker", "src"):
                errors.append(
                    f"{relative}: Worker composition may obtain env.d1 only; raw tokens {forbidden}"
                )
            else:
                errors.append(f"{relative}: raw D1 access outside adapter boundary: {forbidden}")

    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    args = parser.parse_args()

    errors = check(args.root)
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1

    print("D1 access is confined to the typed Cloudflare adapter boundary.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
