#!/usr/bin/env python3
"""Canonical read-only Cloudflare D1 provider adapter.

This module owns transport normalization only. It never decides D1 migration,
compatibility, recovery, authorization, or execution semantics; those remain in
the existing credential-free D1 owners.

The adapter intentionally exposes one bounded observation command with a fixed
set of read-only provider operations:

* GET exact D1 database identity;
* POST fixed read-only SQL to the D1 Query API;
* GET a Time Travel bookmark;
* derive the local-minus-remote pending-name diagnostic from the typed D1
  repository projection.

No arbitrary SQL, mutation endpoint, restore endpoint, token-management surface,
or Production target is accepted.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

API_ROOT = "https://api.cloudflare.com/client/v4"
TOKEN_ENV = "CLOUDFLARE_OBSERVE_API_TOKEN"
ACCOUNT_RE = re.compile(r"[0-9a-f]{32}")
DATABASE_RE = re.compile(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
MIGRATION_RE = re.compile(r"[0-9]{4}_[a-z0-9_]+\.sql")

LEDGER_TABLE_SQL = "SELECT name FROM sqlite_master WHERE type='table' AND name='d1_migrations'"
LEDGER_SQL = "SELECT id, name FROM d1_migrations ORDER BY id"
FOREIGN_KEY_SQL = "PRAGMA foreign_key_check"
QUICK_CHECK_SQL = "PRAGMA quick_check"


class ObservationError(ValueError):
    pass


def _write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n", encoding="utf-8")


def _load_json(path: Path, label: str) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ObservationError(f"{label} is unavailable or invalid JSON: {path}") from exc


def _require_success_envelope(value: Any, label: str) -> Any:
    if not isinstance(value, dict):
        raise ObservationError(f"{label} response is not one JSON object")
    if value.get("success") is not True:
        raise ObservationError(f"{label} response success is not true")
    errors = value.get("errors")
    if errors not in (None, []):
        raise ObservationError(f"{label} response contains provider errors")
    if "result" not in value:
        raise ObservationError(f"{label} response is missing result")
    return value["result"]


def _require_read_query(value: Any, label: str) -> dict[str, Any]:
    result = _require_success_envelope(value, label)
    if not isinstance(result, list) or len(result) != 1 or not isinstance(result[0], dict):
        raise ObservationError(f"{label} query response is not exactly one result object")
    query = result[0]
    if query.get("success") is not True:
        raise ObservationError(f"{label} query result success is not true")
    rows = query.get("results")
    if not isinstance(rows, list):
        raise ObservationError(f"{label} query result rows are not an array")
    meta = query.get("meta")
    if not isinstance(meta, dict):
        raise ObservationError(f"{label} query result meta is missing")
    if meta.get("changed_db") is not False:
        raise ObservationError(f"{label} query did not prove changed_db=false")
    if meta.get("changes") != 0:
        raise ObservationError(f"{label} query did not prove changes=0")
    rows_written = meta.get("rows_written")
    if rows_written is not None and rows_written != 0:
        raise ObservationError(f"{label} query reports rows_written != 0")
    return query


def _timestamp_query(unix_seconds: str | None) -> str:
    if not unix_seconds:
        return ""
    if re.fullmatch(r"[0-9]{10}", unix_seconds) is None:
        raise ObservationError("time-travel timestamp must be exactly 10 decimal Unix-seconds digits")
    value = int(unix_seconds)
    try:
        instant = datetime.fromtimestamp(value, tz=timezone.utc)
    except (OverflowError, OSError, ValueError) as exc:
        raise ObservationError("time-travel timestamp is outside supported datetime range") from exc
    return "?" + urllib.parse.urlencode({"timestamp": instant.isoformat().replace("+00:00", "Z")})


def _typed_local_migrations(repository_json: Path, root: Path) -> list[str]:
    projection = _load_json(repository_json, "typed D1 repository projection")
    if not isinstance(projection, dict):
        raise ObservationError("typed D1 repository projection is not one object")
    authority = projection.get("executable_schema_authority")
    if (
        not isinstance(authority, list)
        or not authority
        or any(not isinstance(item, str) or not item for item in authority)
        or len(authority) != len(set(authority))
    ):
        raise ObservationError("typed D1 executable_schema_authority is missing or invalid")
    components = projection.get("components")
    if not isinstance(components, list):
        raise ObservationError("typed D1 repository projection is missing components")
    matches = [item for item in components if isinstance(item, dict) and item.get("component_id") == "catalog"]
    if len(matches) != 1:
        raise ObservationError("typed Catalog projection is missing or ambiguous")
    sources = matches[0].get("executable_migration_sources")
    if not isinstance(sources, list) or not sources:
        raise ObservationError("typed Catalog executable_migration_sources is missing")

    root = root.resolve()
    allowed_roots = set(authority)
    names: list[str] = []
    for item in sources:
        if not isinstance(item, dict):
            raise ObservationError("typed Catalog migration source entry is invalid")
        name = item.get("migration_file")
        source_root = item.get("source_root")
        if not isinstance(name, str) or MIGRATION_RE.fullmatch(name) is None:
            raise ObservationError("typed Catalog migration_file is invalid")
        if not isinstance(source_root, str) or source_root not in allowed_roots:
            raise ObservationError("typed Catalog migration source escaped executable schema authority")
        source_dir = (root / source_root).resolve()
        path = source_dir / name
        if not source_dir.is_relative_to(root) or path.is_symlink() or not path.is_file():
            raise ObservationError(f"typed Catalog migration source is missing or unsafe: {source_root}/{name}")
        resolved = path.resolve()
        if not resolved.is_relative_to(source_dir):
            raise ObservationError(f"typed Catalog migration source escapes source root: {source_root}/{name}")
        names.append(name)
    if len(names) != len(set(names)):
        raise ObservationError("typed Catalog executable migration names are duplicated")
    return names


class CloudflareReadClient:
    def __init__(self, token: str, *, attempts: int = 3, timeout_seconds: int = 20) -> None:
        if not token:
            raise ObservationError(f"{TOKEN_ENV} is required")
        self._token = token
        self._attempts = attempts
        self._timeout = timeout_seconds

    def request(self, method: str, path: str, *, body: dict[str, Any] | None = None) -> tuple[int, Any]:
        if method not in {"GET", "POST"}:
            raise ObservationError(f"unsupported read adapter method: {method}")
        data = None if body is None else json.dumps(body, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
        headers = {
            "Authorization": f"Bearer {self._token}",
            "Accept": "application/json",
            "User-Agent": "part-crm-d1-read-observer/1",
        }
        if data is not None:
            headers["Content-Type"] = "application/json"
        url = API_ROOT + path
        last: Exception | None = None
        for attempt in range(self._attempts):
            request = urllib.request.Request(url=url, method=method, data=data, headers=headers)
            try:
                with urllib.request.urlopen(request, timeout=self._timeout) as response:
                    raw = response.read().decode("utf-8")
                    return int(response.status), json.loads(raw)
            except urllib.error.HTTPError as exc:
                raw = exc.read().decode("utf-8", errors="replace")
                if exc.code == 429 or 500 <= exc.code <= 599:
                    last = ObservationError(f"provider HTTP {exc.code}")
                else:
                    raise ObservationError(f"provider HTTP {exc.code}: {raw[:300]}") from exc
            except (urllib.error.URLError, TimeoutError, json.JSONDecodeError) as exc:
                last = exc
            if attempt + 1 < self._attempts:
                time.sleep(2 ** attempt)
        raise ObservationError(f"provider request failed after {self._attempts} attempts: {last}")


def _query(client: CloudflareReadClient, account_id: str, database_id: str, sql: str, label: str) -> tuple[dict[str, Any], dict[str, Any]]:
    status, envelope = client.request(
        "POST",
        f"/accounts/{account_id}/d1/database/{database_id}/query",
        body={"sql": sql},
    )
    if status != 200:
        raise ObservationError(f"{label} query returned HTTP {status}")
    query = _require_read_query(envelope, label)
    return query, envelope


def _migration_rows(query: dict[str, Any]) -> list[dict[str, Any]]:
    rows = query["results"]
    normalized: list[dict[str, Any]] = []
    seen_ids: set[int] = set()
    seen_names: set[str] = set()
    previous_id = 0
    for row in rows:
        if not isinstance(row, dict):
            raise ObservationError("remote migration ledger contains a non-object row")
        migration_id = row.get("id")
        name = row.get("name")
        if isinstance(migration_id, bool) or not isinstance(migration_id, int) or migration_id <= previous_id:
            raise ObservationError("remote migration ledger ids are not strictly increasing positive integers")
        if not isinstance(name, str) or not name:
            raise ObservationError("remote migration ledger contains an invalid migration name")
        if migration_id in seen_ids or name in seen_names:
            raise ObservationError("remote migration ledger contains duplicate id/name")
        seen_ids.add(migration_id)
        seen_names.add(name)
        previous_id = migration_id
        normalized.append({"id": migration_id, "name": name})
    return normalized


def command_observe(args: argparse.Namespace) -> int:
    if args.environment != "staging":
        raise ObservationError("D1 read adapter is staging-only")
    if ACCOUNT_RE.fullmatch(args.account_id) is None:
        raise ObservationError("account id is invalid")
    if DATABASE_RE.fullmatch(args.database_id) is None:
        raise ObservationError("database id is invalid")
    if not args.database_name or args.database_name == "part-crm-catalog-production":
        raise ObservationError("Production or empty D1 target is forbidden")

    token = os.environ.get(TOKEN_ENV, "")
    client = CloudflareReadClient(token)
    output = Path(args.output_dir)
    output.mkdir(parents=True, exist_ok=True)
    root = Path(args.root).resolve()
    local_migrations = _typed_local_migrations(Path(args.repository_json), root)

    status, database_envelope = client.request(
        "GET",
        f"/accounts/{args.account_id}/d1/database/{args.database_id}",
    )
    if status != 200:
        raise ObservationError(f"database identity returned HTTP {status}")
    database = _require_success_envelope(database_envelope, "database identity")
    if not isinstance(database, dict):
        raise ObservationError("database identity result is not one object")
    if database.get("uuid") != args.database_id or database.get("name") != args.database_name:
        raise ObservationError("database identity does not preserve exact target name+UUID")
    if database.get("version") != "production":
        raise ObservationError("D1 target is not on the production storage backend required for Time Travel")
    provider_identity = {
        key: database.get(key)
        for key in ("uuid", "name", "version", "created_at", "jurisdiction", "read_replication")
        if key in database
    }
    _write_json(output / "provider-identity.json", provider_identity)

    table_query, table_envelope = _query(client, args.account_id, args.database_id, LEDGER_TABLE_SQL, "ledger-table")
    table_rows = table_query["results"]
    if any(not isinstance(row, dict) or row.get("name") != "d1_migrations" for row in table_rows) or len(table_rows) > 1:
        raise ObservationError("ledger-table query returned an unexpected row shape")
    ledger_exists = len(table_rows) == 1
    _write_json(output / "ledger-table-api.json", table_envelope)

    if ledger_exists:
        ledger_query, ledger_envelope = _query(client, args.account_id, args.database_id, LEDGER_SQL, "ledger")
        migration_rows = _migration_rows(ledger_query)
    else:
        ledger_query = {"results": [], "success": True, "meta": {"changed_db": False, "changes": 0, "rows_written": 0}}
        ledger_envelope = None
        migration_rows = []
    wrangler_compatible_ledger = [{"results": migration_rows, "success": True, "meta": ledger_query.get("meta", {})}]
    _write_json(output / "ledger.json", wrangler_compatible_ledger)
    _write_json(output / "ledger-names.json", [row["name"] for row in migration_rows])
    if ledger_envelope is not None:
        _write_json(output / "ledger-api.json", ledger_envelope)

    remote_names = [row["name"] for row in migration_rows]
    pending = [name for name in local_migrations if name not in remote_names]
    _write_json(output / "provider-pending.json", pending)

    foreign_query, _ = _query(client, args.account_id, args.database_id, FOREIGN_KEY_SQL, "foreign-key-check")
    quick_query, _ = _query(client, args.account_id, args.database_id, QUICK_CHECK_SQL, "quick-check")
    _write_json(output / "foreign-key-check.stdout", [foreign_query])
    (output / "foreign-key-check.exit-code").write_text("0\n", encoding="utf-8")
    _write_json(output / "quick-check.stdout", [quick_query])
    (output / "quick-check.exit-code").write_text("0\n", encoding="utf-8")

    timestamp_query = _timestamp_query(args.time_travel_unix_seconds)
    status, bookmark_envelope = client.request(
        "GET",
        f"/accounts/{args.account_id}/d1/database/{args.database_id}/time_travel/bookmark{timestamp_query}",
    )
    if status != 200:
        raise ObservationError(f"time-travel bookmark returned HTTP {status}")
    bookmark_result = _require_success_envelope(bookmark_envelope, "time-travel bookmark")
    if not isinstance(bookmark_result, dict) or not isinstance(bookmark_result.get("bookmark"), str) or not bookmark_result["bookmark"]:
        raise ObservationError("time-travel bookmark response is missing one non-empty bookmark")
    _write_json(output / "time-travel.json", {"bookmark": bookmark_result["bookmark"]})

    read_contract = {
        "schema_version": 1,
        "kind": "D1_CLOUDFLARE_READ_CONTRACT_V1",
        "environment": "staging",
        "target": {"database_name": args.database_name, "database_id": args.database_id},
        "database": provider_identity,
        "ledger_table_present": ledger_exists,
        "remote_migrations": remote_names,
        "provider_pending_migrations": pending,
        "unknown_remote_migrations": [name for name in remote_names if name not in local_migrations],
        "query_mutation_proof": {
            "ledger_table_changed_db": table_query["meta"]["changed_db"],
            "ledger_changed_db": ledger_query["meta"]["changed_db"],
            "foreign_key_changed_db": foreign_query["meta"]["changed_db"],
            "quick_check_changed_db": quick_query["meta"]["changed_db"],
        },
        "time_travel_bookmark_present": True,
        "provider_mutation": False,
        "d1_mutation": False,
        "production_mutation": False,
    }
    _write_json(output / "read-contract.json", read_contract)
    return 0


def command_self_test() -> int:
    good = {
        "success": True,
        "errors": [],
        "result": [{
            "success": True,
            "results": [{"name": "d1_migrations"}],
            "meta": {"changed_db": False, "changes": 0, "rows_written": 0},
        }],
    }
    assert _require_read_query(good, "self-test")["results"] == [{"name": "d1_migrations"}]
    for mutated in (
        {"changed_db": True, "changes": 0, "rows_written": 0},
        {"changed_db": False, "changes": 1, "rows_written": 0},
        {"changed_db": False, "changes": 0, "rows_written": 1},
    ):
        value = json.loads(json.dumps(good))
        value["result"][0]["meta"] = mutated
        try:
            _require_read_query(value, "self-test")
        except ObservationError:
            pass
        else:
            raise AssertionError("mutation-like query metadata must fail closed")
    bad = json.loads(json.dumps(good))
    bad["success"] = False
    try:
        _require_read_query(bad, "self-test")
    except ObservationError:
        pass
    else:
        raise AssertionError("provider success=false must fail closed")
    assert _timestamp_query("1789500000").startswith("?timestamp=2026-09-")
    local = ["0001_a.sql", "0002_b.sql", "0003_c.sql"]
    remote = ["0001_a.sql", "0003_c.sql"]
    assert [name for name in local if name not in remote] == ["0002_b.sql"]
    print("d1-cloudflare-read self-test: PASS")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("self-test")
    observe = subparsers.add_parser("observe")
    observe.add_argument("--root", default=".")
    observe.add_argument("--environment", required=True)
    observe.add_argument("--account-id", required=True)
    observe.add_argument("--database-name", required=True)
    observe.add_argument("--database-id", required=True)
    observe.add_argument("--repository-json", required=True)
    observe.add_argument("--output-dir", required=True)
    observe.add_argument("--time-travel-unix-seconds", default="")
    return parser


def main() -> int:
    args = build_parser().parse_args()
    try:
        if args.command == "self-test":
            return command_self_test()
        if args.command == "observe":
            return command_observe(args)
        raise ObservationError(f"unsupported command: {args.command}")
    except ObservationError as exc:
        print(f"D1_CLOUDFLARE_READ_BLOCKED: {exc}", file=sys.stderr)
        return 3


if __name__ == "__main__":
    raise SystemExit(main())
