#!/usr/bin/env python3
"""Publish GitHub Release assets with bounded, resumable, fail-closed semantics.

This helper is the single publication primitive for content-addressed component
and aggregate releases. New publications are staged as drafts, partial draft
uploads may be resumed only when already-uploaded bytes are exact, and a
published release is accepted only when its complete asset inventory is exact.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable

DIGEST_RE = re.compile(r"^sha256:([0-9a-f]{64})$")
API_TIMEOUT_SECONDS = 120
UPLOAD_TIMEOUT_SECONDS = 5_400


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
) -> subprocess.CompletedProcess[str]:
    command = ["gh", *args]
    try:
        return subprocess.run(
            command,
            check=False,
            text=True,
            capture_output=capture,
            timeout=timeout_seconds,
        )
    except subprocess.TimeoutExpired:
        fail(f"GitHub CLI command timed out after {timeout_seconds}s: {' '.join(command[:3])}")
    except OSError as error:
        raise PublicationError(f"cannot execute GitHub CLI: {error}") from error


def get_release(repository: str, release_tag: str) -> dict[str, Any] | None:
    # The by-tag REST endpoint returns only published releases. Listing releases
    # with push access includes drafts, which is required for resumable partial
    # publication.
    result = run_gh(
        [
            "api",
            "--paginate",
            "--slurp",
            f"repos/{repository}/releases?per_page=100",
        ],
        timeout_seconds=API_TIMEOUT_SECONDS,
        capture=True,
    )
    if result.returncode != 0:
        fail(
            "GitHub release listing failed: "
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


def delete_starter_asset(repository: str, asset: RemoteAsset) -> None:
    if asset.state == "uploaded":
        fail(f"refusing to delete uploaded release asset: {asset.name}")
    result = run_gh(
        [
            "api",
            "--method",
            "DELETE",
            f"repos/{repository}/releases/assets/{asset.asset_id}",
        ],
        timeout_seconds=API_TIMEOUT_SECONDS,
        capture=True,
    )
    if result.returncode != 0:
        fail(
            "failed to remove incomplete draft asset "
            f"{asset.name}: {(result.stderr or '').strip() or result.returncode}"
        )


def _upload_asset(
    repository: str,
    release_tag: str,
    local: LocalAsset,
) -> subprocess.CompletedProcess[str]:
    return run_gh(
        [
            "release",
            "upload",
            release_tag,
            "--repo",
            repository,
            str(local.path),
        ],
        timeout_seconds=UPLOAD_TIMEOUT_SECONDS,
        capture=False,
    )


def upload_one(
    repository: str,
    release_tag: str,
    local: LocalAsset,
    expected_names: set[str],
) -> None:
    result = _upload_asset(repository, release_tag, local)
    if result.returncode == 0:
        return

    # A concurrent exact publisher may have won the name race. Re-observe
    # before deciding this upload failed.
    release = get_release(repository, release_tag)
    if release is None:
        fail(f"release disappeared after failed upload: {release_tag}")
    analysis = analyze_release(release, release_tag, expected_names)
    candidates = [asset for asset in analysis.assets if asset.name == local.name]
    if len(candidates) == 1 and candidates[0].state == "uploaded":
        assert_remote_matches_local(candidates[0], local)
        return

    # A failed upload may leave a GitHub "starter" asset. Only an incomplete
    # asset on our still-draft transaction is removable; uploaded conflicting
    # bytes are never clobbered.
    if (
        release.get("draft") is True
        and len(candidates) == 1
        and candidates[0].state != "uploaded"
    ):
        delete_starter_asset(repository, candidates[0])
        retry = _upload_asset(repository, release_tag, local)
        if retry.returncode == 0:
            return
        final = get_release(repository, release_tag)
        if final is not None:
            final_analysis = analyze_release(final, release_tag, expected_names)
            final_asset = {
                asset.name: asset for asset in final_analysis.assets
            }.get(local.name)
            if final_asset is not None and final_asset.state == "uploaded":
                assert_remote_matches_local(final_asset, local)
                return

    fail(f"release asset upload failed and no exact concurrent asset appeared: {local.name}")


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
            "release",
            "create",
            release_tag,
            "--repo",
            repository,
            "--target",
            target,
            "--title",
            title,
            "--notes",
            notes,
            "--draft",
        ],
        timeout_seconds=API_TIMEOUT_SECONDS,
        capture=True,
    )
    if result.returncode != 0:
        release = get_release(repository, release_tag)
        if release is None:
            fail(
                "draft release creation failed: "
                f"{(result.stderr or '').strip() or result.returncode}"
            )
        return release

    release = get_release(repository, release_tag)
    if release is None:
        fail("draft release was created but cannot be observed")
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
    observed = get_release(repository, release_tag)
    if observed is not None:
        analysis = analyze_release(observed, release_tag, expected_names)
        if analysis.state == "reuse":
            assert_complete_exact(
                observed,
                release_tag,
                assets,
                require_published=True,
            )
            return "reused"

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

    remote_by_name = {asset.name: asset for asset in analysis.assets}
    for name, local in assets.items():
        remote = remote_by_name.get(name)
        if remote is not None and remote.state == "uploaded":
            assert_remote_matches_local(remote, local)
            continue
        if remote is not None:
            delete_starter_asset(repository, remote)
        upload_one(repository, release_tag, local, expected_names)

        current = get_release(repository, release_tag)
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

    completed_draft = get_release(repository, release_tag)
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

    edit = run_gh(
        [
            "release",
            "edit",
            release_tag,
            "--repo",
            repository,
            "--draft=false",
        ],
        timeout_seconds=API_TIMEOUT_SECONDS,
        capture=True,
    )
    if edit.returncode != 0:
        final = get_release(repository, release_tag)
        if final is None:
            fail(
                "draft publication failed and release disappeared: "
                f"{(edit.stderr or '').strip() or edit.returncode}"
            )
        assert_complete_exact(final, release_tag, assets, require_published=True)
        return "published-by-peer"

    final = get_release(repository, release_tag)
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
    expected = {"runtime-bundle.tar", "runtime-manifest.json"}

    if analyze_release(None, release_tag, expected).state != "absent":
        fail("absent release self-test failed")

    complete = {
        "id": 10,
        "tag_name": release_tag,
        "draft": False,
        "prerelease": False,
        "assets": [
            {
                "id": 1,
                "name": "runtime-bundle.tar",
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
        name="runtime-bundle.tar",
        path=Path("runtime-bundle.tar"),
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

    print("GitHub Release publication self-test passed.")


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
