#!/usr/bin/env python3
"""Fail closed on the current documentation, navigation and local-setup contract.

Owner/consumer: CAP-06 documentation authority; developers and the existing Quality Gate.
Risk: copied mutable order or stale status/setup prose can select unauthorized work after context loss.
Invariant: one binding program document, one linked live pointer, projections/history fail closed.
Tier/lifecycle: cheap repository-source check in the existing required Quality Gate; retire only when
these facts move to an equivalent or stronger natural-owner proof.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import tempfile
from pathlib import Path


PROGRAM = Path("docs/ARCHITECTURE_REBASELINE_V3_PLAN.md")
INDEX = Path("docs/INDEX.md")
AGENT_BOOTSTRAP = Path("AGENTS.md")
NAVIGATION = (
    Path("AGENTS.md"),
    Path("README.md"),
    Path("docs/README.md"),
    Path("docs/INDEX.md"),
    Path("docs/DEVELOPMENT_PLAN.md"),
)
CURRENT_AUTHORITY_DOCS = NAVIGATION + (
    Path("docs/PRODUCT.md"),
    Path("docs/ARCHITECTURE.md"),
    Path("docs/ARCHITECTURE_ACCEPTANCE_PROTOCOL.md"),
    Path("docs/CROSS_COMPONENT_ACCEPTANCE.md"),
    Path("docs/ADR_STATUS.md"),
    Path("docs/DEVELOPER_CAPABILITY_MATRIX.md"),
    Path("docs/TEST_EVIDENCE_INDEX.md"),
)
HISTORICAL_TOMBSTONES = {
    Path("IMPLEMENTATION_PLAN.md"): "Historical Tombstone",
    Path("PROFILE_LIFECYCLE_PLAN.md"): "Historical Tombstone",
    Path("docs/ARCHITECTURE_REBASELINE_V3_AR0.md"): "Historical Tombstone",
    Path("docs/ARCHITECTURE_REBASELINE_V3_AR10.md"): "Historical Tombstone",
    Path("docs/ARCHITECTURE_REBASELINE_V3_AR11.md"): "Historical Tombstone",
    Path("docs/POST_AR11_PRE_AR12_HARDENING_PLAN.md"): "Historical Tombstone",
    Path("docs/PRE2J_ARCHITECTURE_REMEDIATION_PLAN.md"): "Historical Tombstone",
    Path("docs/PRE2J_D3_RESOLVER_BOOTSTRAP_AUTHORITY.md"): "Historical Tombstone",
    Path("docs/PRE2J_C3G_CONTRACT_MIGRATION.md"): "Historical Tombstone",
    Path("docs/CAMOUFOX_RUNTIME_CUTOVER_PLAN.md"): "Historical Tombstone",
    Path("docs/CAMOUHOST_RUNTIME_BUNDLE.md"): "Historical Tombstone",
    Path("docs/WINDOWS_BRIDGE_FEASIBILITY.md"): "Historical Tombstone",
    Path("docs/PHASE2H_UI_GAP_INVENTORY.md"): "Historical Tombstone",
    Path("docs/DEVELOPER_CAPABILITY_MATRIX.md"): "Historical Tombstone",
    Path("docs/AR8_STAGING_PROVIDER_BOOTSTRAP.md"): "Historical Tombstone",
    Path("docs/mailbox-composition.md"): "Historical Snapshot",
}
PROGRAM_TOKEN = re.compile(r"\b(?:D0|C0|A0|S0|E\d+|P\d+|V\d+|R\d+)\b")
LOCAL_LINK = re.compile(r"(?<!!)\[[^\]]*\]\(([^)]+)\)")
SHA40 = re.compile(r"\b[0-9a-f]{40}\b")
FORBIDDEN_CURRENT_CLAIMS = (
    re.compile(r"status\.json\s+(?:is|remains)\s+(?:the\s+)?(?:current\s+)?authoritative", re.I),
    re.compile(r"current factual status (?:is )?(?:stored|lives) in.{0,40}status\.json", re.I),
    re.compile(r"текущ(?:ий|его|ая|ие).{0,40}(?:статус|факт).{0,60}status\.json", re.I),
    re.compile(r"current execution order (?:lives|is|resides) in.{0,50}DEVELOPMENT_PLAN", re.I),
    re.compile(r"DEVELOPMENT_PLAN\.md.{0,60}(?:only normative|current execution order)", re.I),
)


def read(root: Path, relative: Path, errors: list[str]) -> str:
    path = root / relative
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        errors.append(f"missing/unreadable documentation owner {relative}: {error}")
        return ""


def tracked_markdown(root: Path) -> list[Path]:
    completed = subprocess.run(
        ["git", "ls-files", "--", "*.md"],
        cwd=root,
        check=False,
        capture_output=True,
        text=True,
    )
    if completed.returncode == 0:
        return [Path(line) for line in completed.stdout.splitlines() if line]
    return [path.relative_to(root) for path in root.rglob("*.md") if ".git" not in path.parts]


def check_links(root: Path, files: list[Path]) -> list[str]:
    errors: list[str] = []
    for relative in files:
        path = root / relative
        try:
            text = path.read_text(encoding="utf-8")
        except OSError as error:
            errors.append(f"cannot read tracked Markdown {relative}: {error}")
            continue
        for match in LOCAL_LINK.finditer(text):
            raw = match.group(1).strip()
            if raw.startswith("<") and raw.endswith(">"):
                raw = raw[1:-1]
            target = raw.split("#", 1)[0].strip().replace("%20", " ")
            if not target or "://" in target or target.startswith(("mailto:", "app:")):
                continue
            resolved = (path.parent / target).resolve()
            if not resolved.exists():
                line = text.count("\n", 0, match.start()) + 1
                errors.append(f"broken local Markdown link {relative}:{line}: {raw}")
    return errors


def check(root: Path, markdown_files: list[Path] | None = None) -> list[str]:
    errors: list[str] = []
    program = read(root, PROGRAM, errors)
    index = read(root, INDEX, errors)
    agents = read(root, AGENT_BOOTSTRAP, errors)

    if "CURRENT TEMPORARY EXECUTION AUTHORITY" not in program or "Binding execution order" not in program:
        errors.append(f"{PROGRAM} must remain the single explicit temporary program/order owner")
    if "Issue #266" not in program:
        errors.append(f"{PROGRAM} must link the sole live transaction pointer #266")
    for marker in ("Permanent normative knowledge", "Temporary execution authority", "Live state", "Projection/navigation", "History/provenance"):
        if marker not in index:
            errors.append(f"{INDEX} is missing authority class: {marker}")
    for marker in ("docs/INDEX.md", "docs/ARCHITECTURE_REBASELINE_V3_PLAN.md", "Issue #266"):
        if marker not in agents:
            errors.append(f"{AGENT_BOOTSTRAP} bootstrap is missing: {marker}")

    for relative in NAVIGATION:
        text = read(root, relative, errors)
        tokens = PROGRAM_TOKEN.findall(text)
        if len(tokens) >= 4:
            errors.append(
                f"{relative} copies {len(tokens)} program-stage tokens; order belongs only to {PROGRAM}"
            )
        if SHA40.search(text):
            errors.append(f"{relative} embeds a moving 40-character SHA")

    for relative in CURRENT_AUTHORITY_DOCS:
        text = read(root, relative, errors)
        for pattern in FORBIDDEN_CURRENT_CLAIMS:
            if pattern.search(text):
                errors.append(f"{relative} contains forbidden current/status authority claim: {pattern.pattern}")

    for relative, marker in HISTORICAL_TOMBSTONES.items():
        text = read(root, relative, errors)
        fail_closed = text.lower()
        if marker not in text or not any(
            status in fail_closed for status in ("not_current", "superseded", "no current")
        ):
            errors.append(f"{relative} must fail closed as a historical/superseded tombstone")

    ops_readme = read(root, Path("tools/opsctl/README.md"), errors)
    for marker in ("src/help.txt", "OPSCTL_ARCHITECTURE_BOUNDARY.md", "ReadOnlyMetadata"):
        if marker not in ops_readme:
            errors.append(f"tools/opsctl/README.md is missing executable-boundary marker: {marker}")

    try:
        package = json.loads((root / "frontend/package.json").read_text(encoding="utf-8"))
        node = package["engines"]["node"]
        npm = package["engines"]["npm"]
        nvm = (root / "frontend/.nvmrc").read_text(encoding="utf-8").strip()
        toolchain = (root / "rust-toolchain.toml").read_text(encoding="utf-8")
        contributing = read(root, Path("CONTRIBUTING.md"), errors)
        for label, value in (("Node", node), ("npm", npm), (".nvmrc", nvm)):
            if value not in contributing:
                errors.append(f"CONTRIBUTING.md does not document exact {label} version {value}")
        rust_match = re.search(r'channel\s*=\s*"([^"]+)"', toolchain)
        if rust_match and rust_match.group(1) not in contributing:
            errors.append(f"CONTRIBUTING.md does not document exact Rust {rust_match.group(1)}")
        if "Python" not in contributing or "3.12" not in contributing:
            errors.append("CONTRIBUTING.md does not document the Python 3.12 requirement")
    except (OSError, KeyError, json.JSONDecodeError) as error:
        errors.append(f"cannot validate documented local toolchain pins: {error}")

    errors.extend(check_links(root, markdown_files or tracked_markdown(root)))
    return errors


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="documentation-authority-") as directory:
        root = Path(directory)
        (root / "docs").mkdir()
        good = "[ok](target.md)\n"
        (root / "target.md").write_text("ok\n", encoding="utf-8")
        (root / "source.md").write_text(good, encoding="utf-8")
        if check_links(root, [Path("source.md")]):
            raise AssertionError("valid local link was rejected")
        (root / "source.md").write_text("[bad](missing.md)\n", encoding="utf-8")
        if not check_links(root, [Path("source.md")]):
            raise AssertionError("broken local link was accepted")

    copied = "E0 -> E1 -> E2 -> E3"
    if len(PROGRAM_TOKEN.findall(copied)) < 4:
        raise AssertionError("copied program sequence was not detected")
    if not any(pattern.search("status.json remains authoritative") for pattern in FORBIDDEN_CURRENT_CLAIMS):
        raise AssertionError("positive status.json authority claim was not detected")
    if any(pattern.search("status.json is a projection, not current authority") for pattern in FORBIDDEN_CURRENT_CLAIMS):
        raise AssertionError("negative status.json authority rule was rejected")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("Documentation authority negative fixtures passed.")
        return 0
    errors = check(args.root.resolve())
    if errors:
        for error in errors:
            print(error)
        return 1
    print("Documentation authority, setup pins, tombstones and tracked local links are consistent.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
