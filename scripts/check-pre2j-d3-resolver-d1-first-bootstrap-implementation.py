#!/usr/bin/env python3
"""Permanent fail-closed checks for resolver D1 first-bootstrap implementation."""

from __future__ import annotations

import argparse
import importlib.util
import json
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
BOOTSTRAP = Path("scripts/mailbox-secret-resolver-d1-bootstrap.py")
RELEASE = Path("scripts/mailbox-secret-resolver-release.py")
MIGRATIONS = Path("migrations/resolver-d1")
QUALITY = Path(".github/workflows/resolver-d1-first-bootstrap.yml")
STATUS = Path("docs/status.json")
IMPLEMENTATION = Path("architecture/pre2j-d3-resolver-d1-first-bootstrap-implementation.json")
ALLOWED_IMPLEMENTATION_PATHS = {
    "architecture/pre2j-d3-resolver-d1-first-bootstrap-implementation.json",
    "scripts/mailbox-secret-resolver-d1-bootstrap.py",
    "scripts/check-pre2j-d3-resolver-d1-first-bootstrap-implementation.py",
    ".github/workflows/resolver-d1-first-bootstrap.yml",
}
FORBIDDEN_UNCHANGED_PATHS = {
    "architecture/pre2j-d3-resolver-d1-first-bootstrap-authority.json",
    "architecture/pre2j-d3-resolver-d1-first-bootstrap-authority-erratum.json",
    "scripts/mailbox-secret-resolver-release.py",
    ".github/workflows/mailbox-secret-resolver-release.yml",
    ".github/workflows/mailbox-secret-resolver-promotion.yml",
    "deploy/cloudflare/mailbox-secret-resolver.wrangler.jsonc",
}
FORBIDDEN_PREFIXES = (
    "apps/mailbox-secret-resolver-worker/",
    "migrations/resolver-d1/",
    "openapi/v1/",
)


def fail(message: str) -> None:
    raise ValueError(message)


def load_module(path: Path, name: str) -> Any:
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    if spec is None or spec.loader is None:
        fail(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def git(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["git", *args], cwd=ROOT, check=check, text=True, capture_output=True)


def read(path: Path) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def status_errors() -> list[str]:
    value = json.loads(read(STATUS))
    return [] if isinstance(value, dict) and value.get("production_ready") is False else [
        "production_ready must remain false"
    ]


def quality_workflow_errors(text: str) -> list[str]:
    errors: list[str] = []
    for marker in (
        "scripts/mailbox-secret-resolver-d1-bootstrap.py",
        "scripts/check-pre2j-d3-resolver-d1-first-bootstrap-implementation.py",
        "Resolver D1 first-bootstrap implementation",
        "FIRST_BOOTSTRAP_BASE",
        '--base-ref "$FIRST_BOOTSTRAP_BASE"',
        "mailbox-secret-resolver-d1-bootstrap.py check",
        "mailbox-secret-resolver-d1-bootstrap.py self-test",
        "mailbox-secret-resolver-d1-bootstrap.py build --development-working-tree",
        "artifacts/mailbox-secret-resolver-d1-first-bootstrap/bootstrap.sql",
    ):
        if marker not in text:
            errors.append(f"first-bootstrap workflow proof is missing {marker!r}")
    return errors


def release_alignment_errors(bootstrap: Any, release: Any) -> list[str]:
    errors: list[str] = []
    bootstrap_names = [path.name for path in bootstrap.validated_migrations(ROOT / MIGRATIONS)]
    try:
        release_names = [path.name for path in release.migration_paths(ROOT)]
        release_digest = release.migration_digest(ROOT)
    except Exception as error:
        return [f"resolver release migration validator rejected canonical inventory: {error}"]
    if release_names != bootstrap_names:
        errors.append("release and first-bootstrap migration order differ")
    if release_digest != bootstrap.migration_manifest_sha256(ROOT / MIGRATIONS):
        errors.append("release and first-bootstrap migration manifest digests differ")
    return errors


def composite_release_alignment_self_test(bootstrap: Any, release: Any) -> None:
    with tempfile.TemporaryDirectory(prefix="resolver-bootstrap-release-alignment-") as temporary:
        root = Path(temporary)
        directory = root / MIGRATIONS
        directory.mkdir(parents=True)
        canonical = directory / "0001_fixture.sql"
        canonical.write_text("SELECT 1;\n", encoding="utf-8")
        if release.migration_digest(root) != bootstrap.migration_manifest_sha256(directory):
            fail("canonical release/bootstrap migration digest alignment failed")
        canonical.rename(directory / "0002_fixture.sql")
        try:
            release.migration_digest(root)
        except Exception:
            pass
        try:
            bootstrap.validated_migrations(directory)
        except Exception:
            return
        fail("strict bootstrap eligibility unexpectedly accepted missing 0001")


def current_errors() -> list[str]:
    errors: list[str] = []
    bootstrap = load_module(BOOTSTRAP, "resolver_first_bootstrap")
    release = load_module(RELEASE, "resolver_release")
    try:
        bootstrap.check_repository_policy(ROOT)
    except Exception as error:
        errors.append(f"first-bootstrap repository policy failed: {error}")
    errors.extend(release_alignment_errors(bootstrap, release))
    errors.extend(quality_workflow_errors(read(QUALITY)))
    errors.extend(status_errors())
    return errors


def diff_errors(base_ref: str) -> list[str]:
    if git("cat-file", "-e", f"{base_ref}^{{commit}}", check=False).returncode != 0:
        return [f"first-bootstrap implementation base ref is unavailable: {base_ref}"]
    changed = {
        path for path in git("diff", "--name-only", base_ref, "--").stdout.splitlines() if path
    }
    base_has_marker = (
        git("cat-file", "-e", f"{base_ref}:{IMPLEMENTATION}", check=False).returncode == 0
    )
    if base_has_marker and str(IMPLEMENTATION) not in changed:
        return []
    errors: list[str] = []
    forbidden = sorted(
        path
        for path in changed
        if path in FORBIDDEN_UNCHANGED_PATHS
        or any(path.startswith(prefix) for prefix in FORBIDDEN_PREFIXES)
    )
    if forbidden:
        errors.append(f"first-bootstrap implementation changed forbidden authority/runtime surface: {forbidden}")
    unexpected = sorted(path for path in changed if path not in ALLOWED_IMPLEMENTATION_PATHS)
    if unexpected:
        errors.append(f"first-bootstrap implementation escaped bounded inventory: {unexpected}")
    for authority in (
        "architecture/pre2j-d3-resolver-d1-first-bootstrap-authority.json",
        "architecture/pre2j-d3-resolver-d1-first-bootstrap-authority-erratum.json",
    ):
        base = git("show", f"{base_ref}:{authority}", check=False)
        if base.returncode != 0 or base.stdout != read(Path(authority)):
            errors.append(f"accepted immutable first-bootstrap authority changed: {authority}")
    return errors


def self_test() -> None:
    errors = current_errors()
    if errors:
        fail("; ".join(errors))
    bootstrap = load_module(BOOTSTRAP, "resolver_first_bootstrap_selftest")
    release = load_module(RELEASE, "resolver_release_selftest")
    bootstrap.self_test()
    composite_release_alignment_self_test(bootstrap, release)
    quality = read(QUALITY)
    if not quality_workflow_errors(quality.replace("FIRST_BOOTSTRAP_BASE", "REMOVED")):
        fail("first-bootstrap workflow base-ref negative fixture unexpectedly passed")
    print("Resolver D1 first-bootstrap implementation negative policy self-test passed.")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-ref")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    errors = current_errors()
    if args.base_ref:
        errors.extend(diff_errors(args.base_ref))
    if errors:
        raise SystemExit("\n".join(errors))
    print("Resolver D1 first-bootstrap implementation is valid and bounded.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
