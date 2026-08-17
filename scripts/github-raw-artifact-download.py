#!/usr/bin/env python3
"""Download one immutable raw GitHub Actions artifact with fail-closed authority checks."""

from __future__ import annotations

import argparse
import email.utils
import hashlib
import io
import json
import os
import re
import tarfile
import tempfile
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from datetime import timezone
from pathlib import Path, PurePosixPath
from typing import Callable, Mapping, Protocol
from urllib.parse import urlparse

COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
DIGEST_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
REPOSITORY_RE = re.compile(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")
ARTIFACT_NAME_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,254}\.tar$")
MAX_ATTEMPTS = 5
BASE_BACKOFF_SECONDS = 1.0
MAX_BACKOFF_SECONDS = 8.0
MAX_RETRY_AFTER_SECONDS = 60.0
MAX_ARTIFACT_BYTES = 512 * 1024 * 1024
USER_AGENT = "part-crm-raw-artifact-acquisition"
API_VERSION = "2022-11-28"


class ArtifactDownloadError(ValueError):
    """Raised when immutable artifact acquisition cannot be proven safe."""


def fail(message: str) -> None:
    raise ArtifactDownloadError(message)


@dataclass(frozen=True)
class HttpResult:
    status: int
    headers: Mapping[str, str]
    body: bytes


class Transport(Protocol):
    def get(self, url: str, headers: Mapping[str, str], *, follow_redirects: bool) -> HttpResult: ...


class NoRedirectHandler(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):  # type: ignore[no-untyped-def]
        return None


class UrlLibTransport:
    def __init__(self) -> None:
        self._direct = urllib.request.build_opener(NoRedirectHandler())

    def get(self, url: str, headers: Mapping[str, str], *, follow_redirects: bool) -> HttpResult:
        request = urllib.request.Request(url, headers=dict(headers), method="GET")
        try:
            response = (
                urllib.request.urlopen(request, timeout=20)
                if follow_redirects
                else self._direct.open(request, timeout=20)
            )
            with response:
                body = response.read(MAX_ARTIFACT_BYTES + 1)
                if len(body) > MAX_ARTIFACT_BYTES:
                    fail("GitHub artifact response exceeded the bounded body limit")
                return HttpResult(response.status, dict(response.headers.items()), body)
        except urllib.error.HTTPError as error:
            body = error.read() if error.fp is not None else b""
            return HttpResult(error.code, dict(error.headers.items()) if error.headers else {}, body)


def api_headers(token: str) -> dict[str, str]:
    if not token:
        fail("GITHUB_TOKEN is required for raw artifact acquisition")
    return {
        "Accept": "application/vnd.github+json",
        "Authorization": f"Bearer {token}",
        "User-Agent": USER_AGENT,
        "X-GitHub-Api-Version": API_VERSION,
    }


def parse_positive_int(value: str, label: str) -> int:
    if not value.isdigit() or int(value) <= 0:
        fail(f"{label} must be a positive integer")
    return int(value)


def retry_after_seconds(value: str | None, *, now: Callable[[], float] = time.time) -> float | None:
    if not value:
        return None
    stripped = value.strip()
    if stripped.isdigit():
        return min(float(stripped), MAX_RETRY_AFTER_SECONDS)
    try:
        parsed = email.utils.parsedate_to_datetime(stripped)
    except (TypeError, ValueError):
        return None
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    seconds = max(0.0, parsed.timestamp() - now())
    return min(seconds, MAX_RETRY_AFTER_SECONDS)


def retry_delay(attempt: int, headers: Mapping[str, str]) -> float:
    retry_after = retry_after_seconds(headers.get("Retry-After") or headers.get("retry-after"))
    if retry_after is not None:
        return retry_after
    return min(BASE_BACKOFF_SECONDS * (2 ** (attempt - 1)), MAX_BACKOFF_SECONDS)


def request_with_retry(
    transport: Transport,
    url: str,
    headers: Mapping[str, str],
    *,
    follow_redirects: bool,
    success_statuses: set[int],
    sleep: Callable[[float], None] = time.sleep,
    max_attempts: int = MAX_ATTEMPTS,
) -> HttpResult:
    if max_attempts < 1:
        fail("retry attempts must be positive")
    last_error: BaseException | None = None
    for attempt in range(1, max_attempts + 1):
        try:
            result = transport.get(url, headers, follow_redirects=follow_redirects)
        except (urllib.error.URLError, TimeoutError, OSError) as error:
            last_error = error
            if attempt == max_attempts:
                break
            sleep(retry_delay(attempt, {}))
            continue
        if result.status in success_statuses:
            return result
        if result.status == 429 or 500 <= result.status <= 599:
            if attempt == max_attempts:
                fail(f"transient GitHub artifact request exhausted after {max_attempts} attempts: HTTP {result.status}")
            sleep(retry_delay(attempt, result.headers))
            continue
        fail(f"GitHub artifact request failed closed without retry: HTTP {result.status}")
    fail(f"GitHub artifact request exhausted after {max_attempts} transport failures: {type(last_error).__name__}")


def decode_json(result: HttpResult, label: str) -> dict[str, object]:
    try:
        value = json.loads(result.body.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ArtifactDownloadError(f"{label} is not strict UTF-8 JSON") from error
    if not isinstance(value, dict):
        fail(f"{label} must be a JSON object")
    return value


def validate_run(run: dict[str, object], *, run_id: int, source_sha: str) -> None:
    expected = {
        "id": run_id,
        "head_branch": "main",
        "head_sha": source_sha,
        "status": "completed",
        "conclusion": "success",
    }
    mismatches = {key: {"expected": value, "actual": run.get(key)} for key, value in expected.items() if run.get(key) != value}
    if mismatches:
        fail(f"release workflow run is not exact successful accepted-main authority: {mismatches}")


def validate_artifact_metadata(
    artifact: dict[str, object],
    *,
    artifact_id: int,
    run_id: int,
    source_sha: str,
    expected_digest: str,
    expected_name: str,
) -> int:
    if artifact.get("id") != artifact_id:
        fail("artifact metadata id differs from the requested immutable artifact")
    if artifact.get("name") != expected_name:
        fail("artifact name differs from the expected raw release tar name")
    if artifact.get("digest") != expected_digest:
        fail("artifact metadata digest differs from the supplied immutable digest")
    if artifact.get("expired") is not False:
        fail("artifact is expired or retention state is unknown")
    size = artifact.get("size_in_bytes")
    if not isinstance(size, int) or size <= 0 or size > MAX_ARTIFACT_BYTES:
        fail("artifact size is outside the bounded raw-artifact limit")
    workflow_run = artifact.get("workflow_run")
    if not isinstance(workflow_run, dict):
        fail("artifact workflow ownership metadata is missing")
    if workflow_run.get("id") != run_id or workflow_run.get("head_sha") != source_sha:
        fail("artifact is not owned by the exact supplied release run/source SHA")
    return size


def validate_raw_tar(payload: bytes) -> None:
    if not payload:
        fail("downloaded raw artifact is empty")
    try:
        with tarfile.open(fileobj=io.BytesIO(payload), mode="r:") as archive:
            members = archive.getmembers()
    except tarfile.TarError as error:
        raise ArtifactDownloadError("downloaded artifact is not a valid uncompressed TAR payload") from error
    if not members:
        fail("downloaded TAR payload is empty")
    observed: set[str] = set()
    for member in members:
        pure = PurePosixPath(member.name)
        normalized = pure.as_posix()
        if pure.is_absolute() or ".." in pure.parts or not pure.parts:
            fail(f"downloaded TAR contains unsafe path: {member.name}")
        if normalized in observed:
            fail(f"downloaded TAR contains duplicate path: {member.name}")
        observed.add(normalized)
        if member.issym() or member.islnk() or not (member.isdir() or member.isfile()):
            fail(f"downloaded TAR contains unsupported entry: {member.name}")


def write_payload(destination: Path, payload: bytes) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists() or destination.is_symlink():
        fail("raw artifact destination must not already exist")
    with tempfile.NamedTemporaryFile(prefix=destination.name + ".", dir=destination.parent, delete=False) as handle:
        temporary = Path(handle.name)
        handle.write(payload)
        handle.flush()
        os.fchmod(handle.fileno(), 0o600)
    try:
        os.replace(temporary, destination)
    finally:
        temporary.unlink(missing_ok=True)


def validate_common_inputs(repository: str, source_sha: str, expected_digest: str, expected_name: str, api_url: str) -> None:
    if REPOSITORY_RE.fullmatch(repository) is None:
        fail("repository must be owner/name")
    if COMMIT_RE.fullmatch(source_sha) is None:
        fail("source SHA must be exact lowercase 40-hex")
    if DIGEST_RE.fullmatch(expected_digest) is None:
        fail("expected artifact digest must be sha256:<64 lowercase hex>")
    if ARTIFACT_NAME_RE.fullmatch(expected_name) is None or "/" in expected_name or "\\" in expected_name:
        fail("expected raw artifact name must be one canonical .tar filename")
    parsed = urlparse(api_url)
    if parsed.scheme != "https" or not parsed.netloc or parsed.username or parsed.password:
        fail("GitHub API URL must be an absolute HTTPS origin")


def download(
    *,
    repository: str,
    run_id: int,
    artifact_id: int,
    source_sha: str,
    expected_digest: str,
    expected_name: str,
    destination: Path,
    api_url: str,
    token: str,
    transport: Transport | None = None,
    sleep: Callable[[float], None] = time.sleep,
) -> None:
    validate_common_inputs(repository, source_sha, expected_digest, expected_name, api_url)
    transport = transport or UrlLibTransport()
    base = api_url.rstrip("/") + f"/repos/{repository}"
    headers = api_headers(token)

    run = decode_json(
        request_with_retry(
            transport,
            base + f"/actions/runs/{run_id}",
            headers,
            follow_redirects=True,
            success_statuses={200},
            sleep=sleep,
        ),
        "release workflow run metadata",
    )
    validate_run(run, run_id=run_id, source_sha=source_sha)

    artifact = decode_json(
        request_with_retry(
            transport,
            base + f"/actions/artifacts/{artifact_id}",
            headers,
            follow_redirects=True,
            success_statuses={200},
            sleep=sleep,
        ),
        "artifact metadata",
    )
    expected_size = validate_artifact_metadata(
        artifact,
        artifact_id=artifact_id,
        run_id=run_id,
        source_sha=source_sha,
        expected_digest=expected_digest,
        expected_name=expected_name,
    )

    archive_response = request_with_retry(
        transport,
        base + f"/actions/artifacts/{artifact_id}/zip",
        headers,
        follow_redirects=False,
        success_statuses={200, 302, 303, 307},
        sleep=sleep,
    )
    if archive_response.status == 200:
        payload = archive_response.body
    else:
        location = archive_response.headers.get("Location") or archive_response.headers.get("location")
        if not location:
            fail("GitHub artifact download redirect omitted Location")
        parsed = urlparse(location)
        if parsed.scheme != "https" or not parsed.netloc or parsed.username or parsed.password:
            fail("GitHub artifact download redirect is not a safe absolute HTTPS URL")
        payload = request_with_retry(
            transport,
            location,
            {"User-Agent": USER_AGENT},
            follow_redirects=True,
            success_statuses={200},
            sleep=sleep,
        ).body

    if len(payload) != expected_size:
        fail(f"downloaded artifact size differs from GitHub metadata: expected={expected_size} actual={len(payload)}")
    actual_digest = "sha256:" + hashlib.sha256(payload).hexdigest()
    if actual_digest != expected_digest:
        fail("downloaded artifact digest differs from exact GitHub artifact digest")
    validate_raw_tar(payload)
    write_payload(destination, payload)
    print(f"Raw immutable artifact acquisition PASS: id={artifact_id} run={run_id} source={source_sha} digest={actual_digest}")


class FakeTransport:
    def __init__(self, responses: list[HttpResult | BaseException]) -> None:
        self.responses = list(responses)
        self.calls: list[tuple[str, dict[str, str], bool]] = []

    def get(self, url: str, headers: Mapping[str, str], *, follow_redirects: bool) -> HttpResult:
        self.calls.append((url, dict(headers), follow_redirects))
        if not self.responses:
            raise AssertionError("fake transport exhausted")
        value = self.responses.pop(0)
        if isinstance(value, BaseException):
            raise value
        return value


def tar_fixture() -> bytes:
    stream = io.BytesIO()
    with tarfile.open(fileobj=stream, mode="w", format=tarfile.PAX_FORMAT) as archive:
        data = b"fixture\n"
        info = tarfile.TarInfo("release/release-manifest.json")
        info.size = len(data)
        info.mode = 0o644
        info.uid = info.gid = info.mtime = 0
        archive.addfile(info, io.BytesIO(data))
    return stream.getvalue()


def json_result(value: dict[str, object]) -> HttpResult:
    return HttpResult(200, {}, json.dumps(value).encode())


def expect_rejected(label: str, operation: Callable[[], object]) -> None:
    try:
        operation()
    except ArtifactDownloadError:
        return
    raise AssertionError(f"negative raw-artifact fixture unexpectedly passed: {label}")


def self_test() -> None:
    source = "a" * 40
    run_id = 17
    artifact_id = 19
    name = "mailbox-secret-resolver-v1-sha256-" + "b" * 64 + ".tar"
    payload = tar_fixture()
    digest = "sha256:" + hashlib.sha256(payload).hexdigest()
    run = {"id": run_id, "head_branch": "main", "head_sha": source, "status": "completed", "conclusion": "success"}
    artifact = {
        "id": artifact_id,
        "name": name,
        "digest": digest,
        "expired": False,
        "size_in_bytes": len(payload),
        "workflow_run": {"id": run_id, "head_sha": source},
    }
    redirect = HttpResult(302, {"Location": "https://signed.example/artifact"}, b"")
    sleeps: list[float] = []

    def success_transport(*, run_value=run, artifact_value=artifact, archive_value=redirect, payload_value=payload) -> FakeTransport:
        return FakeTransport([json_result(run_value), json_result(artifact_value), archive_value, HttpResult(200, {}, payload_value)])

    with tempfile.TemporaryDirectory(prefix="raw-artifact-self-test-") as temporary:
        destination = Path(temporary) / name
        transport = success_transport()
        download(
            repository="iamaman11/part-crm-emai-profile",
            run_id=run_id,
            artifact_id=artifact_id,
            source_sha=source,
            expected_digest=digest,
            expected_name=name,
            destination=destination,
            api_url="https://api.github.test",
            token="test-token",
            transport=transport,
            sleep=sleeps.append,
        )
        assert destination.read_bytes() == payload
        assert destination.stat().st_mode & 0o777 == 0o600
        assert "Authorization" in transport.calls[2][1]
        assert "Authorization" not in transport.calls[3][1]

    altered_payload = bytearray(payload)
    altered_payload[-1] = (altered_payload[-1] + 1) % 256
    expect_rejected(
        "downloaded payload digest mismatch",
        lambda: download(
            repository="iamaman11/part-crm-emai-profile", run_id=run_id, artifact_id=artifact_id,
            source_sha=source, expected_digest=digest, expected_name=name,
            destination=Path(tempfile.gettempdir()) / "unused.tar", api_url="https://api.github.test",
            token="test-token", transport=success_transport(payload_value=bytes(altered_payload)), sleep=lambda _: None,
        ),
    )
    bad_run = dict(run); bad_run["id"] = run_id + 1
    expect_rejected(
        "wrong release run id",
        lambda: validate_run(bad_run, run_id=run_id, source_sha=source),
    )
    bad_source_run = dict(run); bad_source_run["head_sha"] = "d" * 40
    expect_rejected("wrong source SHA", lambda: validate_run(bad_source_run, run_id=run_id, source_sha=source))
    bad_artifact_id = dict(artifact); bad_artifact_id["id"] = artifact_id + 1
    expect_rejected(
        "wrong artifact id",
        lambda: validate_artifact_metadata(bad_artifact_id, artifact_id=artifact_id, run_id=run_id, source_sha=source, expected_digest=digest, expected_name=name),
    )
    bad_owner = dict(artifact); bad_owner["workflow_run"] = {"id": run_id + 1, "head_sha": source}
    expect_rejected(
        "artifact/run ownership mismatch",
        lambda: validate_artifact_metadata(bad_owner, artifact_id=artifact_id, run_id=run_id, source_sha=source, expected_digest=digest, expected_name=name),
    )
    expect_rejected("malformed non-TAR payload", lambda: validate_raw_tar(b"not-a-tar"))

    retry_sleeps: list[float] = []
    response = request_with_retry(
        FakeTransport([HttpResult(503, {}, b""), HttpResult(200, {}, b"ok")]),
        "https://api.github.test/test", {}, follow_redirects=True, success_statuses={200}, sleep=retry_sleeps.append,
    )
    assert response.body == b"ok" and retry_sleeps == [1.0]
    expect_rejected(
        "persistent 503",
        lambda: request_with_retry(
            FakeTransport([HttpResult(503, {}, b"")] * MAX_ATTEMPTS),
            "https://api.github.test/test", {}, follow_redirects=True, success_statuses={200}, sleep=lambda _: None,
        ),
    )
    retry_sleeps.clear()
    request_with_retry(
        FakeTransport([HttpResult(429, {"Retry-After": "7"}, b""), HttpResult(200, {}, b"ok")]),
        "https://api.github.test/test", {}, follow_redirects=True, success_statuses={200}, sleep=retry_sleeps.append,
    )
    assert retry_sleeps == [7.0]
    for status in (400, 401, 403, 404, 409, 422):
        transport = FakeTransport([HttpResult(status, {}, b"")])
        expect_rejected(
            f"fail-fast HTTP {status}",
            lambda transport=transport: request_with_retry(
                transport, "https://api.github.test/test", {}, follow_redirects=True, success_statuses={200}, sleep=lambda _: None,
            ),
        )
        assert len(transport.calls) == 1
    transport_failure_sleeps: list[float] = []
    request_with_retry(
        FakeTransport([urllib.error.URLError("temporary"), HttpResult(200, {}, b"ok")]),
        "https://api.github.test/test", {}, follow_redirects=True, success_statuses={200}, sleep=transport_failure_sleeps.append,
    )
    assert transport_failure_sleeps == [1.0]
    print("Raw GitHub artifact acquisition positive/negative self-tests passed.")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("self-test")
    acquire = commands.add_parser("download")
    acquire.add_argument("--repository", required=True)
    acquire.add_argument("--run-id", required=True)
    acquire.add_argument("--artifact-id", required=True)
    acquire.add_argument("--source-sha", required=True)
    acquire.add_argument("--expected-digest", required=True)
    acquire.add_argument("--expected-name", required=True)
    acquire.add_argument("--destination", type=Path, required=True)
    acquire.add_argument("--api-url", default=os.environ.get("GITHUB_API_URL", "https://api.github.com"))
    args = parser.parse_args()
    if args.command == "self-test":
        self_test()
        return 0
    download(
        repository=args.repository,
        run_id=parse_positive_int(args.run_id, "release run id"),
        artifact_id=parse_positive_int(args.artifact_id, "artifact id"),
        source_sha=args.source_sha,
        expected_digest=args.expected_digest,
        expected_name=args.expected_name,
        destination=args.destination,
        api_url=args.api_url,
        token=os.environ.get("GITHUB_TOKEN", ""),
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ArtifactDownloadError as error:
        raise SystemExit(f"raw GitHub artifact acquisition rejected: {error}") from error
