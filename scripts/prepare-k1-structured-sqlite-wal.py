#!/usr/bin/env python3
"""Prepare only the independent K1 SQLite stress pair in a disposable profile.

Test fixture generator, NOT a browser/Sync/credential/provider owner. The caller
must separately establish genuine Firefox CleanClose and authorize the one-shot
ignored shipping staging test. Persistent effects are bounded to one new
atomic user_data/k1-structured/ directory containing a SQLite DB+WAL pair.
Builder/stage scratch stays outside the profile snapshot; no network effects.

Example (only after an independently verified disposable Restore/CleanClose):
    python scripts/prepare-k1-structured-sqlite-wal.py \
      --root ABSOLUTE_TEMP_RESTORE_ROOT --profile EXACT_PROFILE_ID \
      --workspace ABSOLUTE_CLOSED_PROFILE_WORKSPACE

Expected test-only output: user_data/k1-structured/{k1-structured.sqlite,
k1-structured.sqlite-wal} published with one directory rename.
The genuine Firefox cookies.sqlite[-wal] is never opened or modified here.
"""

from __future__ import annotations

import argparse
import hashlib
import os
from pathlib import Path
import shutil
import sqlite3
import stat
import tempfile


ROW_COUNT = 1152
PAYLOAD_SIZE = 65536
MIN_WAL_BYTES = 65 * 1024 * 1024
DB_NAME = "k1-structured.sqlite"
WAL_NAME = DB_NAME + "-wal"
LEAF = "k1-structured"
SEED = b"ProfileBridge-K1-independent-structured-SQLite-WAL-v1"


def _regular_components(root: Path, descendant: Path, *, directory: bool) -> None:
    """Refuse an escaping symlink/junction/reparse component before touching it."""
    if not descendant.is_absolute():
        raise ValueError("K1 requires an absolute disposable target")
    try:
        parts = descendant.relative_to(root).parts
    except ValueError as exc:
        raise ValueError("K1 target escapes selected scratch root") from exc
    if not parts or any(part in ("..", ".") for part in parts):
        raise ValueError("K1 requires an existing child of scratch root")
    current = root
    for part in parts:
        current /= part
        info = current.lstat()
        if stat.S_ISLNK(info.st_mode) or (
            info.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT
            if os.name == "nt"
            else False
        ):
            raise ValueError("K1 refuses symlink/junction/reparse descendant")
    if not descendant.resolve().is_relative_to(root.resolve()):
        raise ValueError("K1 target canonical path escapes scratch root")
    info = descendant.lstat()
    if directory and not stat.S_ISDIR(info.st_mode):
        raise ValueError("K1 expected an existing directory")
    if not directory and not stat.S_ISREG(info.st_mode):
        raise ValueError("K1 expected an existing regular file")


def _admit(root: Path, profile: str, workspace: Path) -> Path:
    if not root.is_absolute() or not workspace.is_absolute():
        raise ValueError("K1 fixture requires absolute paths")
    temp = Path(tempfile.gettempdir()).resolve()
    if root.resolve() == temp or not root.resolve().is_relative_to(temp):
        raise ValueError("K1 fixture must be under but not equal to OS TEMP")
    _regular_components(temp, root, directory=True)
    _regular_components(root, root / ".physical-staging-verified-restore", directory=False)
    marker = root / ".physical-staging-verified-restore"
    if marker.read_bytes() != f"restored-test-profile={profile}\n".encode():
        raise ValueError("K1 restored disposable profile marker mismatch")
    _regular_components(root, workspace, directory=True)
    _regular_components(root, workspace / ".local-unsynced", directory=False)
    if (workspace / ".local-unsynced").read_bytes() != b"local-unsynced-v2":
        raise ValueError("K1 requires prior closed dirty scratch profile")
    user_data = workspace / "user_data"
    _regular_components(root, user_data, directory=True)
    if os.path.lexists(user_data / LEAF):
        raise ValueError("K1 refuses to overwrite a preexisting stress fixture directory")
    return user_data


def _sha(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(65536), b""):
            digest.update(block)
    return digest.hexdigest()


def _payload(row_id: int) -> bytes:
    return hashlib.shake_256(SEED + row_id.to_bytes(4, "big")).digest(PAYLOAD_SIZE)


def _assert_readback(db: Path, *, expected_rows: int) -> None:
    # Only a private validator clone is opened by SQLite. Never open the
    # candidate or a real browser DB; SQLite may create -shm/checkpoint on open.
    with sqlite3.connect(db) as conn:
        if conn.execute("PRAGMA integrity_check(1)").fetchone() != ("ok",):
            raise ValueError("K1 independent SQLite integrity_check failed")
        rows = conn.execute("SELECT id,payload FROM k1_wal_probe ORDER BY id")
        count = 0
        for row_id, blob in rows:
            if row_id != count + 1 or blob != _payload(row_id):
                raise ValueError("K1 WAL-only probe row/content mismatch")
            count += 1
        if count != expected_rows:
            raise ValueError("K1 WAL-only probe replay count mismatch")


def _validate_copied_pair(db: Path, wal: Path) -> tuple[str, str]:
    if wal.stat().st_size < MIN_WAL_BYTES:
        raise ValueError("K1 structured SQLite WAL is under 65 MiB")
    db_sha, wal_sha = _sha(db), _sha(wal)
    with tempfile.TemporaryDirectory(prefix="k1-sqlite-validation-") as scratch:
        isolated = Path(scratch)
        plain = isolated / "base"
        pair = isolated / "pair"
        plain.mkdir()
        pair.mkdir()
        shutil.copyfile(db, plain / DB_NAME)
        shutil.copyfile(db, pair / DB_NAME)
        shutil.copyfile(wal, pair / WAL_NAME)
        _assert_readback(plain / DB_NAME, expected_rows=0)
        _assert_readback(pair / DB_NAME, expected_rows=ROW_COUNT)
    if _sha(db) != db_sha or _sha(wal) != wal_sha:
        raise ValueError("K1 candidate changed during independent clone readback")
    return db_sha, wal_sha


def _prepare(user_data: Path, stage_parent: Path) -> tuple[int, str, str]:
    # Keep the builder connection alive/idle while copying both DB and WAL.
    # Closing the last connection may checkpoint and DELETE the builder WAL.
    with tempfile.TemporaryDirectory(prefix="k1-sqlite-builder-") as builder:
        db = Path(builder) / DB_NAME
        wal = Path(builder) / WAL_NAME
        conn = sqlite3.connect(db)
        try:
            conn.execute("CREATE TABLE k1_wal_probe(id INTEGER PRIMARY KEY,payload BLOB NOT NULL)")
            conn.commit()  # schema + EMPTY table exist in the base DB
            if conn.execute("PRAGMA journal_mode=WAL").fetchone() != ("wal",):
                raise ValueError("SQLite refused WAL mode")
            conn.execute("PRAGMA synchronous=FULL")
            conn.execute("PRAGMA wal_autocheckpoint=0")
            conn.execute("PRAGMA journal_size_limit=-1")
            conn.execute("PRAGMA cache_size=-2048")
            conn.execute("BEGIN IMMEDIATE")
            for row_id in range(1, ROW_COUNT + 1):
                conn.execute(
                    "INSERT INTO k1_wal_probe(id,payload) VALUES (?,?)",
                    (row_id, _payload(row_id)),
                )
            conn.commit()  # exactly one WAL-only committed transaction
            if not wal.is_file():
                raise ValueError("SQLite dropped WAL while builder connection was open")

            # Stage OUTSIDE the workspace inventory but on the same drive as
            # the selected TEMP root. A directory rename publishes both
            # files in one indivisible filesystem operation.
            with tempfile.TemporaryDirectory(
                prefix="k1-sqlite-stage-", dir=stage_parent
            ) as stage:
                candidate = Path(stage) / LEAF
                candidate.mkdir()
                staged_db, staged_wal = candidate / DB_NAME, candidate / WAL_NAME
                shutil.copyfile(db, staged_db)
                shutil.copyfile(wal, staged_wal)
                db_sha, wal_sha = _validate_copied_pair(staged_db, staged_wal)
                wal_size = staged_wal.stat().st_size
                target = user_data / LEAF
                if os.path.lexists(target):
                    raise ValueError("K1 refuses an existing destination directory")
                # On Windows this fails if the directory exists or if source
                # and target are on different volumes. No copy fallback:
                # copying into the live snapshot would expose a partial pair.
                os.rename(candidate, target)
                if _sha(target / DB_NAME) != db_sha or _sha(target / WAL_NAME) != wal_sha:
                    raise ValueError("K1 atomic destination failed byte verification")
                return wal_size, db_sha, wal_sha
        finally:
            conn.close()


def _self_test() -> None:
    """Provider-free positive and negative proof in a fresh disposable TEMP."""
    with tempfile.TemporaryDirectory(prefix="k1-sqlite-selftest-") as temp:
        root = Path(temp)
        workspace = root / "closed-profile"
        user_data = workspace / "user_data"
        user_data.mkdir(parents=True)
        (root / ".physical-staging-verified-restore").write_bytes(
            b"restored-test-profile=fixture_profile\n"
        )
        (workspace / ".local-unsynced").write_bytes(b"local-unsynced-v2")
        try:
            _admit(root, "wrong_profile", workspace)
        except ValueError:
            pass
        else:
            raise AssertionError("mismatched restore marker was admitted")
        if (user_data / LEAF).exists():
            raise AssertionError("negative preflight wrote destination")
        assert _admit(root, "fixture_profile", workspace) == user_data
        bytes_written, db_sha, wal_sha = _prepare(user_data, root.parent)
        finished = user_data / LEAF
        if bytes_written < MIN_WAL_BYTES:
            raise AssertionError("positive WAL was not >=65 MiB")
        if _validate_copied_pair(finished / DB_NAME, finished / WAL_NAME) != (
            db_sha,
            wal_sha,
        ):
            raise AssertionError("positive structured WAL digest/readback mismatch")
        try:
            _admit(root, "fixture_profile", workspace)
        except ValueError:
            pass
        else:
            raise AssertionError("existing stress pair could be overwritten")

        # A one-byte damaged WAL still often reports SQLite integrity_check
        # 'ok' while silently IGNORING its frames. WAL-only row replay must
        # reject it, without ever opening the existing genuine candidate.
        with tempfile.TemporaryDirectory(prefix="k1-sqlite-negative-") as bad_dir:
            bad = Path(bad_dir)
            shutil.copyfile(finished / DB_NAME, bad / DB_NAME)
            shutil.copyfile(finished / WAL_NAME, bad / WAL_NAME)
            with (bad / WAL_NAME).open("r+b") as stream:
                stream.seek(-1, os.SEEK_END)
                original = stream.read(1)
                stream.seek(-1, os.SEEK_END)
                stream.write(bytes((original[0] ^ 1,)))
            try:
                _validate_copied_pair(bad / DB_NAME, bad / WAL_NAME)
            except ValueError:
                pass
            else:
                raise AssertionError("corrupt WAL falsely passed clone replay")
    print("K1_STRUCTURED_SQLITE_PROVIDER_FREE_SELF_TEST_PASS")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--root", type=Path)
    parser.add_argument("--profile")
    parser.add_argument("--workspace", type=Path)
    args = parser.parse_args()
    if args.self_test:
        if any(value is not None for value in (args.root, args.profile, args.workspace)):
            parser.error("--self-test is isolated and cannot select a real profile")
        _self_test()
        return
    if args.root is None or args.profile is None or args.workspace is None:
        parser.error("--root, --profile and --workspace are all required")
    if os.name != "nt":
        parser.error("actual disposable profile preparation is Windows-only; use --self-test here")
    target = _admit(args.root, args.profile, args.workspace)
    wal_bytes, db_sha, wal_sha = _prepare(target, args.root.parent)
    print(f"K1_STRUCTURED_SQLITE_FIXTURE_READY;wal_bytes={wal_bytes};db_sha256={db_sha};wal_sha256={wal_sha}")
    print("Firefox native cookies and WAL NOT touched; no Sync/Restore/provider request executed.")


if __name__ == "__main__":
    main()
