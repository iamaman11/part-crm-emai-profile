#!/usr/bin/env python3
"""Render and verify the CURRENT empty-D1 baseline from the typed D1 source owner.

This credential-free build adapter emits final schema and exact migration ledger for an empty
database. Existing databases keep the governed migration executor; this module never calls a provider.
Historical bootstrap evidence remains passive provenance, not a current execution requirement.
"""


from __future__ import annotations


import argparse
import hashlib
import json
import re
import sqlite3
import tempfile
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]


DEFAULT_OUTPUT = ROOT / "artifacts" / "cloudflare-d1-bootstrap" / "bootstrap.sql"


MIGRATION_RE = re.compile(r"^(?P<number>[0-9]{4})_[a-z0-9_]+\.sql$")


FORBIDDEN_FK_DISABLE_RE = re.compile(r"\bpragma\s+foreign_keys\s*=\s*(?:off|0)\b", re.IGNORECASE)


SHA256_RE = re.compile(r"^[0-9a-f]{64}$")


LEDGER_NAME = "d1_migrations"


REMOTE_D1_SYSTEM_TYPE = "table"


REMOTE_D1_SYSTEM_NAME = "_cf_KV"


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


def empty_guard_sql() -> str:
    return f'''SELECT CASE
    WHEN EXISTS (
        SELECT 1
        FROM sqlite_schema
        WHERE name NOT GLOB 'sqlite_*'
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


def normalize_sql(value: str) -> str:
    return " ".join(value.split())


def application_tables(connection: sqlite3.Connection) -> list[str]:
    return [
        str(row[0])
        for row in connection.execute(
            """
            SELECT name
            FROM sqlite_master
            WHERE type = 'table'
              AND name NOT GLOB 'sqlite_*'
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
            WHERE name NOT GLOB 'sqlite_*'
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


def apply_sequential_with_ledger(connection: sqlite3.Connection, migrations: list[Path]) -> None:
    """Model Wrangler migration semantics for convergence: successful migrations append to the ledger."""
    connection.executescript(ledger_sql())
    for migration in migrations:
        connection.executescript(migration.read_text(encoding="utf-8"))
        connection.execute(f'INSERT INTO "{LEDGER_NAME}" (name) VALUES (?)', (migration.name,))
        connection.commit()


def ledger_names(connection: sqlite3.Connection) -> list[str]:
    return [str(row[0]) for row in connection.execute(f'SELECT name FROM "{LEDGER_NAME}" ORDER BY id')]


def require_exact_keys(value: Any, expected: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != expected:
        fail(f"{label} must contain exact keys {sorted(expected)}")
    return value


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


def current_repository_projection() -> dict[str, Any]:
    """Read the CURRENT fresh-zero contract from the production typed D1 owner."""
    import subprocess

    command = [
        "cargo",
        "run",
        "--locked",
        "--quiet",
        "--manifest-path",
        "tools/opsctl/Cargo.toml",
        "--",
        "--root",
        ".",
        "d1",
        "repository",
    ]
    completed = subprocess.run(
        command,
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    if completed.returncode != 0:
        fail(
            "production typed D1 owner could not project CURRENT repository authority: "
            f"{completed.stderr.strip()}"
        )
    try:
        document = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise BootstrapError(f"production typed D1 repository projection is not JSON: {error}") from error
    if not isinstance(document, dict):
        fail("production typed D1 repository projection must be one object")
    return document


def current_source_file(
    repository_root: Path,
    source_root: str,
    migration_file: str,
    expected_sha256: str,
) -> Path:
    """Resolve one manifest-selected source without allowing symlink or repository escape."""
    if (
        not isinstance(source_root, str)
        or not source_root
        or source_root.startswith("/")
        or "\\" in source_root
    ):
        fail("CURRENT fresh-zero source root must be one clean repository-relative POSIX path")
    root_parts = source_root.split("/")
    if any(part in {"", ".", ".."} for part in root_parts):
        fail("CURRENT fresh-zero source root contains a path escape")
    if (
        not isinstance(migration_file, str)
        or "/" in migration_file
        or "\\" in migration_file
        or MIGRATION_RE.fullmatch(migration_file) is None
    ):
        fail("CURRENT fresh-zero migration filename is not one repository-local migration")
    if not isinstance(expected_sha256, str) or SHA256_RE.fullmatch(expected_sha256) is None:
        fail("CURRENT fresh-zero migration digest is malformed")

    root = repository_root.resolve(strict=True)
    directory = repository_root
    for part in root_parts:
        directory = directory / part
        if directory.is_symlink():
            fail("CURRENT fresh-zero source root traverses a symlink")
    if not directory.is_dir():
        fail("CURRENT fresh-zero source root is not a directory")

    path = directory / migration_file
    if path.is_symlink() or not path.is_file():
        fail("CURRENT fresh-zero migration source must be a regular file")
    resolved = path.resolve(strict=True)
    if not resolved.is_relative_to(root):
        fail("CURRENT fresh-zero migration source escaped the repository")
    payload = path.read_bytes()
    if sha256_bytes(payload) != expected_sha256:
        fail("CURRENT fresh-zero migration source digest differs from the typed D1 projection")
    try:
        text = payload.decode("utf-8")
    except UnicodeDecodeError as error:
        raise BootstrapError(
            f"CURRENT fresh-zero migration source is not UTF-8: {migration_file}"
        ) from error
    if "\x00" in text:
        fail("CURRENT fresh-zero migration source contains a NUL byte")
    if FORBIDDEN_FK_DISABLE_RE.search(text):
        fail("CURRENT fresh-zero migration source disables foreign key enforcement")
    return path


def validate_current_fresh_zero_projection(
    projection: Any,
    repository_root: Path = ROOT,
) -> tuple[dict[str, Any], list[Path]]:
    """Verify that materialization consumes, but never re-selects, the typed D1 CURRENT target."""
    if not isinstance(projection, dict):
        fail("CURRENT D1 repository projection must be one object")
    repository_identity = projection.get("repository_identity_sha256")
    if not isinstance(repository_identity, str) or SHA256_RE.fullmatch(repository_identity) is None:
        fail("CURRENT D1 repository identity is malformed")

    authority = projection.get("executable_schema_authority")
    if (
        not isinstance(authority, list)
        or not authority
        or any(not isinstance(root, str) or not root for root in authority)
        or len(set(authority)) != len(authority)
    ):
        fail("CURRENT D1 executable schema authority is malformed")

    envelope = require_exact_keys(
        projection.get("fresh_zero_construction"),
        {"construction", "construction_sha256"},
        "CURRENT fresh-zero construction envelope",
    )
    construction = require_exact_keys(
        envelope["construction"],
        {
            "schema_version",
            "kind",
            "component_id",
            "repository_identity_sha256",
            "target_schema_revision",
            "migration_sources",
            "deferred_revisions",
            "provider_mutation_authorized",
            "production_mutation_authorized",
        },
        "CURRENT fresh-zero construction",
    )
    if construction["schema_version"] != 1 or isinstance(construction["schema_version"], bool):
        fail("CURRENT fresh-zero construction schema version is unsupported")
    if construction["kind"] != "D1_CURRENT_FRESH_ZERO_CONSTRUCTION":
        fail("CURRENT fresh-zero construction kind is not authoritative")
    if construction["component_id"] != "catalog":
        fail("CURRENT fresh-zero construction must target Catalog")
    if construction["repository_identity_sha256"] != repository_identity:
        fail("CURRENT fresh-zero construction is detached from repository identity")
    if construction["provider_mutation_authorized"] is not False:
        fail("CURRENT fresh-zero construction must not authorize provider mutation")
    if construction["production_mutation_authorized"] is not False:
        fail("CURRENT fresh-zero construction must not authorize Production mutation")

    construction_digest = envelope["construction_sha256"]
    if not isinstance(construction_digest, str) or SHA256_RE.fullmatch(construction_digest) is None:
        fail("CURRENT fresh-zero construction digest is malformed")
    if canonical_json_sha256(construction) != construction_digest:
        fail("CURRENT fresh-zero construction digest does not bind the exact manifest")

    target = construction["target_schema_revision"]
    if not isinstance(target, str) or MIGRATION_RE.fullmatch(target) is None:
        fail("CURRENT fresh-zero target schema revision is malformed")

    components = projection.get("components")
    if not isinstance(components, list):
        fail("CURRENT D1 repository projection is missing components")
    catalog_components = [
        component
        for component in components
        if isinstance(component, dict) and component.get("component_id") == "catalog"
    ]
    if len(catalog_components) != 1:
        fail("CURRENT D1 repository projection must contain exactly one Catalog component")
    catalog = catalog_components[0]
    release_contract = catalog.get("release_schema_contract")
    if not isinstance(release_contract, dict):
        fail("CURRENT Catalog projection is missing release schema contract")
    if release_contract.get("target_schema_revision") != target:
        fail("CURRENT fresh-zero target differs from the typed Catalog release target")

    executable = catalog.get("executable_migration_sources")
    if not isinstance(executable, list) or not executable:
        fail("CURRENT Catalog executable migration source lineage is missing")
    normalized_executable: list[dict[str, str]] = []
    for entry in executable:
        exact = require_exact_keys(
            entry,
            {"migration_file", "source_root"},
            "CURRENT Catalog executable migration source",
        )
        migration_file = exact["migration_file"]
        source_root = exact["source_root"]
        if not isinstance(migration_file, str) or not isinstance(source_root, str):
            fail("CURRENT Catalog executable migration source has non-string identity")
        normalized_executable.append(
            {"migration_file": migration_file, "source_root": source_root}
        )

    target_positions = [
        index
        for index, entry in enumerate(normalized_executable)
        if entry["migration_file"] == target
    ]
    if len(target_positions) != 1:
        fail("CURRENT fresh-zero target is missing or ambiguous in executable Catalog lineage")
    target_index = target_positions[0]
    expected_prefix = normalized_executable[: target_index + 1]
    expected_deferred = [
        entry["migration_file"] for entry in normalized_executable[target_index + 1 :]
    ]

    sources = construction["migration_sources"]
    if not isinstance(sources, list) or len(sources) != len(expected_prefix):
        fail("CURRENT fresh-zero materialized prefix differs from the typed Catalog target prefix")
    paths: list[Path] = []
    names: list[str] = []
    for index, source in enumerate(sources):
        exact = require_exact_keys(
            source,
            {"migration_file", "source_root", "sha256"},
            "CURRENT fresh-zero migration source",
        )
        migration_file = exact["migration_file"]
        source_root = exact["source_root"]
        digest = exact["sha256"]
        if (
            migration_file != expected_prefix[index]["migration_file"]
            or source_root != expected_prefix[index]["source_root"]
        ):
            fail("CURRENT fresh-zero migration sources re-ordered or substituted typed lineage")
        if source_root not in authority:
            fail("CURRENT fresh-zero migration source escaped executable schema authority")
        path = current_source_file(repository_root, source_root, migration_file, digest)
        paths.append(path)
        names.append(migration_file)

    if not names or names[-1] != target or len(set(names)) != len(names):
        fail("CURRENT fresh-zero materialized prefix has an invalid terminal target or duplicate source")
    deferred = construction["deferred_revisions"]
    if deferred != expected_deferred:
        fail("CURRENT fresh-zero deferred revisions differ from typed Catalog lineage")
    if any(name in names for name in deferred):
        fail("CURRENT fresh-zero materialized prefix includes a deferred revision")
    return construction, paths


def build_current_fresh_zero_bootstrap_bytes(
    projection: Any,
    repository_root: Path = ROOT,
) -> bytes:
    """Render final CREATE statements, never historical ALTER/DROP operations on the target."""
    construction, paths = validate_current_fresh_zero_projection(projection, repository_root)
    schema = sqlite3.connect(":memory:")
    try:
        schema.execute("PRAGMA foreign_keys=ON")
        apply_sequential_with_ledger(schema, paths)
        assert_foreign_keys_clean(schema, "baseline source")
        assert_integrity_clean(schema, "baseline source")
        for table in application_tables(schema):
            if schema.execute(f"SELECT 1 FROM {quote_identifier(table)} LIMIT 1").fetchone():
                fail(f"baseline source contains seed rows requiring an explicit preservation contract: {table}")
        objects = schema.execute(
            "SELECT sql FROM sqlite_master WHERE sql IS NOT NULL AND name NOT GLOB 'sqlite_*' "
            "AND name <> ? ORDER BY CASE type WHEN 'table' THEN 0 WHEN 'view' THEN 1 "
            "WHEN 'index' THEN 2 WHEN 'trigger' THEN 3 ELSE 4 END, name",
            (LEDGER_NAME,),
        ).fetchall()
        parts = [empty_guard_sql(), ledger_sql(), "-- CURRENT final schema; historical transitions run offline only.\n"]
        parts.extend(str(row[0]).rstrip().rstrip(";") + ";\n" for row in objects)
    finally:
        schema.close()
    for source in construction["migration_sources"]:
        escaped_name = source["migration_file"].replace("'", "''")
        parts.append(f'INSERT INTO "{LEDGER_NAME}" (name) VALUES (\'{escaped_name}\');\n')
    document = "\n".join(parts)
    if not document.endswith("\n"):
        document += "\n"
    return document.encode("utf-8")



# D1/wrangler impose a limit on a single SQL execution payload, distinct from
# the SQLite schema limit. Keep immutable CURRENT baseline bytes for hashing
# and audit; render bounded statement-complete chunks for provider execution.
# Never split inside CREATE TRIGGER ... BEGIN ... END.
MAX_BOOTSTRAP_PART_BYTES = 64 * 1024


def bootstrap_statements(payload: bytes) -> list[bytes]:
    document = payload.decode("utf-8")
    pending = ""
    statements: list[bytes] = []
    for line in document.splitlines(keepends=True):
        pending += line
        if sqlite3.complete_statement(pending):
            statements.append(pending.encode("utf-8"))
            pending = ""
    if pending.strip():
        fail("CURRENT baseline contains an incomplete SQL statement")
    if pending:
        # Trailing whitespace belongs to the last chunk and is authenticated.
        if not statements:
            fail("CURRENT baseline contains no complete SQL statements")
        statements[-1] += pending.encode("utf-8")
    if not statements or b"".join(statements) != payload:
        fail("CURRENT baseline statement framing changed canonical bytes")
    return statements


def bootstrap_parts(payload: bytes) -> list[bytes]:
    chunks: list[bytes] = []
    pending = b""
    for statement in bootstrap_statements(payload):
        if len(statement) > MAX_BOOTSTRAP_PART_BYTES:
            fail("CURRENT baseline contains one oversized unsplittable SQL statement")
        if pending and len(pending) + len(statement) > MAX_BOOTSTRAP_PART_BYTES:
            chunks.append(pending)
            pending = b""
        pending += statement
    if pending:
        chunks.append(pending)
    if not chunks or b"".join(chunks) != payload:
        fail("CURRENT baseline chunks do not reproduce exact canonical bytes")
    return chunks


def write_bootstrap_parts(destination: Path, payload: bytes) -> list[Path]:
    # A partial or substituted output must never be reused as provider input.
    if destination.is_symlink() or destination.exists():
        fail("CURRENT baseline chunk destination already exists")
    destination.mkdir(parents=True)
    chunks = bootstrap_parts(payload)
    paths: list[Path] = []
    manifest = {"schema_version": 1, "source_sha256": sha256_bytes(payload), "parts": []}
    for index, chunk in enumerate(chunks, start=1):
        name = f"part-{index:04d}.sql"
        path = destination / name
        path.write_bytes(chunk)
        paths.append(path)
        manifest["parts"].append({"name": name, "sha256": sha256_bytes(chunk), "bytes": len(chunk)})
    (destination / "manifest.json").write_text(
        json.dumps(manifest, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )
    verify_bootstrap_parts(destination, payload)
    return paths


def verify_bootstrap_parts(directory: Path, expected: bytes) -> list[Path]:
    if directory.is_symlink() or not directory.is_dir():
        fail("CURRENT baseline chunk directory is not a regular directory")
    manifest_path = directory / "manifest.json"
    if manifest_path.is_symlink() or not manifest_path.is_file():
        fail("CURRENT baseline chunk manifest is missing or substituted")
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (ValueError, OSError) as error:
        raise BootstrapError("CURRENT baseline chunk manifest is invalid") from error
    chunks = bootstrap_parts(expected)
    if not isinstance(manifest, dict) or set(manifest) != {"schema_version", "source_sha256", "parts"}:
        fail("CURRENT baseline chunk manifest schema drift")
    if manifest["schema_version"] != 1 or manifest["source_sha256"] != sha256_bytes(expected):
        fail("CURRENT baseline chunk source identity differs from canonical authority")
    rows = manifest["parts"]
    if not isinstance(rows, list) or len(rows) != len(chunks):
        fail("CURRENT baseline chunk manifest count is incomplete")
    names = {"manifest.json"}
    paths: list[Path] = []
    for index, (row, chunk) in enumerate(zip(rows, chunks, strict=True), start=1):
        name = f"part-{index:04d}.sql"
        if not isinstance(row, dict) or row != {
            "name": name, "sha256": sha256_bytes(chunk), "bytes": len(chunk)
        }:
            fail("CURRENT baseline chunk manifest does not match deterministic partition")
        path = directory / name
        if path.is_symlink() or not path.is_file() or path.read_bytes() != chunk:
            fail("CURRENT baseline part differs from typed immutable SQL")
        paths.append(path)
        names.add(name)
    if {p.name for p in directory.iterdir()} != names:
        fail("CURRENT baseline chunk directory contains an unexpected file")
    return paths

def prove_current_fresh_zero_convergence(
    projection: Any,
    repository_root: Path = ROOT,
) -> dict[str, Any]:
    construction, paths = validate_current_fresh_zero_projection(projection, repository_root)
    first = build_current_fresh_zero_bootstrap_bytes(projection, repository_root)
    second = build_current_fresh_zero_bootstrap_bytes(projection, repository_root)
    if first != second:
        fail("CURRENT fresh-zero construction produced non-deterministic bootstrap bytes")

    expected_ledger = [entry["migration_file"] for entry in construction["migration_sources"]]
    sequential = sqlite3.connect(":memory:")
    bootstrap = sqlite3.connect(":memory:")
    try:
        sequential.execute("PRAGMA foreign_keys = ON")
        bootstrap.execute("PRAGMA foreign_keys = ON")
        if sequential.execute("PRAGMA foreign_keys").fetchone() != (1,):
            fail("CURRENT fresh-zero sequential proof did not enable foreign key enforcement")
        if bootstrap.execute("PRAGMA foreign_keys").fetchone() != (1,):
            fail("CURRENT fresh-zero bootstrap proof did not enable foreign key enforcement")

        apply_sequential_with_ledger(sequential, paths)
        bootstrap.executescript(first.decode("utf-8"))
        bootstrap.commit()
        assert_foreign_keys_clean(sequential, "CURRENT fresh-zero sequential")
        assert_foreign_keys_clean(bootstrap, "CURRENT fresh-zero bootstrap")
        assert_integrity_clean(sequential, "CURRENT fresh-zero sequential")
        assert_integrity_clean(bootstrap, "CURRENT fresh-zero bootstrap")
        if ledger_names(sequential) != expected_ledger or ledger_names(bootstrap) != expected_ledger:
            fail("CURRENT fresh-zero ledger differs from the exact typed construction")

        sequential_state = normalized_schema_state(sequential)
        bootstrap_state = normalized_schema_state(bootstrap)
        if sequential_state != bootstrap_state:
            fail("CURRENT fresh-zero bootstrap does not converge with exact sequential sources")

        state_before_replay = normalized_schema_state(bootstrap)
        expect_rejected(
            "CURRENT fresh-zero bootstrap replay",
            lambda: bootstrap.executescript(first.decode("utf-8")),
        )
        if normalized_schema_state(bootstrap) != state_before_replay:
            fail("rejected CURRENT fresh-zero bootstrap replay changed schema or ledger state")

        return {
            "component": "catalog",
            "target_schema_revision": construction["target_schema_revision"],
            "migration_count": len(paths),
            "bootstrap_sha256": sha256_bytes(first),
            "construction_sha256": projection["fresh_zero_construction"]["construction_sha256"],
            "schema_signature": sequential_state["schema_signature"],
            "foreign_key_check": "CLEAN",
            "integrity_check": "ok",
            "replay": "REJECTED",
            "provider_mutation_authorized": False,
            "production_mutation_authorized": False,
        }
    finally:
        sequential.close()
        bootstrap.close()


def rebind_current_construction_digest(projection: dict[str, Any]) -> None:
    construction = projection["fresh_zero_construction"]["construction"]
    projection["fresh_zero_construction"]["construction_sha256"] = canonical_json_sha256(construction)


def current_fresh_zero_self_test() -> None:
    projection = current_repository_projection()
    prove_current_fresh_zero_convergence(projection)
    payload = build_current_fresh_zero_bootstrap_bytes(projection)
    with tempfile.TemporaryDirectory(prefix="cloudflare-d1-bootstrap-chunks-") as temporary:
        output = Path(temporary) / "parts"
        original = write_bootstrap_parts(output, payload)
        if b"".join(path.read_bytes() for path in original) != payload:
            fail("CURRENT baseline execution partition changed final schema bytes")
        first = original[0]
        untouched = first.read_bytes()
        first.write_bytes(untouched + b"-- substituted")
        expect_rejected(
            "CURRENT baseline chunk substitution",
            lambda: verify_bootstrap_parts(output, payload),
        )
        first.write_bytes(untouched)
        if verify_bootstrap_parts(output, payload) != original:
            fail("CURRENT baseline chunk restoration was not deterministic")
    if b"ALTER TABLE" in payload.upper() or b"DROP TABLE" in payload.upper():
        fail("CURRENT empty baseline still replays historical table transitions")
    prove_fk_corruption_detected()
    with tempfile.TemporaryDirectory(prefix="cloudflare-d1-seed-") as temporary:
        root = Path(temporary)
        seeded = json.loads(json.dumps(projection))
        construction, paths = validate_current_fresh_zero_projection(projection)
        for entry, path in zip(construction["migration_sources"], paths, strict=True):
            target = root / entry["source_root"] / entry["migration_file"]
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(path.read_bytes())
        last = seeded["fresh_zero_construction"]["construction"]["migration_sources"][-1]
        target = root / last["source_root"] / last["migration_file"]
        target.write_bytes(target.read_bytes() + b"\nCREATE TABLE baseline_seed_fixture(value TEXT);\n"
                           b"INSERT INTO baseline_seed_fixture VALUES ('must survive');\n")
        last["sha256"] = sha256_bytes(target.read_bytes())
        rebind_current_construction_digest(seeded)
        expect_rejected("seed rows cannot be silently discarded",
                        lambda: build_current_fresh_zero_bootstrap_bytes(seeded, root))
    for reserved in (False, True):
        connection = sqlite3.connect(":memory:")
        try:
            connection.execute("PRAGMA foreign_keys=ON")
            if reserved:
                connection.execute('CREATE TABLE "_cf_KV" (key TEXT PRIMARY KEY,value BLOB) WITHOUT ROWID')
            connection.executescript(payload.decode("utf-8"))
            before = normalized_schema_state(connection)
            expect_rejected("CURRENT baseline replay", lambda: connection.executescript(payload.decode("utf-8")))
            if normalized_schema_state(connection) != before:
                fail("rejected CURRENT baseline replay changed schema or ledger")
        finally:
            connection.close()
    for table in ("existing", "sqlitexexisting"):
        connection = sqlite3.connect(":memory:")
        try:
            connection.execute(f"CREATE TABLE {table}(id INTEGER PRIMARY KEY)")
            expect_rejected("nonempty CURRENT baseline target", lambda: connection.executescript(payload.decode("utf-8")))
            if connection.execute("SELECT name FROM sqlite_master WHERE type='table'").fetchall() != [(table,)]:
                fail("CURRENT baseline modified a nonempty target")
        finally:
            connection.close()

    stale_digest = json.loads(json.dumps(projection))
    stale_digest["fresh_zero_construction"]["construction_sha256"] = "0" * 64
    expect_rejected(
        "CURRENT construction digest mismatch",
        lambda: validate_current_fresh_zero_projection(stale_digest),
    )

    provider_authorized = json.loads(json.dumps(projection))
    provider_authorized["fresh_zero_construction"]["construction"]["provider_mutation_authorized"] = True
    rebind_current_construction_digest(provider_authorized)
    expect_rejected(
        "CURRENT construction provider authorization expansion",
        lambda: validate_current_fresh_zero_projection(provider_authorized),
    )

    production_authorized = json.loads(json.dumps(projection))
    production_authorized["fresh_zero_construction"]["construction"]["production_mutation_authorized"] = True
    rebind_current_construction_digest(production_authorized)
    expect_rejected(
        "CURRENT construction Production authorization expansion",
        lambda: validate_current_fresh_zero_projection(production_authorized),
    )

    source_tamper = json.loads(json.dumps(projection))
    source_tamper["fresh_zero_construction"]["construction"]["migration_sources"][-1]["sha256"] = "0" * 64
    rebind_current_construction_digest(source_tamper)
    expect_rejected(
        "CURRENT construction source digest mismatch",
        lambda: validate_current_fresh_zero_projection(source_tamper),
    )

    reordered = json.loads(json.dumps(projection))
    reordered_sources = reordered["fresh_zero_construction"]["construction"]["migration_sources"]
    reordered_sources[-1], reordered_sources[-2] = reordered_sources[-2], reordered_sources[-1]
    rebind_current_construction_digest(reordered)
    expect_rejected(
        "CURRENT construction source ordering mismatch",
        lambda: validate_current_fresh_zero_projection(reordered),
    )

    missing = json.loads(json.dumps(projection))
    missing["fresh_zero_construction"]["construction"]["migration_sources"].pop()
    rebind_current_construction_digest(missing)
    expect_rejected(
        "CURRENT construction missing source",
        lambda: validate_current_fresh_zero_projection(missing),
    )

    duplicate = json.loads(json.dumps(projection))
    duplicate_sources = duplicate["fresh_zero_construction"]["construction"]["migration_sources"]
    duplicate_sources.append(json.loads(json.dumps(duplicate_sources[-1])))
    rebind_current_construction_digest(duplicate)
    expect_rejected(
        "CURRENT construction duplicate source",
        lambda: validate_current_fresh_zero_projection(duplicate),
    )

    alternate_target = json.loads(json.dumps(projection))
    alternate_sources = alternate_target["fresh_zero_construction"]["construction"]["migration_sources"]
    alternate_target["fresh_zero_construction"]["construction"]["target_schema_revision"] = alternate_sources[-2]["migration_file"]
    rebind_current_construction_digest(alternate_target)
    expect_rejected(
        "CURRENT construction alternate historical target",
        lambda: validate_current_fresh_zero_projection(alternate_target),
    )

    deferred_materialized = json.loads(json.dumps(projection))
    catalog = next(
        component
        for component in deferred_materialized["components"]
        if component.get("component_id") == "catalog"
    )
    # Exercise a deferred suffix even when CURRENT materializes the full lineage.
    deferred_source = {
        "migration_file": "9999_deferred_fixture.sql",
        "source_root": catalog["executable_migration_sources"][-1]["source_root"],
    }
    catalog["executable_migration_sources"].append(deferred_source)
    deferred_materialized["fresh_zero_construction"]["construction"]["deferred_revisions"].append(
        deferred_source["migration_file"]
    )
    rebind_current_construction_digest(deferred_materialized)
    validate_current_fresh_zero_projection(deferred_materialized)
    deferred_materialized["fresh_zero_construction"]["construction"]["migration_sources"].append(
        {
            "migration_file": deferred_source["migration_file"],
            "source_root": deferred_source["source_root"],
            "sha256": "0" * 64,
        }
    )
    rebind_current_construction_digest(deferred_materialized)
    expect_rejected(
        "CURRENT construction materialized deferred revision",
        lambda: validate_current_fresh_zero_projection(deferred_materialized),
    )

    expect_rejected(
        "CURRENT source path escape",
        lambda: current_source_file(ROOT, "../escape", "0001_escape.sql", "0" * 64),
    )

    with tempfile.TemporaryDirectory(prefix="cloudflare-d1-current-symlink-") as temporary:
        root = Path(temporary)
        real = root / "real"
        real.mkdir()
        source = real / "0001_fixture.sql"
        source.write_text("SELECT 1;\n", encoding="utf-8")
        link = root / "link"
        link.symlink_to(real, target_is_directory=True)
        expect_rejected(
            "CURRENT source root symlink",
            lambda: current_source_file(
                root,
                "link",
                source.name,
                sha256_bytes(source.read_bytes()),
            ),
        )

    print("CURRENT typed fresh-zero materialization and negative self-tests passed.")


def check_repository_policy() -> None:
    proof = prove_current_fresh_zero_convergence(current_repository_projection())
    print(f"CURRENT empty-D1 baseline passed: {json.dumps(proof, sort_keys=True, separators=(',', ':'))}.")


def self_test() -> None:
    current_fresh_zero_self_test()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("check")
    subparsers.add_parser("self-test")

    build = subparsers.add_parser("build")
    build.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    build.add_argument("--chunk-directory", type=Path, default=None)

    verify = subparsers.add_parser("verify")
    verify.add_argument("--file", type=Path, required=True)
    verify.add_argument("--chunk-directory", type=Path, default=None)

    args = parser.parse_args()
    if args.command == "check":
        check_repository_policy()
        return 0
    if args.command == "self-test":
        self_test()
        return 0
    if args.command == "build":
        payload = build_current_fresh_zero_bootstrap_bytes(current_repository_projection())
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_bytes(payload)
        if args.chunk_directory is not None:
            parts = write_bootstrap_parts(args.chunk_directory, payload)
            print(f"CURRENT provider-safe baseline chunks: count={len(parts)} max_bytes={MAX_BOOTSTRAP_PART_BYTES}")
        print(f"Built CURRENT empty-D1 baseline: path={args.output} bytes={len(payload)} sha256={sha256_bytes(payload)}")
        return 0
    if args.command == "verify":
        expected = build_current_fresh_zero_bootstrap_bytes(current_repository_projection())
        actual = args.file.read_bytes()
        if actual != expected:
            fail("bootstrap SQL differs from exact current migration authority")
        if args.chunk_directory is not None:
            verify_bootstrap_parts(args.chunk_directory, expected)
        print(f"Verified deterministic empty-D1 bootstrap sha256={sha256_bytes(actual)}.")
        return 0
    fail(f"unsupported command: {args.command}")


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except BootstrapError as error:
        raise SystemExit(f"Empty-D1 bootstrap rejected: {error}") from error
