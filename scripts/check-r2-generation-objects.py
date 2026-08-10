#!/usr/bin/env python3
"""Permanent policy for immutable encrypted-generation objects and exact R2 upload capabilities."""

from __future__ import annotations

import argparse
from pathlib import Path

SOURCE = Path("crates/cloudflare-adapters/src/r2_generation_objects.rs")
CAPABILITY_SOURCE = Path(
    "crates/cloudflare-adapters/src/r2_generation_upload_capability.rs"
)
ADAPTER_LIB = Path("crates/cloudflare-adapters/src/lib.rs")

REQUIRED_PRODUCTION_FRAGMENTS = (
    "GenerationObjectUploadPort",
    "GenerationObjectExactVerifyPort",
    "GenerationObjectDescriptorVerifyPort",
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

CAPABILITY_HEADERS = (
    "if-none-match",
    "x-amz-checksum-sha256",
    "x-amz-meta-container-bytes",
    "x-amz-meta-container-digest",
    "x-amz-meta-generation-id",
    "x-amz-meta-metadata-digest",
    "x-amz-meta-profile-id",
    "x-amz-meta-tenant-id",
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


def verifier_uses_head_and_compare(source: str, marker: str) -> bool:
    verifier = function_body(source, marker)
    return bool(
        verifier
        and ".head_descriptor(" in verifier
        and "Self::descriptor_matches(" in verifier
    )


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
    head = upload.find(".head_descriptor(")
    compare = upload.find("Self::descriptor_matches(")
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
        if 'etag_does_not_match: Some("*".to_owned())' not in conditional_body:
            failures.append("immutable R2 PUT must require wildcard object-absence condition")
    elif 'etag_does_not_match: Some("*".to_owned())' not in upload:
        failures.append("immutable R2 PUT must require wildcard object-absence condition")

    if ".head_descriptor(" in upload[:put]:
        failures.append("immutable R2 create-only semantics must not read before PUT")

    if not verifier_uses_head_and_compare(production, "async fn verify_generation_object_exact"):
        failures.append("exact R2 object verifier must HEAD and compare the immutable descriptor")
    if not verifier_uses_head_and_compare(
        production, "async fn verify_generation_object_descriptor_exact"
    ):
        failures.append("metadata-only R2 descriptor verifier must HEAD and compare exactly")

    matches = function_body(production, "fn descriptor_matches")
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
        "decode_sha256_hex(descriptor.container_digest())",
    ):
        if fragment not in matches:
            failures.append(f"exact R2 comparison is missing: {fragment}")

    return failures


def check_capability_text(source: str) -> list[str]:
    production = source.split("#[cfg(test)]", 1)[0]
    failures: list[str] = []

    for fragment in (
        "const MAX_EXPIRES_SECONDS: u32 = 300;",
        "pub fn sign_put(",
        "expires_seconds == 0 || expires_seconds > MAX_EXPIRES_SECONDS",
        '"tenants/{}/profiles/{}/generations/{}.bpgc"',
        "descriptor.object_key() != canonical_key",
        "uri_encode(descriptor.object_key(), true)",
        '("if-none-match".to_owned(), "*".to_owned())',
        '"x-amz-checksum-sha256".to_owned()',
    ):
        if fragment not in production:
            failures.append(f"missing exact R2 upload capability invariant: {fragment}")

    sign_put = function_body(production, "pub fn sign_put(")
    if not sign_put:
        failures.append("missing exact R2 upload capability signer body")
        return failures

    signed_headers_marker = "let signed_headers = "
    signed_headers_start = sign_put.find(signed_headers_marker)
    signed_headers = (
        sign_put[signed_headers_start:] if signed_headers_start >= 0 else ""
    )
    for header in CAPABILITY_HEADERS:
        if f'"{header}"' not in sign_put:
            failures.append(f"R2 upload capability does not return required header: {header}")
        if header not in signed_headers:
            failures.append(f"R2 upload capability does not sign required header: {header}")

    canonical_key = function_body(production, "fn validate_descriptor(")
    if (
        '"tenants/{}/profiles/{}/generations/{}.bpgc"' not in canonical_key
        or "descriptor.object_key() != canonical_key" not in canonical_key
    ):
        failures.append("R2 upload capability must validate the exact canonical object key")

    canonical_uri = sign_put.find("let canonical_uri = format!(")
    exact_descriptor_key = sign_put.find("uri_encode(descriptor.object_key(), true)")
    if canonical_uri < 0 or exact_descriptor_key < canonical_uri:
        failures.append("R2 upload capability URI must be built from the exact descriptor object key")

    capability = function_body(production, "pub struct R2GenerationUploadCapability")
    if not capability:
        failures.append("missing bounded R2 upload capability response type")
    else:
        for secret_fragment in ("credentials", "access_key", "secret_access_key"):
            if secret_fragment in capability:
                failures.append(
                    f"R2 upload capability response must not expose signer secret material: {secret_fragment}"
                )

    if "MAX_EXPIRES_SECONDS: u32 = 300" not in production:
        failures.append("R2 upload capability TTL policy must remain capped at 300 seconds")
    if "expires_seconds > MAX_EXPIRES_SECONDS" not in sign_put:
        failures.append("R2 upload capability signer must reject TTL above policy")

    return failures


def check(root: Path) -> list[str]:
    source_path = root / SOURCE
    capability_path = root / CAPABILITY_SOURCE
    adapter_lib_path = root / ADAPTER_LIB
    failures: list[str] = []
    if not source_path.is_file():
        failures.append(f"missing production immutable R2 adapter: {SOURCE}")
    else:
        failures.extend(check_text(source_path.read_text(encoding="utf-8")))
    if not capability_path.is_file():
        failures.append(f"missing exact R2 upload capability signer: {CAPABILITY_SOURCE}")
    else:
        failures.extend(
            check_capability_text(capability_path.read_text(encoding="utf-8"))
        )
    if not adapter_lib_path.is_file():
        failures.append(f"missing Cloudflare adapter library: {ADAPTER_LIB}")
    elif "pub mod r2_generation_upload_capability;" not in adapter_lib_path.read_text(
        encoding="utf-8"
    ):
        failures.append("R2 upload capability signer must be exported by cloudflare-adapters")
    return failures


def self_test(root: Path) -> list[str]:
    source_path = root / SOURCE
    capability_path = root / CAPABILITY_SOURCE
    if not source_path.is_file():
        return [f"missing production immutable R2 adapter: {SOURCE}"]
    if not capability_path.is_file():
        return [f"missing exact R2 upload capability signer: {CAPABILITY_SOURCE}"]

    production = source_path.read_text(encoding="utf-8")
    fixture = production.replace(".only_if(Conditional {", ".conditional_removed(Conditional {", 1)
    failures = check_text(fixture)
    if not any("PUT/checksum/conditional" in failure for failure in failures):
        return ["R2 missing-conditional negative fixture unexpectedly passed"]

    descriptor_fixture = production.replace(
        ".head_descriptor(scope, descriptor)", ".descriptor_head_removed(scope, descriptor)", 1
    )
    descriptor_failures = check_text(descriptor_fixture)
    if not any("metadata-only R2 descriptor verifier" in failure for failure in descriptor_failures):
        return ["R2 missing-descriptor-HEAD negative fixture unexpectedly passed"]

    capability = capability_path.read_text(encoding="utf-8")
    negative_fixtures = (
        (
            "TTL cap",
            capability.replace(
                "const MAX_EXPIRES_SECONDS: u32 = 300;",
                "const MAX_EXPIRES_SECONDS: u32 = 301;",
                1,
            ),
            "TTL policy",
        ),
        (
            "TTL enforcement",
            capability.replace(
                "expires_seconds > MAX_EXPIRES_SECONDS",
                "expires_seconds > u32::MAX",
                1,
            ),
            "reject TTL",
        ),
        (
            "create-only header",
            capability.replace('"if-none-match".to_owned()', '"if-match".to_owned()', 1),
            "if-none-match",
        ),
        (
            "checksum header",
            capability.replace(
                '"x-amz-checksum-sha256".to_owned()',
                '"x-amz-checksum-removed".to_owned()',
                1,
            ),
            "x-amz-checksum-sha256",
        ),
        (
            "tenant metadata",
            capability.replace(
                '"x-amz-meta-tenant-id".to_owned()',
                '"x-amz-meta-tenant-removed".to_owned()',
                1,
            ),
            "x-amz-meta-tenant-id",
        ),
        (
            "exact object key",
            capability.replace(
                "uri_encode(descriptor.object_key(), true)",
                'uri_encode("tenants/", true)',
                1,
            ),
            "exact descriptor object key",
        ),
    )
    for label, fixture_text, expected in negative_fixtures:
        fixture_failures = check_capability_text(fixture_text)
        if not any(expected in failure for failure in fixture_failures):
            return [f"R2 upload capability {label} negative fixture unexpectedly passed"]

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
        print("Immutable R2 and exact upload capability negative fixtures were rejected.")
    else:
        print("Immutable R2 generation object and exact upload capability policy passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
