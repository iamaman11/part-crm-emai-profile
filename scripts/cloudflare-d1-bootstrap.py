#!/usr/bin/env python3
"""Build and verify deterministic first-boot SQL for a brand-new Cloudflare D1 database.

This authority is intentionally narrower than D4 migration policy. It is only for an empty database:
it concatenates the exact accepted migration inventory with the canonical d1_migrations ledger so the
result can be sent through Wrangler's remote SQL-file import path. It never upgrades a non-empty DB.

AR-9 extends the existing bootstrap authority with a convergence proof for both D1 components. The
historical Catalog bootstrap bytes/evidence format remain unchanged; the same builder is exercised for
Catalog and Resolver and compared with sequential migrations using a normalized final schema state.
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
RESOLVER_MIGRATIONS_DIR = ROOT / "migrations" / "resolver-d1"
COMPONENT_MIGRATION_ROOTS = {
    "catalog": MIGRATIONS_DIR,
    "resolver": RESOLVER_MIGRATIONS_DIR,
}
DEFAULT_OUTPUT = ROOT / "artifacts" / "cloudflare-d1-bootstrap" / "bootstrap.sql"
DEFAULT_EVIDENCE = ROOT / "docs" / "evidence" / "2026-08-14-pre2j-d3a-empty-d1-bootstrap.json"
MIGRATION_RE = re.compile(r"^(?P<number>[0-9]{4})_[a-z0-9_]+\.sql$")
FORBIDDEN_FK_DISABLE_RE = re.compile(r"\bpragma\s+foreign_keys\s*=\s*(?:off|0)\b", re.IGNORECASE)
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
COMMIT_SHA_RE = re.compile(r"^[0-9a-f]{40}$")
REMOTE_PROOF_SOURCE_SHA = "493d399b9531776aa8208242a5d1c05681764231"
LEDGER_NAME = "d1_migrations"
REMOTE_D1_SYSTEM_TYPE = "table"
REMOTE_D1_SYSTEM_NAME = "_cf_KV"
REMOTE_D1_SYSTEM_SCHEMA_ROW = {
    "type": REMOTE_D1_SYSTEM_TYPE,
    "name": REMOTE_D1_SYSTEM_NAME,
    "tbl_name": REMOTE_D1_SYSTEM_NAME,
}
INTEGRATION_EVENT_FOUNDATION_OBJECTS = [
    {
        "type": "index",
        "name": "consumer_idempotency_event_lookup",
        "tbl_name": "consumer_idempotency",
    },
    {
        "type": "index",
        "name": "notification_events_tenant_time",
        "tbl_name": "notification_events",
    },
    {"type": "table", "name": "consumer_idempotency", "tbl_name": "consumer_idempotency"},
    {"type": "table", "name": "notification_events", "tbl_name": "notification_events"},
    {
        "type": "trigger",
        "name": "consumer_idempotency_source_guard",
        "tbl_name": "consumer_idempotency",
    },
    {
        "type": "trigger",
        "name": "notification_event_source_guard",
        "tbl_name": "notification_events",
    },
    {"type": "trigger", "name": "outbox_event_payload_guard", "tbl_name": "outbox_events"},
    {"type": "trigger", "name": "outbox_event_version_guard", "tbl_name": "outbox_events"},
]
INTEGRATION_EVENT_FOUNDATION_COLUMNS = [
    {"name": "envelope_version", "type": "INTEGER", "notnull": 1, "dflt_value": "1"},
    {"name": "event_version", "type": "INTEGER", "notnull": 1, "dflt_value": "1"},
]


class BootstrapError(ValueError):
    """Raised when the empty-D1 bootstrap authority fails closed."""


def fail(message: str) -> None:
    raise BootstrapError(message)


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def canonical_json_sha256(value: Any) -> str:
    payload = json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return sha256_bytes(payload.encode("utf-8"))


def quote_identifier(value: str) -> str:
    return '"' + value.replace('"', '""') + '"'


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
        text = path.read_text(encoding="utf-8")
        if "\x00" in text:
            fail(f"migration contains a NUL byte: {path.name}")
        if FORBIDDEN_FK_DISABLE_RE.search(text):
            fail(f"migration must not disable foreign key enforcement: {path.name}")
        numbers.append(int(match.group("number")))
    expected = list(range(1, len(migrations) + 1))
    if numbers != expected:
        fail(f"D1 migration sequence must be contiguous from 0001: observed={numbers}")
    if len({path.name for path in migrations}) != len(migrations):
        fail("D1 migration inventory contains duplicate names")
    return migrations


def empty_guard_sql() -> str:
    return f'''SELECT CASE
    WHEN EXISTS (
        SELECT 1
        FROM sqlite_schema
        WHERE name NOT LIKE 'sqlite_%'
          AND NOT (
              type = '{REMOTE_D1_SYSTEM_TYPE}'
              AND name = '{REMOTE_D1_SYSTEM_NAME}'
              AND tbl_name = '{REMOTE_D1_SYSTEM_NAME}'
          )
    ) THEN abs(-9223372036854775808)
    ELSE 1
END AS empty_d1_guard;
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


def normalize_sql(value: str) -> str:
    return " ".join(value.split())


def schema_signature(connection: sqlite3.Connection) -> list[tuple[str, str, str, str]]:
    """Historical Catalog bootstrap signature retained for accepted D3 evidence."""
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


def application_tables(connection: sqlite3.Connection) -> list[str]:
    return [
        str(row[0])
        for row in connection.execute(
            """
            SELECT name
            FROM sqlite_master
            WHERE type = 'table'
              AND name NOT LIKE 'sqlite_%'
              AND name != ?
            ORDER BY name
            """,
            (LEDGER_NAME,),
        ).fetchall()
    ]


def normalized_schema_state(connection: sqlite3.Connection) -> dict[str, Any]:
    """Return deterministic final D1 structure without volatile ledger timestamps."""
    objects = [
        {
            "type": str(row[0]),
            "name": str(row[1]),
            "table": str(row[2]),
            "sql": normalize_sql(str(row[3] or "")),
        }
        for row in connection.execute(
            """
            SELECT type, name, tbl_name, sql
            FROM sqlite_master
            WHERE name NOT LIKE 'sqlite_%'
              AND name != ?
            ORDER BY type, name
            """,
            (LEDGER_NAME,),
        ).fetchall()
    ]
    columns: list[dict[str, Any]] = []
    indexes: list[dict[str, Any]] = []
    foreign_keys: list[dict[str, Any]] = []
    for table in application_tables(connection):
        quoted_table = quote_identifier(table)
        for row in connection.execute(f"PRAGMA table_info({quoted_table})").fetchall():
            columns.append(
                {
                    "table": table,
                    "cid": int(row[0]),
                    "name": str(row[1]),
                    "type": str(row[2]),
                    "notnull": int(row[3]),
                    "default": None if row[4] is None else str(row[4]),
                    "pk": int(row[5]),
                }
            )
        for row in connection.execute(f"PRAGMA index_list({quoted_table})").fetchall():
            index_name = str(row[1])
            index_columns = [
                None if item[2] is None else str(item[2])
                for item in connection.execute(
                    f"PRAGMA index_info({quote_identifier(index_name)})"
                ).fetchall()
            ]
            indexes.append(
                {
                    "table": table,
                    "name": index_name,
                    "unique": int(row[2]),
                    "origin": str(row[3]),
                    "partial": int(row[4]),
                    "columns": index_columns,
                }
            )
        for row in connection.execute(f"PRAGMA foreign_key_list({quoted_table})").fetchall():
            foreign_keys.append(
                {
                    "table": table,
                    "id": int(row[0]),
                    "seq": int(row[1]),
                    "parent_table": str(row[2]),
                    "from": None if row[3] is None else str(row[3]),
                    "to": None if row[4] is None else str(row[4]),
                    "on_update": str(row[5]),
                    "on_delete": str(row[6]),
                    "match": str(row[7]),
                }
            )
    indexes.sort(key=lambda item: (item["table"], item["name"]))
    foreign_keys.sort(key=lambda item: (item["table"], item["id"], item["seq"]))
    state = {
        "objects": objects,
        "columns": columns,
        "indexes": indexes,
        "foreign_keys": foreign_keys,
        "ledger": ledger_names(connection),
    }
    return {**state, "schema_signature": canonical_json_sha256(state)}


def foreign_key_violations(connection: sqlite3.Connection) -> list[list[Any]]:
    return [list(row) for row in connection.execute("PRAGMA foreign_key_check").fetchall()]


def assert_foreign_keys_clean(connection: sqlite3.Connection, label: str) -> None:
    violations = foreign_key_violations(connection)
    if violations:
        fail(f"{label} foreign_key_check failed: {violations}")


def assert_integrity_clean(connection: sqlite3.Connection, label: str) -> None:
    rows = connection.execute("PRAGMA integrity_check").fetchall()
    if rows != [("ok",)]:
        fail(f"{label} integrity_check failed: {rows}")


def apply_sequential(connection: sqlite3.Connection, migrations: list[Path]) -> None:
    """Historical parity helper: apply SQL in order without synthesizing a ledger."""
    for migration in migrations:
        connection.executescript(migration.read_text(encoding="utf-8"))
    connection.commit()


def apply_sequential_with_ledger(connection: sqlite3.Connection, migrations: list[Path]) -> None:
    """Model Wrangler migration semantics for convergence: successful migrations append to the ledger."""
    connection.executescript(ledger_sql())
    for migration in migrations:
        connection.executescript(migration.read_text(encoding="utf-8"))
        connection.execute(f'INSERT INTO "{LEDGER_NAME}" (name) VALUES (?)', (migration.name,))
        connection.commit()


def ledger_names(connection: sqlite3.Connection) -> list[str]:
    return [str(row[0]) for row in connection.execute(f'SELECT name FROM "{LEDGER_NAME}" ORDER BY id')]


def prove_bootstrap_parity(directory: Path = MIGRATIONS_DIR) -> None:
    """Preserve the original Catalog bootstrap parity proof used by historical D3 evidence."""
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


def prove_component_convergence(component: str, directory: Path) -> dict[str, Any]:
    migrations = validated_migrations(directory)
    sequential = sqlite3.connect(":memory:")
    bootstrap = sqlite3.connect(":memory:")
    try:
        sequential.execute("PRAGMA foreign_keys = ON")
        bootstrap.execute("PRAGMA foreign_keys = ON")
        if sequential.execute("PRAGMA foreign_keys").fetchone() != (1,):
            fail(f"{component} sequential proof did not enable foreign key enforcement")
        if bootstrap.execute("PRAGMA foreign_keys").fetchone() != (1,):
            fail(f"{component} bootstrap proof did not enable foreign key enforcement")

        apply_sequential_with_ledger(sequential, migrations)
        bootstrap.executescript(build_bootstrap_bytes(directory).decode("utf-8"))
        bootstrap.commit()

        assert_foreign_keys_clean(sequential, f"{component} sequential")
        assert_foreign_keys_clean(bootstrap, f"{component} bootstrap")
        assert_integrity_clean(sequential, f"{component} sequential")
        assert_integrity_clean(bootstrap, f"{component} bootstrap")

        expected_ledger = [path.name for path in migrations]
        if ledger_names(sequential) != expected_ledger or ledger_names(bootstrap) != expected_ledger:
            fail(f"{component} convergence ledger differs from exact migration inventory")

        sequential_state = normalized_schema_state(sequential)
        bootstrap_state = normalized_schema_state(bootstrap)
        if sequential_state != bootstrap_state:
            fail(f"{component} fresh bootstrap and sequential migrations do not converge")
        return {
            "component": component,
            "migration_count": len(migrations),
            "latest": migrations[-1].name,
            "schema_signature": sequential_state["schema_signature"],
            "foreign_key_check": "CLEAN",
            "integrity_check": "ok",
        }
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
    if rows != [REMOTE_D1_SYSTEM_SCHEMA_ROW]:
        fail(f"bootstrap target does not match exact fresh D1 system schema; observed rows={rows}")


def validate_empty_query_file(path: Path) -> None:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise BootstrapError(f"cannot read remote empty-D1 query JSON: {error}") from error
    validate_empty_query_document(document)


def require_exact_keys(value: Any, expected: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != expected:
        fail(f"{label} must contain exact keys {sorted(expected)}")
    return value


def verify_remote_evidence_document(document: Any) -> None:
    evidence = require_exact_keys(
        document,
        {
            "schema_version",
            "evidence_kind",
            "proof_source_sha",
            "wrangler_version",
            "region",
            "bootstrap",
            "fresh_target_schema",
            "first_import",
            "migration_ledger",
            "integration_event_foundation",
            "replay",
            "canonical_staging_touched",
            "canonical_production_touched",
            "user_data_involved",
            "secret_material_recorded",
        },
        "remote bootstrap evidence",
    )
    if evidence["schema_version"] != 1:
        fail("remote bootstrap evidence schema version is unsupported")
    if evidence["evidence_kind"] != "cloudflare-d1-empty-bootstrap-remote-proof":
        fail("remote bootstrap evidence kind is not authoritative")
    proof_source = evidence["proof_source_sha"]
    if not isinstance(proof_source, str) or COMMIT_SHA_RE.fullmatch(proof_source) is None:
        fail("remote bootstrap proof source must be one exact commit SHA")
    if proof_source != REMOTE_PROOF_SOURCE_SHA:
        fail("remote bootstrap proof source differs from the accepted external execution head")
    if evidence["wrangler_version"] != "4.94.0" or evidence["region"] != "EEUR":
        fail("remote bootstrap evidence toolchain or region differs from the proved authority")

    payload = build_bootstrap_bytes()
    bootstrap = require_exact_keys(evidence["bootstrap"], {"bytes", "sha256"}, "bootstrap identity")
    if bootstrap["bytes"] != len(payload) or bootstrap["sha256"] != sha256_bytes(payload):
        fail("remote proof bootstrap identity differs from exact current migration authority")
    if not isinstance(bootstrap["sha256"], str) or SHA256_RE.fullmatch(bootstrap["sha256"]) is None:
        fail("remote proof bootstrap SHA-256 is malformed")
    if evidence["fresh_target_schema"] != [REMOTE_D1_SYSTEM_SCHEMA_ROW]:
        fail("remote proof target was not an exact fresh D1 system schema")

    first_import = require_exact_keys(
        evidence["first_import"], {"completed", "statement_count", "rows_written"}, "first import"
    )
    if first_import["completed"] is not True:
        fail("remote bootstrap first import did not complete")
    for key in ("statement_count", "rows_written"):
        if (
            not isinstance(first_import[key], int)
            or isinstance(first_import[key], bool)
            or first_import[key] <= 0
        ):
            fail(f"remote bootstrap first import {key} must be a positive integer")

    expected_names = [path.name for path in validated_migrations()]
    ledger = require_exact_keys(
        evidence["migration_ledger"], {"count", "ordered_names", "latest"}, "migration ledger"
    )
    if ledger["count"] != len(expected_names) or ledger["ordered_names"] != expected_names:
        fail("remote bootstrap ledger differs from exact ordered migration authority")
    if ledger["latest"] != expected_names[-1]:
        fail("remote bootstrap latest ledger row differs from migration authority")

    foundation = require_exact_keys(
        evidence["integration_event_foundation"], {"objects", "outbox_columns"}, "0012 evidence"
    )
    if foundation["objects"] != INTEGRATION_EVENT_FOUNDATION_OBJECTS:
        fail("remote bootstrap 0012 schema objects differ from the required foundation")
    if foundation["outbox_columns"] != INTEGRATION_EVENT_FOUNDATION_COLUMNS:
        fail("remote bootstrap 0012 outbox columns differ from the required foundation")

    replay = require_exact_keys(
        evidence["replay"],
        {
            "rejected",
            "error_class",
            "schema_object_count_before",
            "schema_object_count_after",
            "schema_sha256_before",
            "schema_sha256_after",
            "ledger_count_after",
            "latest_after",
            "residue",
        },
        "replay evidence",
    )
    if replay["rejected"] is not True or replay["error_class"] != "SQLITE_ERROR":
        fail("remote bootstrap replay was not rejected by the expected SQLite boundary")
    count_before = replay["schema_object_count_before"]
    count_after = replay["schema_object_count_after"]
    if not isinstance(count_before, int) or isinstance(count_before, bool) or count_before <= 0:
        fail("remote bootstrap replay schema count is invalid")
    if count_before != count_after:
        fail("remote bootstrap replay changed the schema object count")
    digest_before = replay["schema_sha256_before"]
    digest_after = replay["schema_sha256_after"]
    if not isinstance(digest_before, str) or SHA256_RE.fullmatch(digest_before) is None:
        fail("remote bootstrap replay schema SHA-256 is malformed")
    if digest_before != digest_after:
        fail("remote bootstrap replay changed the schema inventory digest")
    if replay["ledger_count_after"] != len(expected_names) or replay["latest_after"] != expected_names[-1]:
        fail("remote bootstrap replay changed the migration ledger")
    if replay["residue"] != []:
        fail("remote bootstrap replay left schema residue")

    for key in (
        "canonical_staging_touched",
        "canonical_production_touched",
        "user_data_involved",
        "secret_material_recorded",
    ):
        if evidence[key] is not False:
            fail(f"remote bootstrap evidence violates the bounded {key} requirement")


def verify_remote_evidence_file(path: Path) -> None:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise BootstrapError(f"cannot read remote bootstrap evidence JSON: {error}") from error
    verify_remote_evidence_document(document)


def check_repository_policy() -> None:
    first = build_bootstrap_bytes()
    second = build_bootstrap_bytes()
    if first != second:
        fail("identical migration inputs produced non-deterministic bootstrap SQL")
    prove_bootstrap_parity()
    convergence = [
        prove_component_convergence(component, directory)
        for component, directory in COMPONENT_MIGRATION_ROOTS.items()
    ]
    print(
        "Empty-D1 bootstrap policy passed: "
        f"migrations={len(validated_migrations())} bytes={len(first)} sha256={sha256_bytes(first)} "
        f"convergence={json.dumps(convergence, sort_keys=True, separators=(',', ':'))}."
    )


def expect_rejected(label: str, operation: Any) -> None:
    try:
        operation()
    except (BootstrapError, sqlite3.DatabaseError):
        return
    fail(f"negative bootstrap fixture unexpectedly passed: {label}")


def prove_fk_corruption_detected() -> None:
    connection = sqlite3.connect(":memory:")
    try:
        # SQLite starts with FK enforcement disabled. Build an intentionally corrupt synthetic DB,
        # then enable enforcement before invoking the same invariant check used by convergence.
        connection.executescript(
            """
            CREATE TABLE parent(id INTEGER PRIMARY KEY);
            CREATE TABLE child(
                id INTEGER PRIMARY KEY,
                parent_id INTEGER NOT NULL REFERENCES parent(id)
            );
            INSERT INTO child(id, parent_id) VALUES (1, 999);
            """
        )
        connection.commit()
        connection.execute("PRAGMA foreign_keys = ON")
        if connection.execute("PRAGMA foreign_keys").fetchone() != (1,):
            fail("negative FK fixture could not enable foreign key enforcement")
        expect_rejected(
            "foreign-key corruption",
            lambda: assert_foreign_keys_clean(connection, "negative fixture"),
        )
    finally:
        connection.close()


def self_test() -> None:
    migrations = validated_migrations()
    with tempfile.TemporaryDirectory(prefix="cloudflare-d1-bootstrap-") as temporary:
        root = Path(temporary)
        fixture = root / "migrations"
        fixture.mkdir()
        for migration in migrations:
            shutil.copyfile(migration, fixture / migration.name)

        prove_bootstrap_parity(fixture)
        prove_component_convergence("catalog-fixture", fixture)
        prove_component_convergence("resolver", RESOLVER_MIGRATIONS_DIR)
        prove_fk_corruption_detected()

        missing = fixture / migrations[min(1, len(migrations) - 1)].name
        saved = missing.read_bytes()
        missing.unlink()
        expect_rejected("missing migration", lambda: validated_migrations(fixture))
        missing.write_bytes(saved)

        duplicate = fixture / "9999_duplicate.sql"
        duplicate.write_text("SELECT 1;\n", encoding="utf-8")
        expect_rejected("non-contiguous migration", lambda: validated_migrations(fixture))
        duplicate.unlink()

        forbidden_fk = fixture / migrations[-1].name
        original_fk_bytes = forbidden_fk.read_bytes()
        forbidden_fk.write_text("PRAGMA foreign_keys = OFF;\n", encoding="utf-8")
        expect_rejected("migration foreign_keys OFF", lambda: validated_migrations(fixture))
        forbidden_fk.write_bytes(original_fk_bytes)

        connection = sqlite3.connect(":memory:")
        try:
            connection.execute("CREATE TABLE preexisting(id INTEGER PRIMARY KEY)")
            expect_rejected(
                "non-empty target",
                lambda: connection.executescript(build_bootstrap_bytes(fixture).decode("utf-8")),
            )
        finally:
            connection.close()

        # Fresh remote D1 databases contain Cloudflare's exact reserved storage table.
        # The in-file guard must accept only that platform-owned schema object.
        connection = sqlite3.connect(":memory:")
        try:
            connection.execute(
                f'CREATE TABLE "{REMOTE_D1_SYSTEM_NAME}" '
                "(key TEXT PRIMARY KEY, value BLOB) WITHOUT ROWID"
            )
            connection.executescript(build_bootstrap_bytes(fixture).decode("utf-8"))
            if ledger_names(connection) != [path.name for path in migrations]:
                fail("bootstrap with the reserved D1 system table produced an incorrect ledger")
            schema_before_replay = schema_signature(connection)
            ledger_before_replay = ledger_names(connection)
            expect_rejected(
                "bootstrap replay",
                lambda: connection.executescript(build_bootstrap_bytes(fixture).decode("utf-8")),
            )
            if schema_signature(connection) != schema_before_replay:
                fail("rejected bootstrap replay changed the application schema")
            if ledger_names(connection) != ledger_before_replay:
                fail("rejected bootstrap replay changed the migration ledger")
        finally:
            connection.close()

        payload = build_bootstrap_bytes(fixture)
        tampered = bytearray(payload)
        tampered[-2] ^= 1
        if sha256_bytes(bytes(tampered)) == sha256_bytes(payload):
            fail("tampered bootstrap fixture retained the original digest")

        validate_empty_query_document(
            [{"results": [REMOTE_D1_SYSTEM_SCHEMA_ROW.copy()], "success": True}]
        )
        expect_rejected(
            "missing reserved D1 system row",
            lambda: validate_empty_query_document([{"results": [], "success": True}]),
        )
        expect_rejected(
            "non-empty remote inventory",
            lambda: validate_empty_query_document(
                [
                    {
                        "results": [
                            REMOTE_D1_SYSTEM_SCHEMA_ROW.copy(),
                            {"type": "table", "name": "existing", "tbl_name": "existing"},
                        ],
                        "success": True,
                    }
                ]
            ),
        )
        expect_rejected(
            "malformed reserved D1 system row",
            lambda: validate_empty_query_document(
                [{"results": [{"type": "table", "name": "_cf_KV"}], "success": True}]
            ),
        )

        evidence = json.loads(DEFAULT_EVIDENCE.read_text(encoding="utf-8"))
        verify_remote_evidence_document(evidence)
        tampered_evidence = json.loads(json.dumps(evidence))
        tampered_evidence["bootstrap"]["sha256"] = "0" * 64
        expect_rejected(
            "stale remote proof bootstrap digest",
            lambda: verify_remote_evidence_document(tampered_evidence),
        )
        reordered_evidence = json.loads(json.dumps(evidence))
        reordered_evidence["migration_ledger"]["ordered_names"].reverse()
        expect_rejected(
            "reordered remote proof ledger",
            lambda: verify_remote_evidence_document(reordered_evidence),
        )
        mutated_replay = json.loads(json.dumps(evidence))
        mutated_replay["replay"]["schema_sha256_after"] = "1" * 64
        expect_rejected(
            "changed replay schema digest",
            lambda: verify_remote_evidence_document(mutated_replay),
        )
        expanded_evidence = json.loads(json.dumps(evidence))
        expanded_evidence["database_id"] = "prohibited-resource-identity"
        expect_rejected(
            "unexpected remote resource identity",
            lambda: verify_remote_evidence_document(expanded_evidence),
        )
        stale_source = json.loads(json.dumps(evidence))
        stale_source["proof_source_sha"] = "0" * 40
        expect_rejected(
            "substituted remote proof source",
            lambda: verify_remote_evidence_document(stale_source),
        )
        expanded_scope = json.loads(json.dumps(evidence))
        expanded_scope["canonical_staging_touched"] = True
        expect_rejected(
            "canonical staging mutation",
            lambda: verify_remote_evidence_document(expanded_scope),
        )

    print("Empty-D1 bootstrap deterministic, convergence, FK and negative self-tests passed.")


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

    evidence = subparsers.add_parser("verify-evidence")
    evidence.add_argument("--file", type=Path, default=DEFAULT_EVIDENCE)

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
    if args.command == "verify-evidence":
        verify_remote_evidence_file(args.file)
        print("Verified sanitized remote empty-D1 bootstrap evidence against current authority.")
        return 0
    fail(f"unsupported command: {args.command}")


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except BootstrapError as error:
        raise SystemExit(f"Empty-D1 bootstrap rejected: {error}") from error
