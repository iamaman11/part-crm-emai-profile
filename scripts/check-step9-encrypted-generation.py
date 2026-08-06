#!/usr/bin/env python3
"""Permanent policy checks for Repository Step 9 encrypted generations."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

CRATE = Path("crates/encrypted-generation-domain")

REQUIRED_FRAGMENTS = (
    "XChaCha20Poly1305",
    "RECORD_FINAL",
    "record_aad",
    "EncryptedGenerationError::NonceReuse",
    "put_immutable",
    "commit_current",
    "rollback",
    "plan_orphans",
    "render_metadata_only",
    "zeroize",
    "Zeroizing",
    "nonce_domain",
    "impl Drop for NonceDomain",
    "let mut plaintext = Zeroizing::new(Vec::new())",
    "wrong_key_does_not_quarantine_unchanged_object",
    "malformed_headers_and_trailing_bytes_fail_closed",
)

FORBIDDEN_FRAGMENTS = (
    "temp/browser_profiles",
    "temp\\browser_profiles",
    "rand::",
    "getrandom::",
    "println!",
    "eprintln!",
    "dbg!",
)

SENSITIVE_DEBUG = re.compile(
    r"#\[derive\([^\]]*Debug[^\]]*\)\]\s*(?:pub(?:\(crate\))?\s+)?struct\s+"
    r"(?:GenerationDek|OpenedGeneration|RestoreResult|NonceDomain)",
    re.MULTILINE,
)
GENERATION_DEK_BYTES = re.compile(
    r"impl\s+GenerationDek\s*\{(?:(?!\n\}).)*pub\s+(?:const\s+)?fn\s+bytes\s*\(",
    re.DOTALL,
)


def check(root: Path) -> list[str]:
    failures: list[str] = []
    crate = root / CRATE
    sources = sorted((crate / "src").glob("*.rs"))
    if not sources:
        return [f"missing encrypted generation sources below {crate / 'src'}"]
    source_text = "\n".join(path.read_text(encoding="utf-8") for path in sources)

    for fragment in REQUIRED_FRAGMENTS:
        if fragment not in source_text:
            failures.append(f"missing required Step 9 boundary: {fragment}")
    for fragment in FORBIDDEN_FRAGMENTS:
        if fragment in source_text:
            failures.append(f"forbidden Step 9 source fragment: {fragment}")
    if SENSITIVE_DEBUG.search(source_text):
        failures.append("key and plaintext-bearing types must not derive Debug")
    if GENERATION_DEK_BYTES.search(source_text):
        failures.append("GenerationDek must not expose key bytes")

    workspace = root / "Cargo.toml"
    if not workspace.is_file():
        failures.append("missing workspace Cargo.toml")
    else:
        workspace_text = workspace.read_text(encoding="utf-8")
        exact_pins = (
            'chacha20poly1305 = { version = "=0.11.0"',
            'sha2 = { version = "=0.11.0"',
            'zeroize = { version = "=1.9.0"',
        )
        for pin in exact_pins:
            if pin not in workspace_text:
                failures.append(f"missing exact crypto dependency pin: {pin}")

    bootstrap_workflows = sorted((root / ".github/workflows").glob("step9-*.yml"))
    if bootstrap_workflows:
        failures.append("temporary Step 9 workflows must be removed")

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
    print("Repository Step 9 encrypted generation policy passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
