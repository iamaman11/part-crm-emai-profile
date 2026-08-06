#!/usr/bin/env python3
"""Permanent policy checks for Repository Step 10 certification contracts."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

CRATE = Path("crates/certification-domain")

REQUIRED_FRAGMENTS = (
    "CertificationPolicy",
    "SignalRequirement",
    "evaluate_certification",
    "MatrixDigest",
    "CertificationOutcome::Prohibited",
    "required_rules",
    "DeviceAuthorizationRegistry",
    "DeviceGrantEvent",
    "pub fn history(&self)",
    "authorize_unwrap",
    "StaleGrantVersion",
    "GrantRevoked",
    "PreverifiedSignatureEvidence",
    "VerificationEvidenceMismatch",
    "fn approves(",
    "UpdateController",
    "AwaitingHealth",
    "UpdateState::Failed",
    "RollbackOutcome",
    "matches_identity",
    "fail_health_and_rollback",
    "render_metadata_only",
)

FORBIDDEN_FRAGMENTS = (
    "unsafe {",
    "println!",
    "eprintln!",
    "dbg!",
    "raw_signal_value",
    "raw_signal_values",
    "temp/browser_profiles",
    "temp\\browser_profiles",
    "windows::",
    "worker::",
    "reqwest::",
    "#[derive(Clone, Debug, Eq, PartialEq)]\npub struct ObservationSet",
    "matrix_digest={}",
    "RollbackUnavailable",
)


def check(root: Path) -> list[str]:
    failures: list[str] = []
    crate = root / CRATE
    sources = sorted((crate / "src").glob("*.rs"))
    if not sources:
        return [f"missing certification sources below {crate / 'src'}"]
    source_text = "\n".join(path.read_text(encoding="utf-8") for path in sources)

    for fragment in REQUIRED_FRAGMENTS:
        if fragment not in source_text:
            failures.append(f"missing required Step 10 boundary: {fragment}")
    for fragment in FORBIDDEN_FRAGMENTS:
        if fragment in source_text:
            failures.append(f"forbidden Step 10 source fragment: {fragment}")

    if root.resolve() == Path.cwd().resolve():
        workspace = root / "Cargo.toml"
        if not workspace.is_file():
            failures.append("missing workspace Cargo.toml")
        else:
            workspace_text = workspace.read_text(encoding="utf-8")
            if '"crates/certification-domain"' not in workspace_text:
                failures.append("certification-domain is not a workspace member")
            if 'certification-domain = { path = "crates/certification-domain" }' not in workspace_text:
                failures.append("certification-domain workspace dependency is missing")
            if 'sha2 = { version = "=0.11.0"' not in workspace_text:
                failures.append("exact SHA-256 dependency pin is missing")

        bootstrap = sorted((root / ".github/workflows").glob("step10-*-bootstrap.yml"))
        if bootstrap:
            failures.append("temporary Step 10 bootstrap workflows must be removed")

    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    args = parser.parse_args()

    failures = check(args.root.resolve())
    if failures:
        for failure in failures:
            print(f"ERROR: {failure}", file=sys.stderr)
        return 1
    print("Repository Step 10 certification policy passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
