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
RUNTIME_COMPONENT_PATTERN = re.compile(r"runtime-bundle-v3-sha256-[0-9a-f]{64}\Z")
REPOSITORY_PATTERN = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+\Z")
RUNTIME_MANIFEST_ASSET = "runtime-manifest.json"
MAX_RUNTIME_MANIFEST_BYTES = 2_000_000


class DownloadError(RuntimeError):
    pass


def download_release_set(
    repository: str,
    release_set_id: str,
    destination: Path,
    runner: Callable = subprocess.run,
    sleep_fn: Callable[[float], None] = time.sleep,
    runtime_manifest_only: bool = False,
) -> None:
    if not REPOSITORY_PATTERN.fullmatch(repository):
        raise DownloadError("repository_invalid")
    release_pattern = RUNTIME_COMPONENT_PATTERN if runtime_manifest_only else RELEASE_SET_PATTERN
    if not release_pattern.fullmatch(release_set_id):
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
                command = [
                    "gh", "release", "download", release_set_id,
                    "--repo", repository, "--dir", str(staging),
                ]
                if runtime_manifest_only:
                    command.extend(["--pattern", RUNTIME_MANIFEST_ASSET])
                result = runner(
                    command,
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
                if runtime_manifest_only:
                    members = list(staging.iterdir())
                    if (len(members) != 1 or members[0].name != RUNTIME_MANIFEST_ASSET
                            or members[0].is_symlink() or not members[0].is_file()
                            or not (1 <= members[0].stat().st_size <= MAX_RUNTIME_MANIFEST_BYTES)):
                        raise DownloadError("runtime_manifest_asset_invalid")
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

    runtime_id = "runtime-bundle-v3-sha256-" + ("c" * 64)
    runtime_calls: list[list[str]] = []
    def runtime_manifest_success(command, **_kwargs):
        runtime_calls.append(command)
        staging = Path(command[command.index("--dir") + 1])
        (staging / RUNTIME_MANIFEST_ASSET).write_bytes(b"{\"kind\":\"test\"}")
        return SimpleNamespace(returncode=0, stderr="")

    with tempfile.TemporaryDirectory() as directory:
        destination = Path(directory) / "runtime-sidecar"
        download_release_set(
            "iamaman11/part-crm-emai-profile", runtime_id, destination,
            runner=runtime_manifest_success, runtime_manifest_only=True,
        )
        assert runtime_calls[0][-2:] == ["--pattern", RUNTIME_MANIFEST_ASSET]
        assert [p.name for p in destination.iterdir()] == [RUNTIME_MANIFEST_ASSET]

    for mode in ("missing", "extra", "oversized", "symlink"):
        def invalid_runtime_asset(command, **_kwargs):
            staging = Path(command[command.index("--dir") + 1])
            if mode != "missing":
                asset = staging / RUNTIME_MANIFEST_ASSET
                if mode == "symlink":
                    asset.symlink_to(Path(directory) / "external-file")
                else:
                    asset.write_bytes(b"x" * (MAX_RUNTIME_MANIFEST_BYTES + 1 if mode == "oversized" else 1))
                if mode == "extra":
                    (staging / "unmatched.json").write_bytes(b"extra")
            return SimpleNamespace(returncode=0, stderr="")

        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "invalid-runtime-sidecar"
            try:
                download_release_set(
                    "iamaman11/part-crm-emai-profile", runtime_id, destination,
                    runner=invalid_runtime_asset, runtime_manifest_only=True,
                )
            except DownloadError:
                pass
            else:
                raise AssertionError(f"{mode} runtime sidecar unexpectedly accepted")
            assert not destination.exists()

    for bad_id in (release_set_id, "runtime-bundle-v3-sha256-" + "g" * 64):
        try:
            download_release_set(
                "iamaman11/part-crm-emai-profile", bad_id, Path("unused"),
                runner=runtime_manifest_success, runtime_manifest_only=True,
            )
        except DownloadError:
            pass
        else:
            raise AssertionError("invalid runtime component identity unexpectedly accepted")

    workflow = (
        Path(__file__).resolve().parents[1]
        / ".github"
        / "workflows"
        / "release-set-promotion.yml"
    ).read_text(encoding="utf-8")
    # A single workflow invocation has one materialization owner. Automatic
    # preflight downloads the candidate and (when required) the distinct
    # known-good predecessor; manual promotion downloads the candidate once.
    ready = workflow.split("\n  preflight-ready:\n", 1)[1].split("\n  promote:\n", 1)[0]
    promote = workflow.split("\n  promote:\n", 1)[1].split("\n  manual-outcome:\n", 1)[0]
    downloader = "scripts/download-release-set-ar11.py"
    assert ready.count("--runtime-manifest-release-id") == 2
    assert promote.count("--runtime-manifest-release-id") == 1
    assert ready.count(downloader) == 4
    assert promote.count(downloader) == 2
    assert workflow.count(downloader) == 6
    assert "gh release download" not in workflow

    print("AR11 Release Set GitHub CLI downloader self-test: PASS")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository")
    release_input = parser.add_mutually_exclusive_group()
    release_input.add_argument("--release-set-id")
    release_input.add_argument("--runtime-manifest-release-id")
    parser.add_argument("--destination")
    parser.add_argument("--self-test", action="store_true")
    arguments = parser.parse_args()
    if arguments.self_test:
        self_test()
        return 0
    selected_release_id = arguments.release_set_id or arguments.runtime_manifest_release_id
    if not arguments.repository or not selected_release_id or not arguments.destination:
        parser.error("--repository, one release identity and --destination are required")
    try:
        download_release_set(
            arguments.repository,
            selected_release_id,
            Path(arguments.destination),
            runtime_manifest_only=arguments.runtime_manifest_release_id is not None,
        )
    except DownloadError as error:
        print(f"release_set_download failed: {error}", file=sys.stderr)
        return 1
    print(f"Immutable release assets downloaded: {selected_release_id}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
