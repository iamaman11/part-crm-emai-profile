#!/usr/bin/env python3
"""Execute the repository-local Phase 2I D1-compatible backup/restore drill."""

from __future__ import annotations

import hashlib
import json
import runpy
import shutil
import sqlite3
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
D1_SCHEMA_TEST = ROOT / "scripts" / "test-d1-schema.py"


def load_schema_helpers() -> dict[str, object]:
    helpers = runpy.run_path(str(D1_SCHEMA_TEST), run_name="phase2i_d1_schema_helpers")
    required = {
        "open_database",
        "apply_migrations",
        "seed_catalog",
        "schema_signature",
    }
    missing = required - set(helpers)
    if missing:
        raise AssertionError(f"D1 schema helper surface changed unexpectedly: {sorted(missing)}")
    return helpers


def database_digest(connection: sqlite3.Connection) -> str:
    """Hash the logical database without emitting synthetic row contents."""
    dump = "\n".join(connection.iterdump()).encode("utf-8")
    return hashlib.sha256(dump).hexdigest()


def integrity_ok(connection: sqlite3.Connection) -> bool:
    row = connection.execute("PRAGMA integrity_check").fetchone()
    return row is not None and str(row[0]).lower() == "ok"


def restore_from_logical_dump(
    logical_dump: str,
    destination: Path,
) -> sqlite3.Connection:
    connection = sqlite3.connect(destination)
    connection.row_factory = sqlite3.Row
    connection.executescript(logical_dump)
    connection.execute("PRAGMA foreign_keys = ON")
    connection.commit()
    return connection


def corrupted_backup_is_rejected(source: Path, destination: Path) -> bool:
    payload = source.read_bytes()
    if len(payload) < 512:
        raise AssertionError("repository-local D1 backup is unexpectedly tiny")
    destination.write_bytes(payload[: max(256, len(payload) // 2)])
    try:
        connection = sqlite3.connect(destination)
        try:
            row = connection.execute("PRAGMA integrity_check").fetchone()
            return row is None or str(row[0]).lower() != "ok"
        finally:
            connection.close()
    except sqlite3.DatabaseError:
        return True


def main() -> int:
    helpers = load_schema_helpers()
    open_database = helpers["open_database"]
    apply_migrations = helpers["apply_migrations"]
    seed_catalog = helpers["seed_catalog"]
    schema_signature = helpers["schema_signature"]

    with tempfile.TemporaryDirectory(prefix="phase2i-d1-dr-") as temporary:
        root = Path(temporary)
        source_path = root / "catalog-source.sqlite3"
        backup_path = root / "catalog-backup.sqlite3"
        physical_restore_path = root / "catalog-physical-restore.sqlite3"
        logical_restore_path = root / "catalog-logical-restore.sqlite3"
        corrupt_path = root / "catalog-corrupt.sqlite3"

        source = open_database(source_path)
        try:
            apply_migrations(source)
            seed_catalog(source)
            assert integrity_ok(source), "seeded source D1-compatible catalog failed integrity_check"
            source_schema = schema_signature(source)
            source_digest = database_digest(source)
            logical_dump = "\n".join(source.iterdump())

            backup = open_database(backup_path)
            try:
                source.backup(backup)
                backup.commit()
                assert integrity_ok(backup), "physical backup failed integrity_check"
                assert schema_signature(backup) == source_schema, "physical backup schema drifted"
                assert database_digest(backup) == source_digest, "physical backup data drifted"
            finally:
                backup.close()

            # Move the live source forward after the point-in-time backup. A valid restore must
            # recover the captured snapshot, not accidentally mirror later live state.
            source.execute(
                "UPDATE tenants SET display_name = 'Post-backup mutation' WHERE tenant_id = ?",
                (helpers["TENANT_A"],),
            )
            source.commit()
            assert database_digest(source) != source_digest, "post-backup mutation was not observed"
        finally:
            source.close()

        backup_source = open_database(backup_path)
        physical_restore = open_database(physical_restore_path)
        try:
            backup_source.backup(physical_restore)
            physical_restore.commit()
            assert integrity_ok(physical_restore), "physical restore failed integrity_check"
            assert schema_signature(physical_restore) == source_schema, "physical restore schema drifted"
            assert database_digest(physical_restore) == source_digest, "physical restore data drifted"
        finally:
            physical_restore.close()
            backup_source.close()

        logical_restore = restore_from_logical_dump(logical_dump, logical_restore_path)
        try:
            assert integrity_ok(logical_restore), "logical export restore failed integrity_check"
            assert schema_signature(logical_restore) == source_schema, "logical restore schema drifted"
            assert database_digest(logical_restore) == source_digest, "logical restore data drifted"
        finally:
            logical_restore.close()

        # A raw byte-for-byte copy is intentionally used only to manufacture a corrupt fixture;
        # the accepted backup path above is SQLite's consistent backup API / logical export.
        shutil.copyfile(backup_path, corrupt_path)
        corrupt_path.unlink()
        if not corrupted_backup_is_rejected(backup_path, corrupt_path):
            raise AssertionError("truncated D1-compatible backup unexpectedly passed integrity validation")

    print(
        json.dumps(
            {
                "d1BackupRestore": "passed",
                "corruptBackupRejected": True,
                "productionReady": False,
                "scope": "repository-local",
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
