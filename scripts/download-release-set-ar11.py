#!/usr/bin/env python3
"""Download one immutable AR11 Release Set using the installed GitHub CLI.

Asset identity and all SHA-256, CMS, and Release Set checks remain owned by
release-set-assets-ar11.sh and opsctl after this download succeeds.
"""
from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from types import SimpleNamespace
from typing import Callable

MAX_ATTEMPTS = 4
BACKOFF_SECONDS = (1, 2, 4)
RELEASE_SET_PATTERN = re.compile(r"release-set-v[23]-sha256-[0-9a-f]{64}\Z")
REPOSITORY_PATTERN = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+\Z")


class DownloadError(RuntimeError):
    pass


def download_release_set(
    repository: str,
    release_set_id: str,
    destination: Path,
    runner: Callable = subprocess.run,
    sleep_fn: Callable[[float], None] = time.sleep,
) -> None:
    if not REPOSITORY_PATTERN.fullmatch(repository):
        raise DownloadError("repository_invalid")
    if not RELEASE_SET_PATTERN.fullmatch(release_set_id):
        raise DownloadError("release_set_id_invalid")

    destination = Path(destination).absolute()
    if destination.is_symlink():
        raise DownloadError("destination_symlink")
    if destination.exists() and (
        not destination.is_dir() or any(destination.iterdir())
    ):
        raise DownloadError("destination_not_empty")
    destination.parent.mkdir(parents=True, exist_ok=True)

    # Download into a sibling temporary directory. A failed attempt can never
    # leave partial assets in the destination consumed by the verifier.
    with tempfile.TemporaryDirectory(
        prefix=".ar11-release-set-download-", dir=destination.parent
    ) as temporary:
        staging = Path(temporary) / "assets"
        for attempt in range(1, MAX_ATTEMPTS + 1):
            shutil.rmtree(staging, ignore_errors=True)
            staging.mkdir()
            try:
                result = runner(
                    [
                        "gh",
                        "release",
                        "download",
                        release_set_id,
                        "--repo",
                        repository,
                        "--dir",
                        str(staging),
                    ],
                    text=True,
                    capture_output=True,
                    check=False,
                )
            except FileNotFoundError:
                raise DownloadError("github_cli_unavailable") from None
            except OSError as error:
                raise DownloadError(
                    f"github_cli_execution_failed:{type(error).__name__}"
                ) from None

            if result.returncode == 0:
                if not any(staging.iterdir()):
                    raise DownloadError("release_assets_missing")
                if destination.exists():
                    destination.rmdir()
                os.replace(staging, destination)
                return

            if attempt == MAX_ATTEMPTS:
                raise DownloadError(
                    f"release_download_exhausted:exit={result.returncode}"
                )
            delay = BACKOFF_SECONDS[attempt - 1]
            print(
                f"release_set_download retry attempt={attempt + 1}/{MAX_ATTEMPTS}",
                file=sys.stderr,
                flush=True,
            )
            sleep_fn(delay)

    raise DownloadError("release_download_incomplete")


def self_test() -> None:
    release_set_id = "release-set-v3-sha256-" + ("a" * 64)
    calls: list[list[str]] = []
    sleeps: list[float] = []

    def transient_then_success(command, **_kwargs):
        calls.append(command)
        staging = Path(command[command.index("--dir") + 1])
        if len(calls) == 1:
            (staging / "partial.bin").write_bytes(b"partial")
            return SimpleNamespace(returncode=1, stderr="temporary transport error")
        (staging / "release-set.json").write_text("{}", encoding="utf-8")
        return SimpleNamespace(returncode=0, stderr="")

    with tempfile.TemporaryDirectory() as directory:
        destination = Path(directory) / "assets"
        download_release_set(
            "iamaman11/part-crm-emai-profile",
            release_set_id,
            destination,
            runner=transient_then_success,
            sleep_fn=sleeps.append,
        )
        assert len(calls) == 2
        assert calls[0][:4] == ["gh", "release", "download", release_set_id]
        assert calls[0][4:6] == ["--repo", "iamaman11/part-crm-emai-profile"]
        assert sleeps == [1]
        assert sorted(path.name for path in destination.iterdir()) == ["release-set.json"]

    failed_calls = 0

    def always_fails(_command, **_kwargs):
        nonlocal failed_calls
        failed_calls += 1
        return SimpleNamespace(returncode=1, stderr="temporary transport error")

    with tempfile.TemporaryDirectory() as directory:
        destination = Path(directory) / "assets"
        try:
            download_release_set(
                "iamaman11/part-crm-emai-profile",
                release_set_id,
                destination,
                runner=always_fails,
                sleep_fn=lambda _delay: None,
            )
        except DownloadError as error:
            assert "release_download_exhausted" in str(error)
        else:
            raise AssertionError("exhausted GitHub CLI download unexpectedly succeeded")
        assert failed_calls == MAX_ATTEMPTS
        assert not destination.exists()
        assert not list(Path(directory).glob(".ar11-release-set-download-*"))

    workflow = (
        Path(__file__).resolve().parents[1]
        / ".github"
        / "workflows"
        / "release-set-promotion.yml"
    ).read_text(encoding="utf-8")
    assert workflow.count("scripts/download-release-set-ar11.py") == 6
    assert "gh release download" not in workflow

    print("AR11 Release Set GitHub CLI downloader self-test: PASS")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository")
    parser.add_argument("--release-set-id")
    parser.add_argument("--destination")
    parser.add_argument("--self-test", action="store_true")
    arguments = parser.parse_args()
    if arguments.self_test:
        self_test()
        return 0
    if not arguments.repository or not arguments.release_set_id or not arguments.destination:
        parser.error("--repository, --release-set-id and --destination are required")
    try:
        download_release_set(
            arguments.repository,
            arguments.release_set_id,
            Path(arguments.destination),
        )
    except DownloadError as error:
        print(f"release_set_download failed: {error}", file=sys.stderr)
        return 1
    print(f"Release Set assets downloaded: {arguments.release_set_id}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
