#!/usr/bin/env python3
"""Download exact immutable AR11 Release Set assets with transient-only retries.

Cryptographic identity, SHA-256, CMS and Release Set verification remain owned by
release-set-assets-ar11.sh and opsctl after this downloader returns successfully.
"""
from __future__ import annotations

import argparse
import http.client
import json
import os
import re
import socket
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Callable

MAX_ATTEMPTS = 4
BACKOFF_SECONDS = (1, 2, 4)
CHUNK_SIZE = 1024 * 1024
RELEASE_SET_PATTERN = re.compile(r"release-set-v[23]-sha256-[0-9a-f]{64}\Z")
REPOSITORY_PATTERN = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+\Z")
NETWORK_ERRORS = (
    urllib.error.URLError,
    TimeoutError,
    ConnectionError,
    socket.timeout,
    http.client.HTTPException,
    OSError,
)


class PermanentDownloadError(RuntimeError):
    pass


class TransientDownloadError(RuntimeError):
    pass


def _open_request(
    url: str,
    accept: str,
    token: str,
    opener: Callable = urllib.request.urlopen,
):
    request = urllib.request.Request(url)
    request.add_header("Accept", accept)
    request.add_header("User-Agent", "ProfileBridge-AR11-ReleaseSet-Downloader")
    # Keep the credential out of cross-origin release-assets redirects.
    request.add_unredirected_header("Authorization", f"Bearer {token}")
    try:
        return opener(request, timeout=60)
    except urllib.error.HTTPError as error:
        status = error.code
        error.close()
        if 500 <= status <= 599:
            raise TransientDownloadError(f"http_{status}") from None
        raise PermanentDownloadError(f"http_{status}") from None
    except NETWORK_ERRORS as error:
        raise TransientDownloadError(type(error).__name__) from None


def _read_response(response) -> bytes:
    try:
        return response.read()
    except NETWORK_ERRORS as error:
        raise TransientDownloadError(type(error).__name__) from None


def _read_release_metadata(
    repository: str,
    release_set_id: str,
    token: str,
    opener: Callable,
) -> dict:
    url = (
        "https://api.github.com/repos/"
        + repository
        + "/releases/tags/"
        + urllib.parse.quote(release_set_id, safe="")
    )
    with _open_request(url, "application/vnd.github+json", token, opener) as response:
        raw = _read_response(response)
    try:
        value = json.loads(raw)
    except (UnicodeDecodeError, json.JSONDecodeError):
        raise PermanentDownloadError("release_metadata_invalid_json") from None
    if not isinstance(value, dict):
        raise PermanentDownloadError("release_metadata_invalid_shape")
    if value.get("tag_name") != release_set_id or value.get("draft") is not False:
        raise PermanentDownloadError("release_identity_or_publication_mismatch")
    assets = value.get("assets")
    if not isinstance(assets, list) or not assets:
        raise PermanentDownloadError("release_assets_missing")
    seen_names: set[str] = set()
    seen_ids: set[int] = set()
    for asset in assets:
        if not isinstance(asset, dict):
            raise PermanentDownloadError("release_asset_invalid_shape")
        asset_id = asset.get("id")
        name = asset.get("name")
        size = asset.get("size")
        if (
            isinstance(asset_id, bool)
            or not isinstance(asset_id, int)
            or asset_id <= 0
            or isinstance(size, bool)
            or not isinstance(size, int)
            or size < 0
            or not isinstance(name, str)
            or not name
            or name in {".", ".."}
            or "/" in name
            or "\\" in name
            or "\x00" in name
            or asset.get("state") not in (None, "uploaded")
            or asset_id in seen_ids
            or name in seen_names
        ):
            raise PermanentDownloadError("release_asset_identity_invalid")
        seen_ids.add(asset_id)
        seen_names.add(name)
    return value


def _download_asset_once(
    repository: str,
    asset: dict,
    token: str,
    part_path: Path,
    opener: Callable,
) -> None:
    asset_id = asset["id"]
    expected_size = asset["size"]
    url = f"https://api.github.com/repos/{repository}/releases/assets/{asset_id}"
    total = 0
    with _open_request(url, "application/octet-stream", token, opener) as response:
        content_type = response.headers.get("Content-Type", "").lower()
        if content_type.startswith("application/json"):
            raise PermanentDownloadError("asset_endpoint_returned_json")
        with part_path.open("wb") as output:
            while True:
                try:
                    chunk = response.read(CHUNK_SIZE)
                except NETWORK_ERRORS as error:
                    raise TransientDownloadError(type(error).__name__) from None
                if not chunk:
                    break
                output.write(chunk)
                total += len(chunk)
    if total < expected_size:
        raise TransientDownloadError("asset_stream_incomplete")
    if total > expected_size:
        raise PermanentDownloadError("asset_size_mismatch")


def _download_asset(
    repository: str,
    asset: dict,
    token: str,
    staging: Path,
    opener: Callable,
    sleep_fn: Callable[[float], None],
    attempts: int,
) -> None:
    target = staging / asset["name"]
    part_path = staging / (asset["name"] + ".part")
    for attempt in range(1, attempts + 1):
        part_path.unlink(missing_ok=True)
        try:
            _download_asset_once(repository, asset, token, part_path, opener)
            os.replace(part_path, target)
            return
        except TransientDownloadError as error:
            part_path.unlink(missing_ok=True)
            if attempt == attempts:
                raise PermanentDownloadError(
                    f"asset_download_exhausted:{asset['name']}:{error}"
                ) from None
            delay = BACKOFF_SECONDS[min(attempt - 1, len(BACKOFF_SECONDS) - 1)]
            print(
                f"release_set_download retry phase=asset attempt={attempt + 1}/{attempts} "
                f"asset={asset['name']} reason={error}",
                file=sys.stderr,
                flush=True,
            )
            sleep_fn(delay)
        except Exception:
            part_path.unlink(missing_ok=True)
            raise


def download_release_set(
    repository: str,
    release_set_id: str,
    destination: Path,
    token: str,
    opener: Callable = urllib.request.urlopen,
    sleep_fn: Callable[[float], None] = time.sleep,
    attempts: int = MAX_ATTEMPTS,
) -> None:
    if not REPOSITORY_PATTERN.fullmatch(repository):
        raise PermanentDownloadError("repository_invalid")
    if not RELEASE_SET_PATTERN.fullmatch(release_set_id):
        raise PermanentDownloadError("release_set_id_invalid")
    if not token:
        raise PermanentDownloadError("github_token_missing")
    if attempts < 1:
        raise PermanentDownloadError("attempt_count_invalid")

    destination = Path(destination).absolute()
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists() and (
        not destination.is_dir() or any(destination.iterdir())
    ):
        raise PermanentDownloadError("destination_not_empty")

    metadata = None
    for attempt in range(1, attempts + 1):
        try:
            metadata = _read_release_metadata(
                repository, release_set_id, token, opener
            )
            break
        except TransientDownloadError as error:
            if attempt == attempts:
                raise PermanentDownloadError(
                    f"release_metadata_download_exhausted:{error}"
                ) from None
            delay = BACKOFF_SECONDS[min(attempt - 1, len(BACKOFF_SECONDS) - 1)]
            print(
                f"release_set_download retry phase=metadata attempt={attempt + 1}/{attempts} "
                f"reason={error}",
                file=sys.stderr,
                flush=True,
            )
            sleep_fn(delay)
    assert metadata is not None

    with tempfile.TemporaryDirectory(
        prefix=".ar11-release-set-download-", dir=destination.parent
    ) as temporary:
        staging = Path(temporary) / "assets"
        staging.mkdir()
        for asset in metadata["assets"]:
            _download_asset(
                repository, asset, token, staging, opener, sleep_fn, attempts
            )
        if destination.exists():
            destination.rmdir()
        os.replace(staging, destination)


def _self_test() -> None:
    from io import BytesIO

    release_set_id = "release-set-v3-sha256-" + ("a" * 64)
    payload = b"exact immutable test asset"

    class FakeResponse:
        def __init__(self, body: bytes, content_type: str):
            self.stream = BytesIO(body)
            self.headers = {"Content-Type": content_type}

        def read(self, size: int = -1) -> bytes:
            return self.stream.read(size)

        def __enter__(self):
            return self

        def __exit__(self, *_args):
            self.stream.close()

    class PartialThenErrorResponse:
        def __init__(self, body: bytes):
            self.body = body
            self.sent_partial = False
            self.headers = {"Content-Type": "application/octet-stream"}

        def read(self, _size: int = -1) -> bytes:
            if not self.sent_partial:
                self.sent_partial = True
                return self.body[:7]
            raise urllib.error.URLError("connection reset")

        def __enter__(self):
            return self

        def __exit__(self, *_args):
            return None

    metadata = {
        "tag_name": release_set_id,
        "draft": False,
        "assets": [
            {
                "id": 77,
                "name": "release-set.json",
                "size": len(payload),
                "state": "uploaded",
            }
        ],
    }
    calls = {"metadata": 0, "asset": 0}
    sleeps: list[float] = []

    def transient_then_success(request, timeout=60):
        url = request.full_url
        if "/releases/tags/" in url:
            calls["metadata"] += 1
            if calls["metadata"] == 1:
                raise urllib.error.HTTPError(url, 503, "temporary", {}, None)
            return FakeResponse(json.dumps(metadata).encode(), "application/json")
        if url.endswith("/releases/assets/77"):
            calls["asset"] += 1
            if calls["asset"] == 1:
                return PartialThenErrorResponse(payload)
            partial_files = list(
                Path(directory).glob(".ar11-release-set-download-*/assets/*.part")
            )
            assert not partial_files, "partial asset was not removed before retry"
            return FakeResponse(payload, "application/octet-stream")
        raise AssertionError(f"unexpected URL: {url}")

    with tempfile.TemporaryDirectory() as directory:
        destination = Path(directory) / "assets"
        download_release_set(
            "iamaman11/part-crm-emai-profile",
            release_set_id,
            destination,
            "test-token",
            opener=transient_then_success,
            sleep_fn=sleeps.append,
            attempts=3,
        )
        assert (destination / "release-set.json").read_bytes() == payload
        assert calls == {"metadata": 2, "asset": 2}
        assert sleeps == [1, 1]

    not_found_calls = 0

    def permanent_not_found(request, timeout=60):
        nonlocal not_found_calls
        not_found_calls += 1
        raise urllib.error.HTTPError(request.full_url, 404, "not found", {}, None)

    with tempfile.TemporaryDirectory() as directory:
        destination = Path(directory) / "assets"
        try:
            download_release_set(
                "iamaman11/part-crm-emai-profile",
                release_set_id,
                destination,
                "test-token",
                opener=permanent_not_found,
                sleep_fn=lambda _delay: None,
                attempts=4,
            )
        except PermanentDownloadError as error:
            assert "http_404" in str(error)
        else:
            raise AssertionError("HTTP 404 unexpectedly succeeded")
        assert not_found_calls == 1
        assert not destination.exists()

    metadata_calls = 0
    failed_asset_calls = 0

    def exhausted_asset(request, timeout=60):
        nonlocal metadata_calls, failed_asset_calls
        url = request.full_url
        if "/releases/tags/" in url:
            metadata_calls += 1
            return FakeResponse(json.dumps(metadata).encode(), "application/json")
        if url.endswith("/releases/assets/77"):
            failed_asset_calls += 1
            raise urllib.error.HTTPError(url, 503, "temporary", {}, None)
        raise AssertionError(f"unexpected URL: {url}")

    with tempfile.TemporaryDirectory() as directory:
        destination = Path(directory) / "assets"
        try:
            download_release_set(
                "iamaman11/part-crm-emai-profile",
                release_set_id,
                destination,
                "test-token",
                opener=exhausted_asset,
                sleep_fn=lambda _delay: None,
                attempts=3,
            )
        except PermanentDownloadError as error:
            assert "asset_download_exhausted" in str(error)
        else:
            raise AssertionError("exhausted transient download unexpectedly succeeded")
        assert metadata_calls == 1
        assert failed_asset_calls == 3
        assert not destination.exists()
        assert not list(Path(directory).glob(".ar11-release-set-download-*"))

    workflow_path = (
        Path(__file__).resolve().parents[1]
        / ".github"
        / "workflows"
        / "release-set-promotion.yml"
    )
    workflow_source = workflow_path.read_text(encoding="utf-8")
    assert "gh release download" not in workflow_source
    assert workflow_source.count("scripts/download-release-set-ar11.py") == 6

    print("AR11 immutable Release Set downloader self-test: PASS")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository")
    parser.add_argument("--release-set-id")
    parser.add_argument("--destination")
    parser.add_argument("--self-test", action="store_true")
    arguments = parser.parse_args()
    if arguments.self_test:
        _self_test()
        return 0
    token = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")
    if not arguments.repository or not arguments.release_set_id or not arguments.destination:
        parser.error("--repository, --release-set-id and --destination are required")
    try:
        download_release_set(
            arguments.repository,
            arguments.release_set_id,
            Path(arguments.destination),
            token or "",
        )
    except PermanentDownloadError as error:
        print(f"release_set_download failed: {error}", file=sys.stderr)
        return 1
    print(f"Release Set assets downloaded: {arguments.release_set_id}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
