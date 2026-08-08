#!/usr/bin/env python3
"""Enforce frontend sibling-feature boundaries without relying on TypeScript path aliases.

Feature modules may import shared/entities/app composition through normal relative imports and may
import declared npm packages. A feature may import a sibling feature only through that sibling's
explicit root public API (`index.ts` / `index.tsx` or the feature directory itself).

Unknown non-relative imports from feature source are rejected. This intentionally makes opaque
TypeScript/Vite aliases fail closed instead of providing an alternate path around the boundary.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
SOURCE_SUFFIXES = {".ts", ".tsx", ".mts", ".cts"}
STATIC_IMPORT_RE = re.compile(
    r"\b(?:import|export)\s+(?:type\s+)?(?:[^;]*?\s+from\s+)?[\"']([^\"']+)[\"']",
    re.MULTILINE | re.DOTALL,
)
DYNAMIC_IMPORT_RE = re.compile(r"\bimport\s*\(\s*[\"']([^\"']+)[\"']\s*\)")


@dataclass(frozen=True)
class Violation:
    path: Path
    specifier: str
    reason: str


def package_name(specifier: str) -> str:
    parts = specifier.split("/")
    if specifier.startswith("@") and len(parts) >= 2:
        return "/".join(parts[:2])
    return parts[0]


def declared_packages(frontend_root: Path) -> set[str]:
    package_json = frontend_root / "package.json"
    if not package_json.is_file():
        return set()
    payload = json.loads(package_json.read_text(encoding="utf-8"))
    packages: set[str] = set()
    for key in ("dependencies", "devDependencies", "peerDependencies"):
        values = payload.get(key, {})
        if isinstance(values, dict):
            packages.update(str(name) for name in values)
    return packages


def import_specifiers(source: str) -> set[str]:
    return {
        *(match.group(1) for match in STATIC_IMPORT_RE.finditer(source)),
        *(match.group(1) for match in DYNAMIC_IMPORT_RE.finditer(source)),
    }


def feature_target(path: Path, features_root: Path) -> tuple[str, tuple[str, ...]] | None:
    try:
        relative = path.relative_to(features_root)
    except ValueError:
        return None
    if not relative.parts:
        return None
    return relative.parts[0], tuple(relative.parts[1:])


def is_public_feature_api(remainder: tuple[str, ...]) -> bool:
    if not remainder:
        return True
    return len(remainder) == 1 and remainder[0] in {
        "index",
        "index.ts",
        "index.tsx",
        "index.mts",
        "index.cts",
    }


def lexical_feature_target(specifier: str, feature_names: set[str]) -> tuple[str, tuple[str, ...]] | None:
    normalized = specifier.replace("\\", "/").strip("/")
    parts = tuple(part for part in normalized.split("/") if part not in {"", "."})
    for index, part in enumerate(parts):
        if part != "features" or index + 1 >= len(parts):
            continue
        feature = parts[index + 1]
        if feature in feature_names:
            return feature, parts[index + 2 :]
    return None


def inspect_feature_source(
    source_path: Path,
    frontend_root: Path,
    features_root: Path,
    packages: set[str],
    feature_names: set[str],
) -> list[Violation]:
    violations: list[Violation] = []
    source_feature = source_path.relative_to(features_root).parts[0]
    text = source_path.read_text(encoding="utf-8")

    for specifier in sorted(import_specifiers(text)):
        if specifier.startswith("."):
            candidate = (source_path.parent / specifier).resolve()
            target = feature_target(candidate, features_root.resolve())
            if target is None:
                continue
            target_feature, remainder = target
            if target_feature != source_feature and not is_public_feature_api(remainder):
                violations.append(
                    Violation(
                        source_path,
                        specifier,
                        f"feature '{source_feature}' imports sibling '{target_feature}' internals; use the sibling root public API",
                    )
                )
            continue

        lexical_target = lexical_feature_target(specifier, feature_names)
        if lexical_target is not None:
            target_feature, remainder = lexical_target
            if target_feature != source_feature and not is_public_feature_api(remainder):
                violations.append(
                    Violation(
                        source_path,
                        specifier,
                        f"feature '{source_feature}' imports sibling '{target_feature}' internals through a non-relative path",
                    )
                )
            continue

        if package_name(specifier) in packages:
            continue

        violations.append(
            Violation(
                source_path,
                specifier,
                "non-relative local/alias import from feature source is forbidden; use a relative shared/entities/app path or an explicit sibling public API",
            )
        )

    return violations


def scan(root: Path) -> list[Violation]:
    frontend_root = root / "frontend"
    features_root = frontend_root / "src" / "features"
    if not features_root.is_dir():
        return [Violation(features_root, "", "frontend feature root is missing")]

    feature_names = {entry.name for entry in features_root.iterdir() if entry.is_dir()}
    packages = declared_packages(frontend_root)
    violations: list[Violation] = []

    for source_path in sorted(features_root.rglob("*")):
        if not source_path.is_file() or source_path.suffix not in SOURCE_SUFFIXES:
            continue
        violations.extend(
            inspect_feature_source(
                source_path=source_path,
                frontend_root=frontend_root,
                features_root=features_root,
                packages=packages,
                feature_names=feature_names,
            )
        )
    return violations


def print_violations(root: Path, violations: list[Violation]) -> None:
    for violation in violations:
        try:
            display = violation.path.relative_to(root)
        except ValueError:
            display = violation.path
        detail = f" import {violation.specifier!r}" if violation.specifier else ""
        print(f"{display}:{detail}: {violation.reason}", file=sys.stderr)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=REPO_ROOT)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        fixture_root = REPO_ROOT / "tests" / "frontend-feature-boundary" / "fixtures" / "sibling-internal"
        violations = scan(fixture_root)
        if not any("imports sibling 'profiles' internals" in violation.reason for violation in violations):
            print("negative sibling-feature fixture was not rejected", file=sys.stderr)
            print_violations(fixture_root, violations)
            return 1
        print("frontend sibling-feature negative fixture rejected as expected")
        return 0

    root = args.root.resolve()
    violations = scan(root)
    if violations:
        print_violations(root, violations)
        return 1
    print("frontend feature boundaries passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
