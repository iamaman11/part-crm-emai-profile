#!/usr/bin/env python3
"""Canonical D3 compatibility entrypoint after Architecture Re-baseline v3 AR-2.

The accepted pre-AR-2 promotion implementation is preserved byte-for-byte in
`_mailbox_secret_resolver_promotion_core.py`. This module keeps the accepted D3
verifier surface available through the canonical path, while adding narrow
interlocks owned by the promotion boundary:

- the superseded legacy production lane is unavailable until PC-1 is separately
  authorized after AR-17 under the AR-11 release-set model;
- immutable release artifacts uploaded as raw TAR payloads are acquired through
  GitHub REST with bounded transient retry and digest-before-TAR verification.
"""

from __future__ import annotations

import argparse
import email.utils
import hashlib
import io
import json
import os
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.request
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from datetime import timezone
from pathlib import Path, PurePosixPath
from typing import Protocol
from urllib.parse import urlparse

import _mailbox_secret_resolver_promotion_core as core

AR2_LEGACY_PRODUCTION_DISABLED = True
AR2_PRODUCTION_AUTHORITY = "PC-1_AFTER_AR-17_USING_AR-11_RELEASE_SET"
ENVIRONMENT_GATED_COMMANDS = frozenset({"github-preflight", "prepare", "attest"})
RAW_ARTIFACT_COMMAND = "download-raw-artifact"
RAW_ARTIFACT_MAX_ATTEMPTS = 5
RAW_ARTIFACT_BASE_BACKOFF_SECONDS = 1.0
RAW_ARTIFACT_MAX_BACKOFF_SECONDS = 8.0
RAW_ARTIFACT_MAX_RETRY_AFTER_SECONDS = 60.0
RAW_ARTIFACT_MAX_BYTES = 512 * 1024 * 1024
RAW_ARTIFACT_USER_AGENT = "part-crm-d3-raw-artifact-acquisition"
RAW_ARTIFACT_API_VERSION = "2022-11-28"

# Preserve the accepted D3 verifier surface at the canonical module path. These
# are aliases to the byte-for-byte accepted implementation, not replacement
# implementations. Permanent D3 checks intentionally verify that these safety
# primitives and their policy messages remain reachable from this entrypoint.
require_mode_0600 = core.require_mode_0600
render_resolver_config = core.render_resolver_config
render_control_config = core.render_control_config
validate_release_identities = core.validate_release_identities
validate_staging_evidence_artifact = core.validate_staging_evidence_artifact
validate_deployment_closures = core.validate_deployment_closures

ACCEPTED_D3_POLICY_MESSAGES = (
    "cross-environment-identical secret documents are forbidden",
    "caller-auth secret must match both Workers",
    "Production same-bits artifacts match immutable passed staging evidence",
)


def requested_environment(argv: Sequence[str]) -> str | None:
    if len(argv) < 2 or argv[1] not in ENVIRONMENT_GATED_COMMANDS:
        return None
    try:
        index = argv.index("--environment", 2)
    except ValueError:
        return None
    value_index = index + 1
    return argv[value_index] if value_index < len(argv) else None


def enforce_ar2_environment_gate(argv: Sequence[str]) -> None:
    environment = requested_environment(argv)
    if environment == "production":
        raise core.PromotionError(
            "legacy D3 production promotion is disabled by Architecture Re-baseline v3 AR-2; "
            "production mutation remains forbidden through AR-17 and future production promotion "
            f"requires {AR2_PRODUCTION_AUTHORITY}"
        )


def self_test_gate() -> None:
    staging = ["promotion", "github-preflight", "--environment", "staging"]
    enforce_ar2_environment_gate(staging)
    production = ["promotion", "github-preflight", "--environment", "production"]
    try:
        enforce_ar2_environment_gate(production)
    except core.PromotionError:
        return
    raise core.PromotionError("AR-2 legacy production negative fixture unexpectedly passed")


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
                body = response.read(RAW_ARTIFACT_MAX_BYTES + 1)
                if len(body) > RAW_ARTIFACT_MAX_BYTES:
                    core.fail("GitHub artifact response exceeded the bounded body limit")
                return HttpResult(response.status, dict(response.headers.items()), body)
        except urllib.error.HTTPError as error:
            body = error.read() if error.fp is not None else b""
            return HttpResult(error.code, dict(error.headers.items()) if error.headers else {}, body)


def raw_api_headers(token: str) -> dict[str, str]:
    if not token:
        core.fail("GITHUB_TOKEN is required for raw artifact acquisition")
    return {
        "Accept": "application/vnd.github+json",
        "Authorization": f"Bearer {token}",
        "User-Agent": RAW_ARTIFACT_USER_AGENT,
        "X-GitHub-Api-Version": RAW_ARTIFACT_API_VERSION,
    }


def retry_after_seconds(value: str | None, *, now: Callable[[], float] = time.time) -> float | None:
    if not value:
        return None
    stripped = value.strip()
    if stripped.isdigit():
        return min(float(stripped), RAW_ARTIFACT_MAX_RETRY_AFTER_SECONDS)
    try:
        parsed = email.utils.parsedate_to_datetime(stripped)
    except (TypeError, ValueError):
        return None
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return min(max(0.0, parsed.timestamp() - now()), RAW_ARTIFACT_MAX_RETRY_AFTER_SECONDS)


def retry_delay(attempt: int, headers: Mapping[str, str]) -> float:
    retry_after = retry_after_seconds(headers.get("Retry-After") or headers.get("retry-after"))
    if retry_after is not None:
        return retry_after
    return min(
        RAW_ARTIFACT_BASE_BACKOFF_SECONDS * (2 ** (attempt - 1)),
        RAW_ARTIFACT_MAX_BACKOFF_SECONDS,
    )


def request_with_retry(
    transport: Transport,
    url: str,
    headers: Mapping[str, str],
    *,
    follow_redirects: bool,
    success_statuses: set[int],
    sleep: Callable[[float], None] = time.sleep,
    max_attempts: int = RAW_ARTIFACT_MAX_ATTEMPTS,
) -> HttpResult:
    if max_attempts < 1:
        core.fail("raw artifact retry attempts must be positive")
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
                core.fail(
                    f"transient GitHub artifact request exhausted after {max_attempts} attempts: "
                    f"HTTP {result.status}"
                )
            sleep(retry_delay(attempt, result.headers))
            continue
        core.fail(f"GitHub artifact request failed closed without retry: HTTP {result.status}")
    core.fail(
        f"GitHub artifact request exhausted after {max_attempts} transport failures: "
        f"{type(last_error).__name__}"
    )


def decode_json(result: HttpResult, label: str) -> dict[str, object]:
    try:
        value = json.loads(result.body.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise core.PromotionError(f"{label} is not strict UTF-8 JSON") from error
    if not isinstance(value, dict):
        core.fail(f"{label} must be a JSON object")
    return value


def validate_raw_artifact_metadata(
    artifact: dict[str, object],
    *,
    artifact_id: int,
    run_id: int,
    source_sha: str,
    expected_digest: str,
    expected_name: str,
) -> int:
    if artifact.get("id") != artifact_id:
        core.fail("artifact metadata id differs from the requested immutable artifact")
    if artifact.get("name") != expected_name:
        core.fail("artifact name differs from the expected raw release TAR name")
    if core.ARTIFACT_DIGEST_RE.fullmatch(expected_digest) is None or artifact.get("digest") != expected_digest:
        core.fail("artifact metadata digest differs from the supplied immutable digest")
    if artifact.get("expired") is not False:
        core.fail("artifact is expired or retention state is unknown")
    size = artifact.get("size_in_bytes")
    if not isinstance(size, int) or size <= 0 or size > RAW_ARTIFACT_MAX_BYTES:
        core.fail("artifact size is outside the bounded raw-artifact limit")
    workflow_run = artifact.get("workflow_run")
    if not isinstance(workflow_run, dict):
        core.fail("artifact workflow ownership metadata is missing")
    if workflow_run.get("id") != run_id or workflow_run.get("head_sha") != source_sha:
        core.fail("artifact is not owned by the exact supplied release run/source SHA")
    return size


def validate_raw_tar(payload: bytes) -> None:
    if not payload:
        core.fail("downloaded raw artifact is empty")
    try:
        with tarfile.open(fileobj=io.BytesIO(payload), mode="r:") as archive:
            members = archive.getmembers()
    except tarfile.TarError as error:
        raise core.PromotionError("downloaded artifact is not a valid uncompressed TAR payload") from error
    if not members:
        core.fail("downloaded TAR payload is empty")
    observed: set[str] = set()
    for member in members:
        pure = PurePosixPath(member.name)
        normalized = pure.as_posix()
        if pure.is_absolute() or ".." in pure.parts or not pure.parts:
            core.fail(f"downloaded TAR contains unsafe path: {member.name}")
        if normalized in observed:
            core.fail(f"downloaded TAR contains duplicate path: {member.name}")
        observed.add(normalized)
        if member.issym() or member.islnk() or not (member.isdir() or member.isfile()):
            core.fail(f"downloaded TAR contains unsupported entry: {member.name}")


def write_raw_payload(destination: Path, payload: bytes) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists() or destination.is_symlink():
        core.fail("raw artifact destination must not already exist")
    with tempfile.NamedTemporaryFile(
        prefix=destination.name + ".", dir=destination.parent, delete=False
    ) as handle:
        temporary = Path(handle.name)
        handle.write(payload)
        handle.flush()
        os.fchmod(handle.fileno(), 0o600)
    try:
        os.replace(temporary, destination)
    finally:
        temporary.unlink(missing_ok=True)


def validate_raw_inputs(
    repository: str, source_sha: str, expected_digest: str, expected_name: str, api_url: str
) -> None:
    if repository != core.CANONICAL_REPOSITORY:
        core.fail(f"raw artifact repository must be {core.CANONICAL_REPOSITORY}")
    if core.COMMIT_RE.fullmatch(source_sha) is None:
        core.fail("raw artifact source SHA must be exact lowercase 40-hex")
    if core.ARTIFACT_DIGEST_RE.fullmatch(expected_digest) is None:
        core.fail("expected artifact digest must be sha256:<64 lowercase hex>")
    if "/" in expected_name or "\\" in expected_name or not expected_name.endswith(".tar"):
        core.fail("expected raw artifact name must be one canonical .tar filename")
    parsed = urlparse(api_url)
    if parsed.scheme != "https" or not parsed.netloc or parsed.username or parsed.password:
        core.fail("GitHub API URL must be an absolute HTTPS origin")


def download_raw_artifact(
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
    validate_raw_inputs(repository, source_sha, expected_digest, expected_name, api_url)
    transport = transport or UrlLibTransport()
    base = api_url.rstrip("/") + f"/repos/{repository}"
    headers = raw_api_headers(token)
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
    expected_size = validate_raw_artifact_metadata(
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
            core.fail("GitHub artifact download redirect omitted Location")
        parsed = urlparse(location)
        if parsed.scheme != "https" or not parsed.netloc or parsed.username or parsed.password:
            core.fail("GitHub artifact download redirect is not a safe absolute HTTPS URL")
        payload = request_with_retry(
            transport,
            location,
            {"User-Agent": RAW_ARTIFACT_USER_AGENT},
            follow_redirects=True,
            success_statuses={200},
            sleep=sleep,
        ).body
    if len(payload) != expected_size:
        core.fail(
            "downloaded artifact size differs from GitHub metadata: "
            f"expected={expected_size} actual={len(payload)}"
        )
    actual_digest = "sha256:" + hashlib.sha256(payload).hexdigest()
    if actual_digest != expected_digest:
        core.fail("downloaded artifact digest differs from exact GitHub artifact digest")
    validate_raw_tar(payload)
    write_raw_payload(destination, payload)
    print(
        "Raw immutable artifact acquisition PASS: "
        f"id={artifact_id} run={run_id} source={source_sha} digest={actual_digest}"
    )


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
    except core.PromotionError:
        return
    raise core.PromotionError(f"negative raw-artifact fixture unexpectedly passed: {label}")


def raw_artifact_self_test() -> None:
    source = "a" * 40
    run_id = 17
    artifact_id = 19
    name = "mailbox-secret-resolver-v1-sha256-" + "b" * 64 + ".tar"
    payload = tar_fixture()
    digest = "sha256:" + hashlib.sha256(payload).hexdigest()
    artifact = {
        "id": artifact_id,
        "name": name,
        "digest": digest,
        "expired": False,
        "size_in_bytes": len(payload),
        "workflow_run": {"id": run_id, "head_sha": source},
    }
    redirect = HttpResult(302, {"Location": "https://signed.example/artifact"}, b"")

    def success_transport(
        *, artifact_value: dict[str, object] = artifact, payload_value: bytes = payload
    ) -> FakeTransport:
        return FakeTransport(
            [json_result(artifact_value), redirect, HttpResult(200, {}, payload_value)]
        )

    with tempfile.TemporaryDirectory(prefix="raw-artifact-self-test-") as temporary:
        destination = Path(temporary) / name
        transport = success_transport()
        download_raw_artifact(
            repository=core.CANONICAL_REPOSITORY,
            run_id=run_id,
            artifact_id=artifact_id,
            source_sha=source,
            expected_digest=digest,
            expected_name=name,
            destination=destination,
            api_url="https://api.github.test",
            token="test-token",
            transport=transport,
            sleep=lambda _: None,
        )
        if destination.read_bytes() != payload or destination.stat().st_mode & 0o777 != 0o600:
            core.fail("raw TAR success fixture did not preserve exact bytes/mode")
        if "Authorization" not in transport.calls[1][1] or "Authorization" in transport.calls[2][1]:
            core.fail("GitHub token redirect-boundary fixture failed")

    altered = bytearray(payload)
    altered[-1] = (altered[-1] + 1) % 256
    expect_rejected(
        "downloaded payload digest mismatch",
        lambda: download_raw_artifact(
            repository=core.CANONICAL_REPOSITORY,
            run_id=run_id,
            artifact_id=artifact_id,
            source_sha=source,
            expected_digest=digest,
            expected_name=name,
            destination=Path(tempfile.gettempdir()) / "unused-raw-artifact.tar",
            api_url="https://api.github.test",
            token="test-token",
            transport=success_transport(payload_value=bytes(altered)),
            sleep=lambda _: None,
        ),
    )
    wrong_id = dict(artifact)
    wrong_id["id"] = artifact_id + 1
    expect_rejected(
        "wrong artifact id",
        lambda: validate_raw_artifact_metadata(
            wrong_id,
            artifact_id=artifact_id,
            run_id=run_id,
            source_sha=source,
            expected_digest=digest,
            expected_name=name,
        ),
    )
    wrong_run = dict(artifact)
    wrong_run["workflow_run"] = {"id": run_id + 1, "head_sha": source}
    expect_rejected(
        "wrong release run id",
        lambda: validate_raw_artifact_metadata(
            wrong_run,
            artifact_id=artifact_id,
            run_id=run_id,
            source_sha=source,
            expected_digest=digest,
            expected_name=name,
        ),
    )
    wrong_source = dict(artifact)
    wrong_source["workflow_run"] = {"id": run_id, "head_sha": "d" * 40}
    expect_rejected(
        "wrong source SHA",
        lambda: validate_raw_artifact_metadata(
            wrong_source,
            artifact_id=artifact_id,
            run_id=run_id,
            source_sha=source,
            expected_digest=digest,
            expected_name=name,
        ),
    )
    expect_rejected("malformed non-TAR payload", lambda: validate_raw_tar(b"not-a-tar"))

    sleeps: list[float] = []
    response = request_with_retry(
        FakeTransport([HttpResult(503, {}, b""), HttpResult(200, {}, b"ok")]),
        "https://api.github.test/test",
        {},
        follow_redirects=True,
        success_statuses={200},
        sleep=sleeps.append,
    )
    if response.body != b"ok" or sleeps != [1.0]:
        core.fail("503 then success bounded-retry fixture failed")
    expect_rejected(
        "persistent 503",
        lambda: request_with_retry(
            FakeTransport([HttpResult(503, {}, b"")] * RAW_ARTIFACT_MAX_ATTEMPTS),
            "https://api.github.test/test",
            {},
            follow_redirects=True,
            success_statuses={200},
            sleep=lambda _: None,
        ),
    )
    sleeps.clear()
    request_with_retry(
        FakeTransport(
            [HttpResult(429, {"Retry-After": "7"}, b""), HttpResult(200, {}, b"ok")]
        ),
        "https://api.github.test/test",
        {},
        follow_redirects=True,
        success_statuses={200},
        sleep=sleeps.append,
    )
    if sleeps != [7.0]:
        core.fail("429 Retry-After fixture failed")
    for status in (400, 401, 403, 404, 409, 422):
        transport = FakeTransport([HttpResult(status, {}, b"")])
        expect_rejected(
            f"fail-fast HTTP {status}",
            lambda transport=transport: request_with_retry(
                transport,
                "https://api.github.test/test",
                {},
                follow_redirects=True,
                success_statuses={200},
                sleep=lambda _: None,
            ),
        )
        if len(transport.calls) != 1:
            core.fail(f"HTTP {status} fail-fast fixture retried unexpectedly")
    transport_sleeps: list[float] = []
    request_with_retry(
        FakeTransport([urllib.error.URLError("temporary"), HttpResult(200, {}, b"ok")]),
        "https://api.github.test/test",
        {},
        follow_redirects=True,
        success_statuses={200},
        sleep=transport_sleeps.append,
    )
    if transport_sleeps != [1.0]:
        core.fail("transport bounded-retry fixture failed")
    print("Raw GitHub artifact acquisition positive/negative self-tests passed.")


def raw_artifact_main(argv: Sequence[str]) -> int:
    parser = argparse.ArgumentParser(description="Acquire one immutable raw GitHub Actions TAR")
    parser.add_argument("--repository", required=True)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--artifact-id", required=True)
    parser.add_argument("--source-sha", required=True)
    parser.add_argument("--expected-digest", required=True)
    parser.add_argument("--expected-name", required=True)
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument("--api-url", default=os.environ.get("GITHUB_API_URL", "https://api.github.com"))
    args = parser.parse_args(list(argv))
    download_raw_artifact(
        repository=args.repository,
        run_id=core.parse_positive_int(args.run_id, "release run id"),
        artifact_id=core.parse_positive_int(args.artifact_id, "artifact id"),
        source_sha=args.source_sha,
        expected_digest=args.expected_digest,
        expected_name=args.expected_name,
        destination=args.destination,
        api_url=args.api_url,
        token=os.environ.get("GITHUB_TOKEN", ""),
    )
    return 0


def main() -> int:
    if len(sys.argv) >= 2 and sys.argv[1] == RAW_ARTIFACT_COMMAND:
        return raw_artifact_main(sys.argv[2:])
    if len(sys.argv) >= 2 and sys.argv[1] == "self-test":
        self_test_gate()
        raw_artifact_self_test()
    enforce_ar2_environment_gate(sys.argv)
    return core.main()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except core.PromotionError as error:
        raise SystemExit(f"mailbox resolver promotion rejected: {error}") from error
