#!/usr/bin/env python3
"""Publish GitHub Release assets with bounded, resumable, fail-closed semantics.

This helper is the single publication primitive for content-addressed component
and aggregate releases. New publications are staged as drafts, partial draft
uploads may be resumed only when already-uploaded bytes are exact, and a
published release is accepted only when its complete asset inventory is exact.
"""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import http.client
import io
import json
import os
import re
import ssl
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable
from urllib.parse import urlencode, urlsplit

DIGEST_RE = re.compile(r"^sha256:([0-9a-f]{64})$")
API_TIMEOUT_SECONDS = 120
UPLOAD_TIMEOUT_SECONDS = 5_400
UPLOAD_SOCKET_TIMEOUT_SECONDS = 120
UPLOAD_CHUNK_BYTES = 8 * 1024 * 1024
UPLOAD_PROGRESS_BYTES = 128 * 1024 * 1024


class PublicationError(RuntimeError):
    """Fail-closed release publication error."""


def fail(message: str) -> None:
    raise PublicationError(message)


@dataclass(frozen=True)
class LocalAsset:
    name: str
    path: Path
    size: int
    digest: str


@dataclass(frozen=True)
class RemoteAsset:
    asset_id: int
    name: str
    state: str
    size: int
    digest: str | None


@dataclass(frozen=True)
class ReleaseAnalysis:
    state: str
    release_id: int | None
    draft: bool | None
    assets: tuple[RemoteAsset, ...]


def sha256_file(path: Path) -> str:
    if path.is_symlink() or not path.is_file():
        fail(f"release asset must be a regular file: {path}")
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def local_assets(paths: Iterable[Path]) -> dict[str, LocalAsset]:
    result: dict[str, LocalAsset] = {}
    for path in paths:
        name = path.name
        if not name or name in result:
            fail(f"release asset basename must be unique: {name!r}")
        if path.is_symlink() or not path.is_file():
            fail(f"release asset must be a regular file: {path}")
        size = path.stat().st_size
        if size <= 0:
            fail(f"release asset must be non-empty: {path}")
        result[name] = LocalAsset(
            name=name,
            path=path,
            size=size,
            digest=f"sha256:{sha256_file(path)}",
        )
    if not result:
        fail("at least one release asset is required")
    return result


def _remote_assets(release: dict[str, Any]) -> tuple[RemoteAsset, ...]:
    raw_assets = release.get("assets")
    if not isinstance(raw_assets, list):
        fail("GitHub release assets inventory is missing")
    result: list[RemoteAsset] = []
    seen: set[str] = set()
    for raw in raw_assets:
        if not isinstance(raw, dict):
            fail("GitHub release asset entry is invalid")
        asset_id = raw.get("id")
        name = raw.get("name")
        state = raw.get("state")
        size = raw.get("size")
        digest = raw.get("digest")
        if not isinstance(asset_id, int) or asset_id <= 0:
            fail("GitHub release asset id is invalid")
        if not isinstance(name, str) or not name:
            fail("GitHub release asset name is invalid")
        if name in seen:
            fail(f"GitHub release contains duplicate asset name: {name}")
        seen.add(name)
        if not isinstance(state, str) or not state:
            fail(f"GitHub release asset state is invalid: {name}")
        if not isinstance(size, int) or size < 0:
            fail(f"GitHub release asset size is invalid: {name}")
        if digest is not None and (
            not isinstance(digest, str) or DIGEST_RE.fullmatch(digest) is None
        ):
            fail(f"GitHub release asset digest is invalid: {name}")
        result.append(
            RemoteAsset(
                asset_id=asset_id,
                name=name,
                state=state,
                size=size,
                digest=digest,
            )
        )
    return tuple(result)


def analyze_release(
    release: dict[str, Any] | None,
    release_tag: str,
    expected_names: set[str],
) -> ReleaseAnalysis:
    if not expected_names or any(not name for name in expected_names):
        fail("expected release asset names are invalid")
    if release is None:
        return ReleaseAnalysis("absent", None, None, ())

    if release.get("tag_name") != release_tag:
        fail("GitHub release tag identity mismatch")
    release_id = release.get("id")
    draft = release.get("draft")
    prerelease = release.get("prerelease")
    if not isinstance(release_id, int) or release_id <= 0:
        fail("GitHub release id is invalid")
    if not isinstance(draft, bool) or not isinstance(prerelease, bool):
        fail("GitHub release publication flags are invalid")
    if prerelease:
        fail("content-addressed release must not be a prerelease")

    assets = _remote_assets(release)
    names = {asset.name for asset in assets}
    extras = sorted(names.difference(expected_names))
    if extras:
        fail(f"GitHub release contains unexpected assets: {extras}")

    missing = sorted(expected_names.difference(names))
    incomplete = sorted(
        asset.name
        for asset in assets
        if asset.state != "uploaded"
        or asset.size <= 0
        or asset.digest is None
    )

    if draft:
        return ReleaseAnalysis("resume", release_id, True, assets)

    if missing or incomplete:
        fail(
            "published content-addressed release is incomplete: "
            f"missing={missing}, incomplete={incomplete}"
        )
    return ReleaseAnalysis("reuse", release_id, False, assets)


def assert_metadata_owned(
    release: dict[str, Any],
    release_tag: str,
    title: str,
    notes: str,
) -> None:
    if release.get("tag_name") != release_tag:
        fail("draft release tag identity mismatch")
    if release.get("name") != title:
        fail("draft release title differs from the exact publication transaction")
    body = release.get("body")
    if not isinstance(body, str) or body.strip() != notes.strip():
        fail("draft release notes differ from the exact publication transaction")


def assert_remote_matches_local(
    remote: RemoteAsset,
    local: LocalAsset,
) -> None:
    if remote.name != local.name:
        fail("release asset name mismatch")
    if remote.state != "uploaded":
        fail(f"release asset is not uploaded: {remote.name} ({remote.state})")
    if remote.size != local.size:
        fail(f"release asset size conflict: {remote.name}")
    if remote.digest != local.digest:
        fail(f"release asset digest conflict: {remote.name}")


def assert_complete_exact(
    release: dict[str, Any],
    release_tag: str,
    expected: dict[str, LocalAsset],
    *,
    require_published: bool,
) -> None:
    analysis = analyze_release(release, release_tag, set(expected))
    if require_published and analysis.state != "reuse":
        fail("release is not durably published")
    remote = {asset.name: asset for asset in analysis.assets}
    if set(remote) != set(expected):
        fail("release exact asset inventory mismatch")
    for name, local in expected.items():
        assert_remote_matches_local(remote[name], local)


def run_gh(
    args: list[str],
    *,
    timeout_seconds: int,
    capture: bool = False,
    operation: str = "release_api",
) -> subprocess.CompletedProcess[str]:
    command = ["gh", *args]
    started = time.monotonic()
    result_code = "error"
    try:
        result = subprocess.run(
            command,
            check=False,
            text=True,
            capture_output=capture,
            timeout=timeout_seconds,
        )
        result_code = "success" if result.returncode == 0 else "api_error"
        return result
    except subprocess.TimeoutExpired:
        result_code = "timeout"
        fail(f"GitHub CLI command timed out after {timeout_seconds}s: {' '.join(command[:3])}")
    except OSError as error:
        result_code = "execution_error"
        raise PublicationError(f"cannot execute GitHub CLI: {error}") from error
    finally:
        # Labels are source-owned constants: never log CLI arguments, token,
        # headers, response bodies, asset paths or release notes.
        elapsed_ms = round((time.monotonic() - started) * 1_000)
        print(
            f"release_api_timing operation={operation} "
            f"elapsed_ms={elapsed_ms} outcome={result_code}",
            file=sys.stderr,
            flush=True,
        )


def get_release(repository: str, release_tag: str) -> dict[str, Any] | None:
    # Fast path: normal reuse is a published release and GitHub supports exact
    # lookup by tag. Only the exceptional recovery path needs draft discovery.
    published = run_gh(
        ["api", f"repos/{repository}/releases/tags/{release_tag}"],
        timeout_seconds=API_TIMEOUT_SECONDS,
        capture=True,
        operation="published_lookup",
    )
    if published.returncode == 0:
        try:
            payload = json.loads(published.stdout)
        except json.JSONDecodeError as error:
            raise PublicationError("GitHub release response is invalid JSON") from error
        if not isinstance(payload, dict):
            fail("GitHub release response is not an object")
        return payload
    published_error = published.stderr or ""
    if "HTTP 404" not in published_error and "Not Found" not in published_error:
        fail(
            "GitHub published release lookup failed: "
            f"{published_error.strip() or published.returncode}"
        )

    # Draft releases are not addressable by the by-tag endpoint. Listing with
    # push access includes drafts; paginate only on this exceptional path.
    result = run_gh(
        [
            "api",
            "--paginate",
            "--slurp",
            f"repos/{repository}/releases?per_page=100",
        ],
        timeout_seconds=API_TIMEOUT_SECONDS,
        capture=True,
        operation="draft_list",
    )
    if result.returncode != 0:
        fail(
            "GitHub draft release listing failed: "
            f"{(result.stderr or '').strip() or result.returncode}"
        )
    try:
        pages = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise PublicationError("GitHub release list response is invalid JSON") from error
    if not isinstance(pages, list):
        fail("GitHub release list response is not an array")
    matches: list[dict[str, Any]] = []
    for page in pages:
        if not isinstance(page, list):
            fail("GitHub release list page is not an array")
        for release in page:
            if not isinstance(release, dict):
                fail("GitHub release list entry is invalid")
            if release.get("tag_name") == release_tag:
                matches.append(release)
    if len(matches) > 1:
        fail(f"multiple GitHub releases claim the same tag: {release_tag}")
    return matches[0] if matches else None


def get_release_by_id(
    repository: str,
    release_id: int,
) -> dict[str, Any] | None:
    if not isinstance(release_id, int) or release_id <= 0:
        fail("GitHub release id is invalid")
    result = run_gh(
        ["api", f"repos/{repository}/releases/{release_id}"],
        timeout_seconds=API_TIMEOUT_SECONDS,
        capture=True,
        operation="exact_release_lookup",
    )
    if result.returncode == 0:
        try:
            payload = json.loads(result.stdout)
        except json.JSONDecodeError as error:
            raise PublicationError(
                "GitHub exact release response is invalid JSON"
            ) from error
        if not isinstance(payload, dict):
            fail("GitHub exact release response is not an object")
        if payload.get("id") != release_id:
            fail("GitHub exact release id mismatch")
        return payload
    error_text = result.stderr or ""
    if "HTTP 404" in error_text or "Not Found" in error_text:
        return None
    fail(
        "GitHub exact release lookup failed: "
        f"{error_text.strip() or result.returncode}"
    )


def delete_starter_asset(
    repository: str,
    release_tag: str,
    release_id: int,
    asset: RemoteAsset,
    expected_names: set[str],
) -> None:
    if asset.state == "uploaded":
        fail(f"refusing to delete uploaded release asset: {asset.name}")
    # The inventory that selected this starter can be stale. Recheck the same
    # exact release and asset immediately before DELETE. This is not an atomic
    # GitHub compare-and-delete, so external writers must still be excluded by
    # the owning publication policy.
    current = get_release_by_id(repository, release_id)
    if current is None or current.get("draft") is not True:
        fail("refusing to delete incomplete asset outside the exact draft")
    analysis = analyze_release(current, release_tag, expected_names)
    fresh = [a for a in analysis.assets if a.name == asset.name]
    if len(fresh) != 1 or fresh[0] != asset:
        fail(f"refusing to delete changed or uploaded draft asset: {asset.name}")
    result = run_gh(
        [
            "api",
            "--method",
            "DELETE",
            f"repos/{repository}/releases/assets/{asset.asset_id}",
        ],
        timeout_seconds=API_TIMEOUT_SECONDS,
        capture=True,
        operation="delete_incomplete_asset",
    )
    if result.returncode != 0:
        fail(
            "failed to remove incomplete draft asset "
            f"{asset.name}: {(result.stderr or '').strip() or result.returncode}"
        )


def release_asset_upload_url(
    release: dict[str, Any],
    repository: str,
    asset_name: str,
) -> str:
    release_id = release.get("id")
    template = release.get("upload_url")
    if not isinstance(release_id, int) or release_id <= 0:
        fail("GitHub draft release id is invalid")
    if not isinstance(template, str) or not template:
        fail("GitHub draft release upload_url is missing")
    base = template.split("{", 1)[0]
    parsed = urlsplit(base)
    expected_path = f"/repos/{repository}/releases/{release_id}/assets"
    if (
        parsed.scheme != "https"
        or parsed.hostname != "uploads.github.com"
        or parsed.port not in (None, 443)
        or parsed.path.casefold() != expected_path.casefold()
        or parsed.query
        or parsed.fragment
    ):
        fail("GitHub draft release upload_url is outside the exact release boundary")
    return base + "?" + urlencode({"name": asset_name})


def _upload_asset(
    repository: str,
    release: dict[str, Any],
    local: LocalAsset,
) -> tuple[int | None, RemoteAsset | None, str]:
    token = os.environ.get("GH_TOKEN")
    if not token:
        fail("GH_TOKEN is required for release asset upload")

    url = release_asset_upload_url(release, repository, local.name)
    parsed = urlsplit(url)
    target = parsed.path + (f"?{parsed.query}" if parsed.query else "")
    connection = http.client.HTTPSConnection(
        parsed.hostname,
        parsed.port or 443,
        timeout=UPLOAD_SOCKET_TIMEOUT_SECONDS,
        context=ssl.create_default_context(),
    )
    started = time.monotonic()
    deadline = started + UPLOAD_TIMEOUT_SECONDS
    sent = 0
    next_progress = UPLOAD_PROGRESS_BYTES
    try:
        connection.putrequest("POST", target, skip_accept_encoding=True)
        connection.putheader("Accept", "application/vnd.github+json")
        connection.putheader("Authorization", f"Bearer {token}")
        connection.putheader("X-GitHub-Api-Version", "2022-11-28")
        connection.putheader("User-Agent", "part-crm-release-publication/1")
        connection.putheader("Content-Type", "application/octet-stream")
        connection.putheader("Content-Length", str(local.size))
        connection.endheaders()

        with local.path.open("rb") as handle:
            while True:
                if time.monotonic() > deadline:
                    return None, None, "release asset upload exceeded bounded deadline"
                chunk = handle.read(UPLOAD_CHUNK_BYTES)
                if not chunk:
                    break
                connection.send(chunk)
                sent += len(chunk)
                if sent >= next_progress or sent == local.size:
                    print(
                        f"release upload progress {local.name}: "
                        f"{sent}/{local.size} bytes",
                        flush=True,
                    )
                    next_progress += UPLOAD_PROGRESS_BYTES
        if sent != local.size:
            return None, None, "release asset upload byte count changed during read"

        response = connection.getresponse()
        body_bytes = response.read(1024 * 1024)
        body = body_bytes.decode("utf-8", errors="replace")
        if response.status != 201:
            return response.status, None, body.strip()
        try:
            payload = json.loads(body)
        except json.JSONDecodeError:
            return response.status, None, "GitHub upload response is invalid JSON"
        if not isinstance(payload, dict):
            return response.status, None, "GitHub upload response is not an object"
        remote = _remote_assets({"assets": [payload]})[0]
        return response.status, remote, ""
    except (OSError, TimeoutError, http.client.HTTPException) as error:
        return None, None, f"{type(error).__name__}: {error}"
    finally:
        connection.close()
        print(
            f"release_upload_timing elapsed_ms={round((time.monotonic() - started) * 1_000)} "
            f"sent_bytes={sent}",
            file=sys.stderr,
            flush=True,
        )


def upload_one(
    repository: str,
    release_tag: str,
    release: dict[str, Any],
    local: LocalAsset,
    expected_names: set[str],
) -> None:
    status, uploaded, detail = _upload_asset(repository, release, local)
    if status == 201 and uploaded is not None:
        assert_remote_matches_local(uploaded, local)
        return

    # A transport/API failure can happen after GitHub committed the bytes.
    # Re-observe once and accept only exact durable bytes. With the accepted-main
    # workflow serialized, incomplete draft residue is recovered by the next run
    # rather than blindly retrying a heavy upload in the same transaction.
    release_id = release.get("id")
    if not isinstance(release_id, int) or release_id <= 0:
        fail("GitHub draft release id is invalid")
    observed = get_release_by_id(repository, release_id)
    if observed is None:
        fail(f"release disappeared after failed upload: {release_tag}")
    analysis = analyze_release(observed, release_tag, expected_names)
    candidates = [asset for asset in analysis.assets if asset.name == local.name]
    if len(candidates) == 1 and candidates[0].state == "uploaded":
        assert_remote_matches_local(candidates[0], local)
        return
    if (
        observed.get("draft") is True
        and len(candidates) == 1
        and candidates[0].state != "uploaded"
    ):
        fail(
            "release asset upload left an incomplete draft asset; "
            f"next serialized run will recover it: {local.name}; "
            f"status={status!r} detail={detail!r}"
        )

    fail(
        "release asset upload failed without exact durable bytes: "
        f"{local.name}; status={status!r} detail={detail!r}"
    )


def ensure_draft_release(
    repository: str,
    release_tag: str,
    target: str,
    title: str,
    notes: str,
) -> dict[str, Any]:
    release = get_release(repository, release_tag)
    if release is not None:
        return release

    result = run_gh(
        [
            "api",
            "--method",
            "POST",
            f"repos/{repository}/releases",
            "-f",
            f"tag_name={release_tag}",
            "-f",
            f"target_commitish={target}",
            "-f",
            f"name={title}",
            "-f",
            f"body={notes}",
            "-F",
            "draft=true",
            "-F",
            "prerelease=false",
        ],
        timeout_seconds=API_TIMEOUT_SECONDS,
        capture=True,
        operation="create_draft",
    )
    if result.returncode != 0:
        release = get_release(repository, release_tag)
        if release is None:
            fail(
                "draft release creation failed: "
                f"{(result.stderr or '').strip() or result.returncode}"
            )
        return release
    try:
        release = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise PublicationError("draft release creation response is invalid JSON") from error
    if not isinstance(release, dict):
        fail("draft release creation response is not an object")
    if release.get("draft") is not True:
        fail("new release was not created as draft")
    if release.get("tag_name") != release_tag:
        fail("new draft release tag identity mismatch")
    return release


def publish(
    *,
    repository: str,
    release_tag: str,
    target: str,
    title: str,
    notes: str,
    assets: dict[str, LocalAsset],
) -> str:
    expected_names = set(assets)
    # A single discovery is enough for both exact published replay and draft
    # creation/resumption. A failed create still uses exact recovery discovery.
    release = ensure_draft_release(repository, release_tag, target, title, notes)
    analysis = analyze_release(release, release_tag, expected_names)
    if analysis.state == "reuse":
        assert_complete_exact(
            release,
            release_tag,
            assets,
            require_published=True,
        )
        return "reused"
    if release.get("draft") is not True:
        fail("incomplete publication cannot be resumed from a non-draft release")
    assert_metadata_owned(release, release_tag, title, notes)
    transaction_release_id = release.get("id")
    if not isinstance(transaction_release_id, int) or transaction_release_id <= 0:
        fail("GitHub draft release id is invalid")

    remote_by_name = {asset.name: asset for asset in analysis.assets}
    for name, local in assets.items():
        remote = remote_by_name.get(name)
        if remote is not None and remote.state == "uploaded":
            assert_remote_matches_local(remote, local)
            continue
        if remote is not None:
            delete_starter_asset(
                repository, release_tag, transaction_release_id, remote,
                expected_names,
            )
        upload_one(repository, release_tag, release, local, expected_names)

        # GitHub's published releases may be mutable. Observe between uploads
        # to avoid POSTing another asset after a peer publishes a partial draft.
        current = get_release_by_id(repository, transaction_release_id)
        if current is None:
            fail("draft release disappeared before asset verification")
        if current.get("draft") is not True:
            assert_complete_exact(
                current,
                release_tag,
                assets,
                require_published=True,
            )
            return "published-by-peer"
        current_analysis = analyze_release(current, release_tag, expected_names)
        exact = {asset.name: asset for asset in current_analysis.assets}.get(name)
        if exact is None:
            fail(f"uploaded release asset cannot be observed: {name}")
        assert_remote_matches_local(exact, local)

    # Keep a distinct fresh read immediately before PATCH. The published
    # release is mutable, so a peer can change assets after the last upload.
    completed_draft = get_release_by_id(repository, transaction_release_id)
    if completed_draft is None:
        fail("completed draft release cannot be observed")
    if completed_draft.get("draft") is not True:
        assert_complete_exact(
            completed_draft,
            release_tag,
            assets,
            require_published=True,
        )
        return "published-by-peer"
    assert_metadata_owned(completed_draft, release_tag, title, notes)
    assert_complete_exact(
        completed_draft,
        release_tag,
        assets,
        require_published=False,
    )

    completed_release_id = completed_draft.get("id")
    if not isinstance(completed_release_id, int) or completed_release_id <= 0:
        fail("completed draft release id is invalid")
    edit = run_gh(
        [
            "api",
            "--method",
            "PATCH",
            f"repos/{repository}/releases/{completed_release_id}",
            "-F",
            "draft=false",
            "-F",
            "prerelease=false",
        ],
        timeout_seconds=API_TIMEOUT_SECONDS,
        capture=True,
        operation="publish_draft",
    )
    if edit.returncode != 0:
        final = get_release_by_id(repository, completed_release_id)
        if final is None:
            fail(
                "draft publication failed and release disappeared: "
                f"{(edit.stderr or '').strip() or edit.returncode}"
            )
        assert_complete_exact(final, release_tag, assets, require_published=True)
        return "published-by-peer"

    final = get_release_by_id(repository, completed_release_id)
    if final is None:
        fail("published release cannot be observed")
    assert_complete_exact(final, release_tag, assets, require_published=True)
    return "published"


def write_json(path: Path | None, payload: dict[str, Any]) -> None:
    text = json.dumps(payload, sort_keys=True, indent=2) + "\n"
    if path is None:
        print(text, end="")
        return
    if path.exists():
        fail(f"output already exists: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="\n")


def probe(
    *,
    repository: str,
    release_tag: str,
    expected_names: set[str],
    output: Path | None,
) -> None:
    release = get_release(repository, release_tag)
    analysis = analyze_release(release, release_tag, expected_names)
    payload = {
        "schema_version": 1,
        "kind": "GITHUB_RELEASE_PUBLICATION_PROBE",
        "release_id": release_tag,
        "state": analysis.state,
        "expected_assets": sorted(expected_names),
    }
    write_json(output, payload)


def self_test() -> None:
    release_tag = "runtime-bundle-v3-sha256-" + "a" * 64
    expected = {"runtime-bundle.tar.gz", "runtime-manifest.json"}

    if analyze_release(None, release_tag, expected).state != "absent":
        fail("absent release self-test failed")

    upload_fixture = {
        "id": 10,
        "upload_url": (
            "https://uploads.github.com/repos/owner/repo/releases/10/assets"
            "{?name,label}"
        ),
    }
    expected_upload = (
        "https://uploads.github.com/repos/owner/repo/releases/10/assets"
        "?name=runtime-bundle.tar.gz"
    )
    if (
        release_asset_upload_url(
            upload_fixture,
            "owner/repo",
            "runtime-bundle.tar.gz",
        )
        != expected_upload
    ):
        fail("release upload URL self-test failed")
    escaped_upload = dict(upload_fixture)
    escaped_upload["upload_url"] = (
        "https://evil.example/repos/owner/repo/releases/10/assets{?name,label}"
    )
    try:
        release_asset_upload_url(
            escaped_upload,
            "owner/repo",
            "runtime-bundle.tar.gz",
        )
    except PublicationError:
        pass
    else:
        fail("release upload URL boundary negative self-test unexpectedly passed")

    exact_fixture = {
        "id": 10,
        "tag_name": release_tag,
        "draft": True,
        "prerelease": False,
        "assets": [],
    }
    original_run_gh = globals()["run_gh"]

    def fake_exact_run_gh(
        args: list[str],
        *,
        timeout_seconds: int,
        capture: bool = False,
        operation: str = "release_api",
    ) -> subprocess.CompletedProcess[str]:
        if args != ["api", "repos/owner/repo/releases/10"]:
            fail(f"unexpected exact release lookup command in self-test: {args}")
        if (timeout_seconds != API_TIMEOUT_SECONDS or capture is not True
                or operation != "exact_release_lookup"):
            fail("exact release lookup self-test command options changed")
        return subprocess.CompletedProcess(
            ["gh", *args],
            0,
            json.dumps(exact_fixture),
            "",
        )

    globals()["run_gh"] = fake_exact_run_gh
    try:
        exact_observed = get_release_by_id("owner/repo", 10)
    finally:
        globals()["run_gh"] = original_run_gh
    if exact_observed != exact_fixture:
        fail("exact release-id lookup self-test failed")

    complete = {
        "id": 10,
        "tag_name": release_tag,
        "draft": False,
        "prerelease": False,
        "assets": [
            {
                "id": 1,
                "name": "runtime-bundle.tar.gz",
                "state": "uploaded",
                "size": 100,
                "digest": "sha256:" + "1" * 64,
            },
            {
                "id": 2,
                "name": "runtime-manifest.json",
                "state": "uploaded",
                "size": 10,
                "digest": "sha256:" + "2" * 64,
            },
        ],
    }
    if analyze_release(complete, release_tag, expected).state != "reuse":
        fail("complete published release self-test failed")

    partial = json.loads(json.dumps(complete))
    partial["draft"] = True
    partial["assets"][0]["state"] = "starter"
    partial["assets"][0]["size"] = 0
    partial["assets"][0]["digest"] = None
    partial["assets"].pop()
    if analyze_release(partial, release_tag, expected).state != "resume":
        fail("partial draft release self-test failed")

    published_partial = json.loads(json.dumps(partial))
    published_partial["draft"] = False
    try:
        analyze_release(published_partial, release_tag, expected)
    except PublicationError:
        pass
    else:
        fail("published partial release negative self-test unexpectedly passed")

    extra = json.loads(json.dumps(complete))
    extra["assets"].append(
        {
            "id": 3,
            "name": "unexpected.bin",
            "state": "uploaded",
            "size": 1,
            "digest": "sha256:" + "3" * 64,
        }
    )
    try:
        analyze_release(extra, release_tag, expected)
    except PublicationError:
        pass
    else:
        fail("unexpected asset negative self-test unexpectedly passed")

    local = LocalAsset(
        name="runtime-bundle.tar.gz",
        path=Path("runtime-bundle.tar.gz"),
        size=100,
        digest="sha256:" + "1" * 64,
    )
    assert_remote_matches_local(_remote_assets(complete)[0], local)
    conflicting = LocalAsset(
        name=local.name,
        path=local.path,
        size=101,
        digest=local.digest,
    )
    try:
        assert_remote_matches_local(_remote_assets(complete)[0], conflicting)
    except PublicationError:
        pass
    else:
        fail("asset conflict negative self-test unexpectedly passed")

    self_test_publication_protocol()
    print("GitHub Release publication self-test passed.")


def self_test_publication_protocol() -> None:
    """Exercise the real publisher against a closed, mutable fake GitHub API.

    No network, credentials, uploads or filesystem writes are used. Changing
    the publication path must retain these exact transitions and refusals.
    """
    release_tag = "release-set-v3-sha256-" + "a" * 64
    repository, target, title, notes = "owner/repo", "b" * 40, release_tag, "test release"
    assets = {
        name: LocalAsset(name, Path(name), size, "sha256:" + digest * 64)
        for name, size, digest in (("first.bin", 10, "1"), ("second.bin", 20, "2"))
    }

    class FakeGithub:
        def __init__(self, scenario: str) -> None:
            self.scenario = scenario
            self.release: dict[str, Any] | None = None
            self.calls: list[str] = []
            self.uploads = 0
            self.exact_reads = 0
            if scenario in ("replay", "published_conflict"):
                self.create_draft()
                assert self.release is not None
                self.release["draft"] = False
                self.release["assets"] = [
                    self.remote(local) for local in assets.values()
                ]
                if scenario == "published_conflict":
                    self.release["assets"][0]["digest"] = "sha256:" + "f" * 64
            if scenario in ("resume_starter", "starter_race"):
                self.create_draft()
                assert self.release is not None
                starter = self.remote(assets["first.bin"])
                starter.update(state="starter", size=0, digest=None)
                self.release["assets"] = [starter]

        def create_draft(self) -> None:
            self.release = {
                "id": 10, "tag_name": release_tag, "name": title,
                "body": notes, "draft": True, "prerelease": False,
                "upload_url": (
                    "https://uploads.github.com/repos/owner/repo/releases/10/assets"
                    "{?name,label}"
                ),
                "assets": [],
            }

        def remote(self, local: LocalAsset) -> dict[str, Any]:
            return {
                "id": 20 if local.name == "first.bin" else 21,
                "name": local.name, "state": "uploaded",
                "size": local.size, "digest": local.digest,
            }

        def cli(self, args: list[str], *, timeout_seconds: int,
                capture: bool = False,
                operation: str = "release_api") -> subprocess.CompletedProcess[str]:
            if timeout_seconds != API_TIMEOUT_SECONDS or not capture:
                fail("publication self-test received unexpected CLI parameters")
            self.calls.append(operation)
            if self.scenario == "api_timeout" and operation == "exact_release_lookup":
                fail("simulated GitHub API timeout")
            release = self.release
            if operation == "published_lookup":
                payload = release if release is not None and not release["draft"] else None
            elif operation == "draft_list":
                payload = [[release]] if release is not None and release["draft"] else [[]]
            elif operation == "exact_release_lookup":
                self.exact_reads += 1
                if release is not None and self.scenario == "starter_race":
                    # A peer completed an earlier starter before DELETE.
                    release["assets"][0].update(
                        state="uploaded", size=assets["first.bin"].size,
                        digest=assets["first.bin"].digest,
                    )
                if (release is not None and self.scenario == "peer_inject_prepatch"
                        and self.exact_reads == 3):
                    # Changes between the last post-upload GET and the
                    # separate, immediately pre-PATCH GET must block publish.
                    release["assets"].append({
                        "id": 99, "name": "foreign.bin", "state": "uploaded",
                        "size": 3, "digest": "sha256:" + "3" * 64,
                    })
                if (release is not None and self.scenario == "postpatch_tamper"
                        and release["draft"] is False):
                    release["assets"][0]["digest"] = "sha256:" + "e" * 64
                payload = release
                if payload is not None and self.scenario == "foreign_release_id":
                    payload = dict(payload, id=11)
                if self.scenario == "release_disappeared":
                    payload = None
            elif operation == "create_draft":
                if release is not None:
                    fail("duplicate draft creation in publication self-test")
                if self.scenario == "create_failed":
                    return subprocess.CompletedProcess(args, 1, "", "HTTP 503")
                self.create_draft()
                if self.scenario == "create_peer_collision":
                    assert self.release is not None
                    self.release["body"] = "a conflicting publisher's draft"
                if self.scenario in ("create_ack_lost", "create_peer_collision"):
                    return subprocess.CompletedProcess(args, 1, "", "timeout or HTTP 409")
                payload = self.release
            elif operation == "delete_incomplete_asset":
                if release is None or release["draft"] is not True:
                    fail("attempted to delete a published release asset")
                asset_id = int(args[3].rsplit("/", 1)[1])
                release["assets"] = [
                    asset for asset in release["assets"] if asset["id"] != asset_id
                ]
                payload = {}
            elif operation == "publish_draft":
                if release is None or not release["draft"]:
                    fail("attempted to publish a non-draft release")
                release["draft"] = False
                if self.scenario == "publish_ack_lost":
                    return subprocess.CompletedProcess(args, 1, "", "timeout")
                payload = release
            else:
                fail(f"unexpected self-test GitHub API operation: {operation}")
            if payload is None:
                return subprocess.CompletedProcess(args, 1, "", "HTTP 404: Not Found")
            return subprocess.CompletedProcess(args, 0, json.dumps(payload), "")

        def upload(self, repository_arg: str, release: dict[str, Any],
                   local: LocalAsset) -> tuple[int | None, RemoteAsset | None, str]:
            if repository_arg != repository or release.get("id") != 10:
                fail("wrong publication upload identity in self-test")
            self.uploads += 1
            if self.release is None:
                fail("upload before release creation in self-test")
            if not self.release["draft"] and self.scenario == "peer_incomplete":
                # Real GitHub releases can be mutable (immutable=false). If
                # publisher posts after the peer's early publication, this
                # mock intentionally permits the unsafe mutation.
                self.release["assets"].append(self.remote(local))
                return 201, _remote_assets({"assets": [self.remote(local)]})[0], ""
            if not self.release["draft"]:
                return 422, None, "already published"
            remote = self.remote(local)
            self.release["assets"].append(remote)
            if self.scenario == "wrong_upload_digest" and self.uploads == 1:
                bad = dict(remote, digest="sha256:" + "e" * 64)
                return 201, _remote_assets({"assets": [bad]})[0], ""
            if self.scenario == "wrong_upload_name" and self.uploads == 1:
                bad = dict(remote, name="wrong.bin")
                return 201, _remote_assets({"assets": [bad]})[0], ""
            if self.scenario == "wrong_upload_size" and self.uploads == 1:
                bad = dict(remote, size=local.size + 1)
                return 201, _remote_assets({"assets": [bad]})[0], ""
            if self.scenario == "lost_upload_ack" and self.uploads == 1:
                return None, None, "connection reset after durable upload"
            if self.scenario == "starter_ack_lost" and self.uploads == 1:
                remote.update(state="starter", size=0, digest=None)
                return None, None, "connection reset with starter residue"
            if self.scenario in ("peer_complete", "peer_incomplete") and self.uploads == 1:
                if self.scenario == "peer_complete":
                    self.release["assets"] = [self.remote(item) for item in assets.values()]
                self.release["draft"] = False
            if self.scenario == "extra_asset" and self.uploads == 2:
                self.release["assets"].append({
                    "id": 99, "name": "foreign.bin", "state": "uploaded",
                    "size": 3, "digest": "sha256:" + "3" * 64,
                })
            if self.scenario == "duplicate_asset" and self.uploads == 2:
                self.release["assets"].append(dict(remote, id=98))
            if self.scenario == "tampered_asset" and self.uploads == 2:
                self.release["assets"][0]["digest"] = "sha256:" + "e" * 64
            if self.scenario == "missing_asset" and self.uploads == 2:
                self.release["assets"].pop(0)
            return 201, _remote_assets({"assets": [remote]})[0], ""

    original_run_gh = globals()["run_gh"]
    original_upload = globals()["_upload_asset"]
    cases = {
        "initial": ("published", 2),
        "replay": ("reused", 0),
        "resume_starter": ("published", 2),
        "create_ack_lost": ("published", 2),
        "create_peer_collision": (None, 0),
        "lost_upload_ack": ("published", 2),
        "peer_complete": ("published-by-peer", 1),
        "publish_ack_lost": ("published-by-peer", 2),
        "wrong_upload_digest": (None, 1),
        "wrong_upload_name": (None, 1),
        "wrong_upload_size": (None, 1),
        "starter_ack_lost": (None, 1),
        "starter_race": (None, 0),
        "create_failed": (None, 0),
        "api_timeout": (None, 1),
        "peer_incomplete": (None, 1),
        "foreign_release_id": (None, 1),
        "release_disappeared": (None, 1),
        "extra_asset": (None, 2),
        "peer_inject_prepatch": (None, 2),
        "postpatch_tamper": (None, 2),
        "duplicate_asset": (None, 2),
        "tampered_asset": (None, 2),
        "missing_asset": (None, 2),
        "published_conflict": (None, 0),
    }
    try:
        for scenario, (outcome, upload_count) in cases.items():
            fake = FakeGithub(scenario)
            globals()["run_gh"] = fake.cli
            globals()["_upload_asset"] = fake.upload
            try:
                result = publish(
                    repository=repository, release_tag=release_tag,
                    target=target, title=title, notes=notes, assets=assets,
                )
            except PublicationError:
                if outcome is not None:
                    fail(f"publication positive self-test failed: {scenario}")
            else:
                if result != outcome:
                    fail(f"publication negative/replay self-test failed: {scenario}")
            if fake.uploads != upload_count:
                fail(f"publication uploaded unexpected number of times: {scenario}")
            own_patches = fake.calls.count("publish_draft")
            expected_patches = int(
                outcome == "published"
                or scenario in ("publish_ack_lost", "postpatch_tamper")
            )
            if own_patches != expected_patches:
                fail(f"publication performed unexpected PATCH: {scenario}")
            if scenario == "initial" and (
                fake.calls.count("published_lookup") != 1
                or fake.calls.count("draft_list") != 1
                or fake.calls.count("exact_release_lookup") != 4
                or fake.calls.count("publish_draft") != 1
            ):
                fail("publication success path made redundant GitHub API requests")
            if scenario == "lost_upload_ack" and (
                fake.calls.count("exact_release_lookup") != 5
            ):
                fail("lost upload ACK did not require one exact recovery lookup")
            if scenario == "replay" and fake.calls != ["published_lookup"]:
                fail("published exact replay performed a mutation or extra lookup")
            if scenario == "resume_starter" and (
                fake.calls.count("delete_incomplete_asset") != 1
            ):
                fail("partial draft did not retire the incomplete starter asset")
            if scenario == "starter_race" and (
                fake.calls.count("delete_incomplete_asset") != 0
            ):
                fail("starter race deleted an uploaded peer asset")
    finally:
        globals()["run_gh"] = original_run_gh
        globals()["_upload_asset"] = original_upload

    original_subprocess_run = subprocess.run

    def simulate_cli_timeout(*args: Any, **kwargs: Any) -> Any:
        raise subprocess.TimeoutExpired(args[0], kwargs["timeout"])

    try:
        subprocess.run = simulate_cli_timeout
        timer_log = io.StringIO()
        with contextlib.redirect_stderr(timer_log):
            try:
                run_gh(
                    ["api", "repos/owner/repo/releases/10"],
                    timeout_seconds=1, capture=True, operation="exact_release_lookup",
                )
            except PublicationError as error:
                if "timed out" not in str(error):
                    fail("real subprocess timeout was misclassified")
            else:
                fail("real subprocess timeout did not fail closed")
        if "outcome=timeout" not in timer_log.getvalue():
            fail("timeout API telemetry was not reported")
    finally:
        subprocess.run = original_subprocess_run


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    subcommands = result.add_subparsers(dest="command", required=True)

    probe_parser = subcommands.add_parser("probe")
    probe_parser.add_argument("--repository", required=True)
    probe_parser.add_argument("--release-id", required=True)
    probe_parser.add_argument(
        "--expected-asset",
        action="append",
        required=True,
        dest="expected_assets",
    )
    probe_parser.add_argument("--output", type=Path)

    publish_parser = subcommands.add_parser("publish")
    publish_parser.add_argument("--repository", required=True)
    publish_parser.add_argument("--release-id", required=True)
    publish_parser.add_argument("--target", required=True)
    publish_parser.add_argument("--title", required=True)
    publish_parser.add_argument("--notes", required=True)
    publish_parser.add_argument("--asset", action="append", required=True, type=Path)

    subcommands.add_parser("self-test")
    return result


def main() -> int:
    args = parser().parse_args()
    try:
        if args.command == "self-test":
            self_test()
        elif args.command == "probe":
            probe(
                repository=args.repository,
                release_tag=args.release_id,
                expected_names=set(args.expected_assets),
                output=args.output,
            )
        elif args.command == "publish":
            assets = local_assets(args.asset)
            status = publish(
                repository=args.repository,
                release_tag=args.release_id,
                target=args.target,
                title=args.title,
                notes=args.notes,
                assets=assets,
            )
            print(
                json.dumps(
                    {
                        "schema_version": 1,
                        "kind": "GITHUB_RELEASE_PUBLICATION_RESULT",
                        "release_id": args.release_id,
                        "status": status,
                        "assets": sorted(assets),
                    },
                    sort_keys=True,
                )
            )
        else:
            fail(f"unsupported command: {args.command}")
        return 0
    except PublicationError as error:
        print(f"release publication error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
