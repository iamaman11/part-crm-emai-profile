#!/usr/bin/env python3
"""Permanent policy for immutable encrypted-generation objects in production R2."""

from __future__ import annotations

import argparse
from pathlib import Path

SOURCE = Path("crates/cloudflare-adapters/src/r2_generation_objects.rs")

REQUIRED_PRODUCTION_FRAGMENTS = (
    "GenerationObjectUploadPort",
    "GenerationObjectExactVerifyPort",
    "META_TENANT_ID",
    "META_PROFILE_ID",
    "META_GENERATION_ID",
    "META_METADATA_DIGEST",
    "META_CONTAINER_DIGEST",
    '"tenants/{}/profiles/{profile_id}/generations/{generation_id}.bpgc"',
    ".custom_metadata(",
    ".sha256(",
    "etag_does_not_match: Some(\"*\".to_owned())",
    "stored.custom_metadata()",
    "stored.checksum().sha256",
    "GenerationObjectUploadOutcome::Created",
    "GenerationObjectUploadOutcome::Idempotent",
    "GenerationObjectUploadOutcome::ImmutableConflict",
)

FORBIDDEN_PRODUCTION_FRAGMENTS = (
    ".delete(",
    "delete_generation_object",
    "overwrite_generation_object",
)


def function_body(source: str, marker: str) -> str:
    start = source.find(marker)
    if start < 0:
        return ""
    opening = source.find("{", start)
    if opening < 0:
        return ""
    depth = 0
    for index in range(opening, len(source)):
        character = source[index]
        if character == "{":
            depth += 1
        elif character == "}":
            depth -= 1
            if depth == 0:
                return source[opening : index + 1]
    return ""


def check_text(source: str) -> list[str]:
    production = source.split("#[cfg(test)]", 1)[0]
    failures: list[str] = []

    for fragment in REQUIRED_PRODUCTION_FRAGMENTS:
        if fragment not in production:
            failures.append(f"missing immutable R2 invariant: {fragment}")
    for fragment in FORBIDDEN_PRODUCTION_FRAGMENTS:
        if fragment in production:
            failures.append(f"forbidden immutable R2 operation: {fragment}")

    upload = function_body(
        production,
        "async fn put_generation_object_if_absent",
    )
    if not upload:
        failures.append("missing production R2 upload function body")
        return failures

    put = upload.find(".put(")
    checksum = upload.find(".sha256(")
    conditional = upload.find(".only_if(")
    execute = upload.find(".execute()")
    created = upload.find("GenerationObjectUploadOutcome::Created")
    head = upload.find(".head_exact(")
    compare = upload.find("Self::object_matches(")
    idempotent = upload.find("GenerationObjectUploadOutcome::Idempotent")
    conflict = upload.find("GenerationObjectUploadOutcome::ImmutableConflict")

    if upload.count(".put(") != 1:
        failures.append("immutable R2 upload must contain exactly one PUT")
    if min(put, checksum, conditional, execute, created, head, compare, idempotent, conflict) < 0:
        failures.append(
            "immutable R2 upload must preserve PUT/checksum/conditional/create/HEAD/exact-compare outcomes"
        )
    elif not (
        put < checksum < conditional < execute < created < head < compare < idempotent < conflict
    ):
        failures.append(
            "immutable R2 upload order must be create-only PUT -> created gate -> HEAD -> exact compare"
        )

    conditional_body = function_body(upload, ".only_if(Conditional")
    if conditional_body:
        # Conditional is a struct literal, so the generic brace scanner can validate its payload.
        if 'etag_does_not_match: Some("*".to_owned())' not in conditional_body:
            failures.append("immutable R2 PUT must require wildcard object-absence condition")
    elif 'etag_does_not_match: Some("*".to_owned())' not in upload:
        failures.append("immutable R2 PUT must require wildcard object-absence condition")

    if ".head_exact(" in upload[:put]:
        failures.append("immutable R2 create-only semantics must not read before PUT")

    verifier = function_body(production, "async fn verify_generation_object_exact")
    if not verifier or ".head_exact(" not in verifier or "Self::object_matches(" not in verifier:
        failures.append("exact R2 verifier must HEAD and compare the immutable object descriptor")

    matches = function_body(production, "fn object_matches")
    for fragment in (
        "stored.key()",
        "stored.size()",
        "stored.custom_metadata()",
        "META_TENANT_ID",
        "META_PROFILE_ID",
        "META_GENERATION_ID",
        "META_METADATA_DIGEST",
        "META_CONTAINER_DIGEST",
        "stored.checksum().sha256",
    ):
        if fragment not in matches:
            failures.append(f"exact R2 comparison is missing: {fragment}")

    return failures


def check(root: Path) -> list[str]:
    source_path = root / SOURCE
    if not source_path.is_file():
        return [f"missing production immutable R2 adapter: {SOURCE}"]
    return check_text(source_path.read_text(encoding="utf-8"))


def self_test(root: Path) -> list[str]:
    source_path = root / SOURCE
    if not source_path.is_file():
        return [f"missing production immutable R2 adapter: {SOURCE}"]
    production = source_path.read_text(encoding="utf-8")
    fixture = production.replace(".only_if(Conditional {", ".conditional_removed(Conditional {", 1)
    failures = check_text(fixture)
    if not any("PUT/checksum/conditional" in failure for failure in failures):
        return ["R2 missing-conditional negative fixture unexpectedly passed"]
    return []


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    root = args.root.resolve()

    failures = self_test(root) if args.self_test else check(root)
    if failures:
        for failure in failures:
            print(f"ERROR: {failure}")
        return 1

    if args.self_test:
        print("Immutable R2 negative fixture was rejected.")
    else:
        print("Immutable R2 generation object policy passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
