#!/usr/bin/env python3
"""Enforce frontend feature and root-route composition boundaries.

Feature modules may import shared/entities/app composition through normal relative imports and may
import declared npm packages. A feature may import a sibling feature only through that sibling's
explicit root public API (`index.ts` / `index.tsx` or the feature directory itself).

The root app router may compose feature routes only through those same public feature APIs; it may
not import feature-internal workspaces/components. Unknown non-relative imports from feature source
are rejected. TypeScript `paths` and custom Vite `resolve` configuration are also rejected until
this checker explicitly understands their resolved targets, so aliases cannot become bypasses.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
SOURCE_SUFFIXES = {".ts", ".tsx", ".mts", ".cts"}
STATIC_IMPORT_RE = re.compile(
    r"\b(?:import|export)\s+(?:type\s+)?(?:[^;]*?\s+from\s+)?[\"']([^\"']+)[\"']",
    re.MULTILINE | re.DOTALL,
)
DYNAMIC_IMPORT_RE = re.compile(r"\bimport\s*\(\s*[\"']([^\"']+)[\"']\s*\)")
VITE_RESOLVE_RE = re.compile(r"\bresolve\s*:")


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


def resolver_configuration_violations(frontend_root: Path) -> list[Violation]:
    violations: list[Violation] = []

    for tsconfig in sorted(frontend_root.glob("tsconfig*.json")):
        try:
            payload = json.loads(tsconfig.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            violations.append(
                Violation(
                    tsconfig,
                    "",
                    f"cannot audit TypeScript resolver configuration: {error}",
                )
            )
            continue
        compiler_options = payload.get("compilerOptions", {})
        if not isinstance(compiler_options, dict):
            continue
        paths = compiler_options.get("paths")
        if isinstance(paths, dict) and paths:
            violations.append(
                Violation(
                    tsconfig,
                    "paths",
                    "TypeScript path aliases are forbidden until the feature-boundary checker resolves and validates their targets",
                )
            )

    for vite_config in sorted(frontend_root.glob("vite.config.*")):
        try:
            source = vite_config.read_text(encoding="utf-8")
        except OSError as error:
            violations.append(
                Violation(vite_config, "", f"cannot audit Vite resolver configuration: {error}")
            )
            continue
        if VITE_RESOLVE_RE.search(source):
            violations.append(
                Violation(
                    vite_config,
                    "resolve",
                    "custom Vite resolve configuration is forbidden until the feature-boundary checker resolves and validates aliases",
                )
            )

    return violations


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


def app_route_composition_violations(
    frontend_root: Path,
    feature_names: set[str],
) -> list[Violation]:
    router_path = frontend_root / "src" / "app" / "router.tsx"
    if not router_path.is_file():
        return [Violation(router_path, "", "root app router is missing")]

    violations: list[Violation] = []
    text = router_path.read_text(encoding="utf-8")
    for specifier in sorted(import_specifiers(text)):
        target = lexical_feature_target(specifier, feature_names)
        if target is None:
            continue
        feature, remainder = target
        if not is_public_feature_api(remainder):
            violations.append(
                Violation(
                    router_path,
                    specifier,
                    f"root app router imports feature '{feature}' internals; compose routes through the feature root public API",
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
    violations = resolver_configuration_violations(frontend_root)
    violations.extend(app_route_composition_violations(frontend_root, feature_names))

    for source_path in sorted(features_root.rglob("*")):
        if not source_path.is_file() or source_path.suffix not in SOURCE_SUFFIXES:
            continue
        violations.extend(
            inspect_feature_source(
                source_path=source_path,
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


def require_fixture_rejection(
    fixture_root: Path,
    expected_reason_fragment: str,
    label: str,
) -> bool:
    violations = scan(fixture_root)
    if any(expected_reason_fragment in violation.reason for violation in violations):
        print(f"{label} negative fixture rejected as expected")
        return True
    print(f"{label} negative fixture was not rejected", file=sys.stderr)
    print_violations(fixture_root, violations)
    return False


def root_route_negative_self_test() -> bool:
    with tempfile.TemporaryDirectory(prefix="frontend-root-route-boundary-") as directory:
        root = Path(directory)
        frontend = root / "frontend"
        features = frontend / "src" / "features"
        clients = features / "clients"
        app = frontend / "src" / "app"
        clients.mkdir(parents=True)
        app.mkdir(parents=True)
        (frontend / "package.json").write_text("{}\n", encoding="utf-8")
        (clients / "ClientsWorkspace.tsx").write_text(
            "export function ClientsWorkspace() { return null; }\n",
            encoding="utf-8",
        )
        (clients / "index.ts").write_text(
            "export const createClientsRoute = () => null;\n",
            encoding="utf-8",
        )
        (app / "router.tsx").write_text(
            "import { ClientsWorkspace } from '../features/clients/ClientsWorkspace';\n"
            "void ClientsWorkspace;\n",
            encoding="utf-8",
        )
        violations = scan(root)
        if any("root app router imports feature 'clients' internals" in item.reason for item in violations):
            print("root-route feature-internal import negative fixture rejected as expected")
            return True
        print("root-route feature-internal import negative fixture was not rejected", file=sys.stderr)
        print_violations(root, violations)
        return False


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=REPO_ROOT)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        fixture_base = REPO_ROOT / "tests" / "frontend-feature-boundary" / "fixtures"
        results = [
            require_fixture_rejection(
                fixture_base / "sibling-internal",
                "imports sibling 'profiles' internals",
                "sibling-feature",
            ),
            require_fixture_rejection(
                fixture_base / "alias-bypass",
                "TypeScript path aliases are forbidden",
                "TypeScript alias bypass",
            ),
            require_fixture_rejection(
                fixture_base / "alias-bypass",
                "custom Vite resolve configuration is forbidden",
                "Vite alias bypass",
            ),
            root_route_negative_self_test(),
        ]
        return 0 if all(results) else 1

    root = args.root.resolve()
    violations = scan(root)
    if violations:
        print_violations(root, violations)
        return 1
    print("frontend feature and root-route boundaries passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
