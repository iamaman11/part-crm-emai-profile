#!/usr/bin/env python3
"""Fail closed if Phase 2B protected client-contact persistence regresses."""

from __future__ import annotations

import argparse
import re
import tempfile
from pathlib import Path

MIGRATION = Path("migrations/d1/0014_client_contact_protection.sql")
D1_TEST = Path("scripts/test-phase2b-client-contacts.py")
LOOKUP_PORT = Path("crates/application-ports/src/client_contact_lookup.rs")
LOOKUP_ADAPTER = Path("crates/cloudflare-adapters/src/contact_lookup.rs")

REQUIRED_TABLE_MARKERS = (
    "CREATE TABLE client_contact_points",
    "ciphertext BLOB NOT NULL",
    "nonce BLOB NOT NULL",
    "exact_lookup_token BLOB NOT NULL",
    "CHECK(length(exact_lookup_token) = 32)",
    "encryption_key_version INTEGER NOT NULL",
    "lookup_key_version INTEGER NOT NULL",
    "normalization_version INTEGER NOT NULL",
    "protection_version INTEGER NOT NULL",
    "PRIMARY KEY (tenant_id, contact_point_id)",
    "REFERENCES clients(tenant_id, client_id) ON DELETE RESTRICT",
)
REQUIRED_GUARDS = (
    "client_contact_active_client_insert_guard",
    "client_contact_update_guard",
    "client_contact_identity_immutable",
    "client_contact_archived_immutable",
    "client_contact_client_not_active",
    "client_contact_updated_at_rewind",
    "client_contact_delete_guard",
    "client_contact_delete_forbidden",
)
REQUIRED_LOOKUP_PORT_MARKERS = (
    "pub trait ContactLookupProtectionPort",
    "derive_exact_lookup_candidates",
    "pub trait ContactExactLookupRepositoryPort",
    "find_active_contacts_by_exact_lookup",
    "token: &ExactLookupToken",
)
REQUIRED_LOOKUP_ADAPTER_MARKERS = (
    "impl ContactExactLookupRepositoryPort for D1ContactExactLookupRepository",
    "WHERE tenant_id = ?",
    "AND kind = ?",
    "AND normalization_version = ?",
    "AND lookup_key_version = ?",
    "AND exact_lookup_token = ?",
    "AND status = 'ACTIVE'",
    "token.key_version().value()",
    "token.bytes().as_slice()",
    ".all()",
    ".results::<ExactLookupRow>()",
    "impl<N: ContactNonceSource> ContactLookupProtectionPort",
    "derive_lookup_candidates(request.tenant_id(), request.hmac_input())",
)
FORBIDDEN_LOOKUP_ADAPTER_MARKERS = (
    "decrypt_contact_display(",
    "normalize_contact_value(",
    "SELECT ciphertext",
    "SELECT nonce",
    "SELECT exact_lookup_token",
)
FORBIDDEN_COLUMNS = {
    "value",
    "display_value",
    "normalized_value",
    "email",
    "phone",
    "url",
    "plaintext",
    "plaintext_value",
    "raw_contact",
}


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def table_body(source: str, table: str) -> str:
    match = re.search(
        rf"CREATE\s+TABLE\s+{re.escape(table)}\s*\((.*?)\)\s*STRICT\s*;",
        source,
        flags=re.IGNORECASE | re.DOTALL,
    )
    return "" if match is None else match.group(1)


def declared_columns(body: str) -> set[str]:
    columns: set[str] = set()
    for raw_line in body.splitlines():
        line = raw_line.strip()
        if not line or line.startswith(("PRIMARY ", "FOREIGN ", "UNIQUE ", "CHECK(")):
            continue
        match = re.match(r"([A-Za-z_][A-Za-z0-9_]*)\s+(TEXT|BLOB|INTEGER|REAL)\b", line)
        if match:
            columns.add(match.group(1).lower())
    return columns


def exact_lookup_index(source: str) -> str:
    match = re.search(
        r"CREATE\s+INDEX\s+client_contact_exact_lookup\s+ON\s+client_contact_points\s*"
        r"\((.*?)\)\s*WHERE\s+status\s*=\s*'ACTIVE'\s*;",
        source,
        flags=re.IGNORECASE | re.DOTALL,
    )
    return "" if match is None else match.group(1)


def validate(root: Path) -> list[str]:
    errors: list[str] = []
    migration = root / MIGRATION
    d1_test = root / D1_TEST
    lookup_port = root / LOOKUP_PORT
    lookup_adapter = root / LOOKUP_ADAPTER
    if not migration.is_file():
        return [f"missing Phase 2B migration: {MIGRATION}"]
    if not d1_test.is_file() or not read(d1_test).strip():
        errors.append(f"missing Phase 2B D1 invariant proof: {D1_TEST}")
    if not lookup_port.is_file() or not read(lookup_port).strip():
        errors.append(f"missing Phase 2B exact lookup port: {LOOKUP_PORT}")
    if not lookup_adapter.is_file() or not read(lookup_adapter).strip():
        errors.append(f"missing Phase 2B exact lookup adapter: {LOOKUP_ADAPTER}")

    source = read(migration)
    for marker in REQUIRED_TABLE_MARKERS + REQUIRED_GUARDS:
        if marker not in source:
            errors.append(f"Phase 2B migration missing required protected-D1 marker `{marker}`")

    body = table_body(source, "client_contact_points")
    if not body:
        errors.append("client_contact_points must be a STRICT table")
        return errors

    columns = declared_columns(body)
    overlap = sorted(columns.intersection(FORBIDDEN_COLUMNS))
    if overlap:
        errors.append(f"plaintext-capable client contact columns are forbidden: {overlap}")

    required_columns = {
        "tenant_id",
        "client_id",
        "contact_point_id",
        "kind",
        "status",
        "normalization_version",
        "protection_version",
        "ciphertext",
        "nonce",
        "encryption_key_version",
        "exact_lookup_token",
        "lookup_key_version",
        "created_by_actor_id",
        "updated_by_actor_id",
        "created_at_ms",
        "updated_at_ms",
    }
    missing = sorted(required_columns - columns)
    if missing:
        errors.append(f"protected client contact schema is missing columns: {missing}")

    index = exact_lookup_index(source)
    if not index:
        errors.append("tenant-scoped active exact-contact lookup index is missing")
    else:
        normalized = " ".join(index.replace("\n", " ").split()).lower()
        ordered = (
            "tenant_id",
            "kind",
            "normalization_version",
            "lookup_key_version",
            "exact_lookup_token",
            "contact_point_id",
        )
        cursor = -1
        for column in ordered:
            next_cursor = normalized.find(column, cursor + 1)
            if next_cursor < 0:
                errors.append(f"exact-contact lookup index missing `{column}`")
                break
            cursor = next_cursor
        if not normalized.lstrip().startswith("tenant_id"):
            errors.append("exact-contact lookup index must lead with tenant_id")

    lower = source.lower()
    if "unique index client_contact_exact_lookup" in lower:
        errors.append("exact lookup token must not become a global uniqueness identity")
    if " on client_contact_points(exact_lookup_token" in lower:
        errors.append("unscoped exact-lookup-token index is forbidden")
    if " on client_contact_points(lookup_key_version" in lower:
        errors.append("lookup index must be tenant-scoped before key/token dimensions")

    test_source = read(d1_test) if d1_test.is_file() else ""
    for marker in (
        "test_schema_has_no_plaintext_contact_column",
        "test_tenant_and_client_scope_are_structural",
        "test_exact_lookup_is_index_backed",
        "test_archival_is_one_way_and_delete_is_forbidden",
        "test_active_contact_requires_active_client",
    ):
        if marker not in test_source:
            errors.append(f"Phase 2B D1 proof missing `{marker}`")

    lookup_port_source = read(lookup_port) if lookup_port.is_file() else ""
    for marker in REQUIRED_LOOKUP_PORT_MARKERS:
        if marker not in lookup_port_source:
            errors.append(f"Phase 2B exact lookup port missing `{marker}`")

    lookup_source = read(lookup_adapter) if lookup_adapter.is_file() else ""
    for marker in REQUIRED_LOOKUP_ADAPTER_MARKERS:
        if marker not in lookup_source:
            errors.append(f"Phase 2B exact lookup adapter missing `{marker}`")
    for marker in FORBIDDEN_LOOKUP_ADAPTER_MARKERS:
        if marker in lookup_source:
            errors.append(f"Phase 2B exact lookup adapter must not use `{marker}`")

    return errors


def write_fixture(root: Path) -> None:
    migration = root / MIGRATION
    migration.parent.mkdir(parents=True, exist_ok=True)
    migration.write_text(
        """CREATE TABLE client_contact_points (
 tenant_id TEXT NOT NULL,
 client_id TEXT NOT NULL,
 contact_point_id TEXT NOT NULL,
 kind TEXT NOT NULL,
 status TEXT NOT NULL,
 normalization_version INTEGER NOT NULL,
 protection_version INTEGER NOT NULL,
 ciphertext BLOB NOT NULL CHECK(length(ciphertext) BETWEEN 1 AND 4096),
 nonce BLOB NOT NULL CHECK(length(nonce) BETWEEN 1 AND 64),
 encryption_key_version INTEGER NOT NULL,
 exact_lookup_token BLOB NOT NULL CHECK(length(exact_lookup_token) = 32),
 lookup_key_version INTEGER NOT NULL,
 created_by_actor_id TEXT NOT NULL,
 updated_by_actor_id TEXT NOT NULL,
 created_at_ms INTEGER NOT NULL,
 updated_at_ms INTEGER NOT NULL,
 PRIMARY KEY (tenant_id, contact_point_id),
 FOREIGN KEY (tenant_id, client_id) REFERENCES clients(tenant_id, client_id) ON DELETE RESTRICT
) STRICT;
CREATE INDEX client_contact_exact_lookup
 ON client_contact_points(tenant_id, kind, normalization_version, lookup_key_version, exact_lookup_token, contact_point_id)
 WHERE status = 'ACTIVE';
CREATE TRIGGER client_contact_active_client_insert_guard BEFORE INSERT ON client_contact_points BEGIN SELECT 1; END;
CREATE TRIGGER client_contact_update_guard BEFORE UPDATE ON client_contact_points BEGIN
 SELECT RAISE(ABORT, 'client_contact_identity_immutable') WHERE 0;
 SELECT RAISE(ABORT, 'client_contact_archived_immutable') WHERE 0;
 SELECT RAISE(ABORT, 'client_contact_client_not_active') WHERE 0;
 SELECT RAISE(ABORT, 'client_contact_updated_at_rewind') WHERE 0;
END;
CREATE TRIGGER client_contact_delete_guard BEFORE DELETE ON client_contact_points BEGIN
 SELECT RAISE(ABORT, 'client_contact_delete_forbidden');
END;
""",
        encoding="utf-8",
    )
    d1_test = root / D1_TEST
    d1_test.parent.mkdir(parents=True, exist_ok=True)
    d1_test.write_text(
        "test_schema_has_no_plaintext_contact_column\n"
        "test_tenant_and_client_scope_are_structural\n"
        "test_exact_lookup_is_index_backed\n"
        "test_archival_is_one_way_and_delete_is_forbidden\n"
        "test_active_contact_requires_active_client\n",
        encoding="utf-8",
    )
    lookup_port = root / LOOKUP_PORT
    lookup_port.parent.mkdir(parents=True, exist_ok=True)
    lookup_port.write_text(
        "pub trait ContactLookupProtectionPort { derive_exact_lookup_candidates token: &ExactLookupToken }\n"
        "pub trait ContactExactLookupRepositoryPort { find_active_contacts_by_exact_lookup token: &ExactLookupToken }\n",
        encoding="utf-8",
    )
    lookup_adapter = root / LOOKUP_ADAPTER
    lookup_adapter.parent.mkdir(parents=True, exist_ok=True)
    lookup_adapter.write_text(
        """impl ContactExactLookupRepositoryPort for D1ContactExactLookupRepository
WHERE tenant_id = ?
AND kind = ?
AND normalization_version = ?
AND lookup_key_version = ?
AND exact_lookup_token = ?
AND status = 'ACTIVE'
token.key_version().value()
token.bytes().as_slice()
.all()
.results::<ExactLookupRow>()
impl<N: ContactNonceSource> ContactLookupProtectionPort
derive_lookup_candidates(request.tenant_id(), request.hmac_input())
""",
        encoding="utf-8",
    )


def self_test() -> int:
    with tempfile.TemporaryDirectory(prefix="phase2b-client-boundary-") as temp:
        root = Path(temp)
        write_fixture(root)
        baseline = validate(root)
        if baseline:
            print("invalid Phase 2B protected-D1 fixture baseline")
            print("\n".join(baseline))
            return 1

        write_fixture(root)
        migration = root / MIGRATION
        source = read(migration).replace(
            " ciphertext BLOB NOT NULL",
            " display_value TEXT NOT NULL,\n ciphertext BLOB NOT NULL",
            1,
        )
        migration.write_text(source, encoding="utf-8")
        if not any("plaintext-capable" in error for error in validate(root)):
            print("plaintext contact column fixture unexpectedly passed")
            return 1

        write_fixture(root)
        migration = root / MIGRATION
        source = read(migration).replace(
            "client_contact_points(tenant_id, kind, normalization_version, lookup_key_version, exact_lookup_token, contact_point_id)",
            "client_contact_points(kind, normalization_version, lookup_key_version, exact_lookup_token, contact_point_id, tenant_id)",
            1,
        )
        migration.write_text(source, encoding="utf-8")
        if not any("lead with tenant_id" in error for error in validate(root)):
            print("unscoped exact lookup index fixture unexpectedly passed")
            return 1

        write_fixture(root)
        adapter = root / LOOKUP_ADAPTER
        adapter.write_text(read(adapter).replace("WHERE tenant_id = ?\n", "", 1), encoding="utf-8")
        if not any("WHERE tenant_id = ?" in error for error in validate(root)):
            print("unscoped exact lookup repository fixture unexpectedly passed")
            return 1

        write_fixture(root)
        adapter = root / LOOKUP_ADAPTER
        adapter.write_text(read(adapter) + "\ndecrypt_contact_display(\n", encoding="utf-8")
        if not any("must not use `decrypt_contact_display(`" in error for error in validate(root)):
            print("decrypting exact lookup fixture unexpectedly passed")
            return 1

    print("Phase 2B protected-D1 negative fixtures rejected as expected.")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    errors = validate(args.root.resolve())
    if errors:
        print("\n".join(errors))
        return 1
    print("Phase 2B protected client-contact D1 boundaries passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
