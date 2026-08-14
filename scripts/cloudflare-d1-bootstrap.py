#!/usr/bin/env python3
"""Build and verify deterministic first-boot SQL for a brand-new Cloudflare D1 database.

This authority is intentionally narrower than D4 migration policy. It is only for an empty database:
it concatenates the exact accepted migration inventory with the canonical d1_migrations ledger so the
result can be sent through Wrangler's remote SQL-file import path. It never upgrades a non-empty DB.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import sqlite3
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS_DIR = ROOT / "migrations" / "d1"
DEFAULT_OUTPUT = ROOT / "artifacts" / "cloudflare-d1-bootstrap" / "bootstrap.sql"
MIGRATION_RE = re.compile(r"^(?P<number>[0-9]{4})_[a-z0-9_]+\.sql$")
LEDGER_NAME = "d1_migrations"
GUARD_NAME = "__part_crm_empty_d1_guard"


class BootstrapError(ValueError):
    """Raised when the empty-D1 bootstrap authority fails closed."""


def fail(message: str) -> None:
    raise BootstrapError(message)


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def validated_migrations(directory: Path = MIGRATIONS_DIR) -> list[Path]:
    if not directory.is_dir() or directory.is_symlink():
        fail(f"migration directory must be a real directory: {directory}")
    migrations = sorted(directory.glob("*.sql"), key=lambda path: path.name)
    if not migrations:
        fail("D1 migration inventory must not be empty")
    numbers: list[int] = []
    for path in migrations:
        if path.is_symlink() or not path.is_file():
            fail(f"migration must be a regular file: {path}")
        match = MIGRATION_RE.fullmatch(path.name)
        if match is None:
            fail(f"unexpected D1 migration filename: {path.name}")
        numbers.append(int(match.group("number")))
    expected = list(range(1, len(migrations) + 1))
    if numbers != expected:
        fail(f"D1 migration sequence must be contiguous from 0001: observed={numbers}")
    if len({path.name for path in migrations}) != len(migrations):
        fail("D1 migration inventory contains duplicate names")
    return migrations


def empty_guard_sql() -> str:
    return f'''CREATE TABLE "{GUARD_NAME}" (
    ok INTEGER NOT NULL CHECK(ok = 1)
) STRICT;
INSERT INTO "{GUARD_NAME}" (ok)
SELECT CASE
    WHEN EXISTS (
        SELECT 1
        FROM sqlite_schema
        WHERE name NOT LIKE 'sqlite_%'
          AND name <> '{GUARD_NAME}'
    ) THEN 0
    ELSE 1
END;
DROP TABLE "{GUARD_NAME}";
'''


def ledger_sql() -> str:
    return f'''CREATE TABLE "{LEDGER_NAME}" (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT UNIQUE,
    applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP NOT NULL
);
'''


def build_bootstrap_bytes(directory: Path = MIGRATIONS_DIR) -> bytes:
    migrations = validated_migrations(directory)
    parts = [empty_guard_sql(), ledger_sql()]
    for migration in migrations:
        text = migration.read_text(encoding="utf-8")
        if "\x00" in text:
            fail(f"migration contains a NUL byte: {migration.name}")
        parts.append(text.rstrip() + "\n")
        escaped_name = migration.name.replace("'", "''")
        parts.append(f'INSERT INTO "{LEDGER_NAME}" (name) VALUES (\'{escaped_name}\');\n')
    document = "\n".join(parts)
    if not document.endswith("\n"):
        document += "\n"
    return document.encode("utf-8")


def write_bootstrap(path: Path) -> tuple[int, str]:
    payload = build_bootstrap_bytes()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(payload)
    return len(payload), sha256_bytes(payload)


def schema_signature(connection: sqlite3.Connection) -> list[tuple[str, str, str, str]]:
    rows = connection.execute(
        """
        SELECT type, name, tbl_name, COALESCE(sql, '') AS sql
        FROM sqlite_master
        WHERE name NOT LIKE 'sqlite_%'
          AND name != ?
        ORDER BY type, name
        """,
        (LEDGER_NAME,),
    ).fetchall()
    return [(str(row[0]), str(row[1]), str(row[2]), str(row[3])) for row in rows]


def apply_sequential(connection: sqlite3.Connection, migrations: list[Path]) -> None:
    for migration in migrations:
        connection.executescript(migration.read_text(encoding="utf-8"))
    connection.commit()


def ledger_names(connection: sqlite3.Connection) -> list[str]:
    return [str(row[0]) for row in connection.execute(f'SELECT name FROM "{LEDGER_NAME}" ORDER BY id')]


def prove_bootstrap_parity(directory: Path = MIGRATIONS_DIR) -> None:
    migrations = validated_migrations(directory)
    expected_names = [path.name for path in migrations]
    sequential = sqlite3.connect(":memory:")
    bootstrap = sqlite3.connect(":memory:")
    try:
        sequential.execute("PRAGMA foreign_keys = ON")
        bootstrap.execute("PRAGMA foreign_keys = ON")
        apply_sequential(sequential, migrations)
        bootstrap.executescript(build_bootstrap_bytes(directory).decode("utf-8"))
        bootstrap.commit()
        if schema_signature(sequential) != schema_signature(bootstrap):
            fail("empty-D1 bootstrap schema differs from sequential migration semantics")
        if ledger_names(bootstrap) != expected_names:
            fail("empty-D1 bootstrap ledger differs from exact migration inventory")
    finally:
        sequential.close()
        bootstrap.close()


def validate_empty_query_document(document: Any) -> None:
    if not isinstance(document, list) or len(document) != 1 or not isinstance(document[0], dict):
        fail("remote empty-D1 query must be one Wrangler JSON result object")
    result = document[0]
    if result.get("success") is not True or not isinstance(result.get("results"), list):
        fail("remote empty-D1 query did not succeed")
    rows = result["results"]
    if rows:
        fail(f"bootstrap target is not empty; observed sqlite_schema rows={rows}")


def validate_empty_query_file(path: Path) -> None:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise BootstrapError(f"cannot read remote empty-D1 query JSON: {error}") from error
    validate_empty_query_document(document)


def check_repository_policy() -> None:
    first = build_bootstrap_bytes()
    second = build_bootstrap_bytes()
    if first != second:
        fail("identical migration inputs produced non-deterministic bootstrap SQL")
    prove_bootstrap_parity()
    print(
        "Empty-D1 bootstrap policy passed: "
        f"migrations={len(validated_migrations())} bytes={len(first)} sha256={sha256_bytes(first)}."
    )


def expect_rejected(label: str, operation: Any) -> None:
    try:
        operation()
    except (BootstrapError, sqlite3.DatabaseError):
        return
    fail(f"negative bootstrap fixture unexpectedly passed: {label}")


def self_test() -> None:
    migrations = validated_migrations()
    with tempfile.TemporaryDirectory(prefix="cloudflare-d1-bootstrap-") as temporary:
        root = Path(temporary)
        fixture = root / "migrations"
        fixture.mkdir()
        for migration in migrations:
            shutil.copyfile(migration, fixture / migration.name)

        prove_bootstrap_parity(fixture)

        missing = fixture / migrations[min(1, len(migrations) - 1)].name
        saved = missing.read_bytes()
        missing.unlink()
        expect_rejected("missing migration", lambda: validated_migrations(fixture))
        missing.write_bytes(saved)

        duplicate = fixture / "9999_duplicate.sql"
        duplicate.write_text("SELECT 1;\n", encoding="utf-8")
        expect_rejected("non-contiguous migration", lambda: validated_migrations(fixture))
        duplicate.unlink()

        connection = sqlite3.connect(":memory:")
        try:
            connection.execute("CREATE TABLE preexisting(id INTEGER PRIMARY KEY)")
            expect_rejected(
                "non-empty target",
                lambda: connection.executescript(build_bootstrap_bytes(fixture).decode("utf-8")),
            )
        finally:
            connection.close()

        payload = build_bootstrap_bytes(fixture)
        tampered = bytearray(payload)
        tampered[-2] ^= 1
        if sha256_bytes(bytes(tampered)) == sha256_bytes(payload):
            fail("tampered bootstrap fixture retained the original digest")

        validate_empty_query_document([{"results": [], "success": True}])
        expect_rejected(
            "non-empty remote inventory",
            lambda: validate_empty_query_document(
                [{"results": [{"type": "table", "name": "existing"}], "success": True}]
            ),
        )

    print("Empty-D1 bootstrap deterministic and negative self-tests passed.")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("check")
    subparsers.add_parser("self-test")

    build = subparsers.add_parser("build")
    build.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)

    verify = subparsers.add_parser("verify")
    verify.add_argument("--file", type=Path, required=True)

    empty = subparsers.add_parser("validate-empty")
    empty.add_argument("--query-json", type=Path, required=True)

    args = parser.parse_args()
    if args.command == "check":
        check_repository_policy()
        return 0
    if args.command == "self-test":
        self_test()
        return 0
    if args.command == "build":
        size, digest = write_bootstrap(args.output)
        print(f"Built deterministic empty-D1 bootstrap: path={args.output} bytes={size} sha256={digest}")
        return 0
    if args.command == "verify":
        expected = build_bootstrap_bytes()
        actual = args.file.read_bytes()
        if actual != expected:
            fail("bootstrap SQL differs from exact current migration authority")
        print(f"Verified deterministic empty-D1 bootstrap sha256={sha256_bytes(actual)}.")
        return 0
    if args.command == "validate-empty":
        validate_empty_query_file(args.query_json)
        print("Remote D1 target is empty and eligible for first bootstrap import.")
        return 0
    fail(f"unsupported command: {args.command}")


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except BootstrapError as error:
        raise SystemExit(f"Empty-D1 bootstrap rejected: {error}") from error
