use std::path::Path;
use std::process::Command;

const QUERY_PLAN_PROBE: &str = r#"
import sqlite3
import sys
from pathlib import Path

repo_root = Path(sys.argv[1])
migrations = sorted((repo_root / "migrations" / "resolver-d1").glob("[0-9][0-9][0-9][0-9]_*.sql"))
if not migrations:
    raise AssertionError("resolver D1 migrations are missing")

connection = sqlite3.connect(":memory:")
try:
    for migration in migrations:
        connection.executescript(migration.read_text(encoding="utf-8"))

    probes = (
        (
            "resolver_idempotency_records",
            """
            SELECT idempotency_digest, request_sha256, hmac_version
            FROM resolver_idempotency_records
            WHERE tenant_id = ? AND operation = ?
              AND idempotency_digest IN (?, ?, ?, ?)
            """,
            ("tenant_probe", "claim", "a" * 64, "b" * 64, "c" * 64, "d" * 64),
        ),
        (
            "resolver_encrypted_records",
            """
            SELECT lookup_digest, lookup_hmac_version, logical_id
            FROM resolver_encrypted_records
            WHERE tenant_id = ? AND record_kind = ?
              AND lookup_digest IN (?, ?, ?, ?)
            """,
            ("tenant_probe", "credential", "a" * 64, "b" * 64, "c" * 64, "d" * 64),
        ),
    )

    for table, sql, params in probes:
        details = [
            row[3]
            for row in connection.execute("EXPLAIN QUERY PLAN " + sql, params).fetchall()
        ]
        if any(f"SCAN {table}" in detail for detail in details):
            raise AssertionError(f"{table} lookup regressed to a table scan: {details}")
        if not any(f"SEARCH {table}" in detail for detail in details):
            raise AssertionError(f"{table} lookup is not an indexed search: {details}")
        print(f"{table}: {' | '.join(details)}")
finally:
    connection.close()
"#;

#[test]
fn retained_lookup_queries_are_indexed_bounded_searches() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("resolver crate must remain under apps/<crate>");

    let output = Command::new("python")
        .arg("-c")
        .arg(QUERY_PLAN_PROBE)
        .arg(repo_root)
        .output()
        .expect("python is required by the repository acceptance toolchain");

    assert!(
        output.status.success(),
        "resolver lookup query-plan evidence failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
