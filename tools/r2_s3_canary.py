#!/usr/bin/env python3
"""Exercise an R2 bucket through the S3 API without external dependencies."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import hmac
import json
import os
import secrets
import sys
import urllib.error
import urllib.parse
import urllib.request
import uuid
import xml.etree.ElementTree as ET
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class R2Credentials:
    endpoint: str
    bucket: str
    region: str
    access_key_id: str
    secret_access_key: str

    @classmethod
    def from_file(cls, path: Path) -> "R2Credentials":
        data = json.loads(path.read_text(encoding="utf-8"))
        required = {
            "endpoint",
            "bucket",
            "region",
            "access_key_id",
            "secret_access_key",
        }
        missing = sorted(required - data.keys())
        if missing:
            raise ValueError(f"credential file is missing fields: {', '.join(missing)}")
        return cls(**{field: str(data[field]) for field in required})


class R2S3Client:
    def __init__(self, credentials: R2Credentials) -> None:
        self._credentials = credentials
        self._endpoint = credentials.endpoint.rstrip("/")

    def request(
        self,
        method: str,
        key: str = "",
        *,
        query: dict[str, str] | None = None,
        body: bytes = b"",
    ) -> bytes:
        parsed = urllib.parse.urlsplit(self._endpoint)
        if parsed.scheme != "https" or not parsed.hostname:
            raise ValueError("R2 endpoint must be an HTTPS URL")

        path_segments = [self._credentials.bucket]
        if key:
            path_segments.extend(key.split("/"))
        canonical_uri = "/" + "/".join(
            urllib.parse.quote(segment, safe="-_.~") for segment in path_segments
        )
        canonical_query = urllib.parse.urlencode(
            sorted((query or {}).items()),
            quote_via=urllib.parse.quote,
            safe="-_.~",
        )

        now = dt.datetime.now(dt.UTC)
        amz_date = now.strftime("%Y%m%dT%H%M%SZ")
        date_stamp = now.strftime("%Y%m%d")
        payload_hash = hashlib.sha256(body).hexdigest()
        canonical_headers = (
            f"host:{parsed.netloc}\n"
            f"x-amz-content-sha256:{payload_hash}\n"
            f"x-amz-date:{amz_date}\n"
        )
        signed_headers = "host;x-amz-content-sha256;x-amz-date"
        canonical_request = "\n".join(
            [
                method,
                canonical_uri,
                canonical_query,
                canonical_headers,
                signed_headers,
                payload_hash,
            ]
        )
        credential_scope = f"{date_stamp}/{self._credentials.region}/s3/aws4_request"
        string_to_sign = "\n".join(
            [
                "AWS4-HMAC-SHA256",
                amz_date,
                credential_scope,
                hashlib.sha256(canonical_request.encode()).hexdigest(),
            ]
        )
        signature = hmac.new(
            self._signing_key(date_stamp),
            string_to_sign.encode(),
            hashlib.sha256,
        ).hexdigest()
        authorization = (
            "AWS4-HMAC-SHA256 "
            f"Credential={self._credentials.access_key_id}/{credential_scope}, "
            f"SignedHeaders={signed_headers}, Signature={signature}"
        )

        url = f"{self._endpoint}{canonical_uri}"
        if canonical_query:
            url += f"?{canonical_query}"
        request = urllib.request.Request(
            url,
            data=body if method in {"PUT", "POST"} else None,
            method=method,
            headers={
                "Authorization": authorization,
                "Host": parsed.netloc,
                "X-Amz-Content-Sha256": payload_hash,
                "X-Amz-Date": amz_date,
            },
        )
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                return response.read()
        except urllib.error.HTTPError as error:
            detail = _safe_error_detail(error.read())
            raise RuntimeError(f"R2 S3 {method} failed with HTTP {error.code}: {detail}") from error

    def _signing_key(self, date_stamp: str) -> bytes:
        secret = f"AWS4{self._credentials.secret_access_key}".encode()
        date_key = hmac.new(secret, date_stamp.encode(), hashlib.sha256).digest()
        region_key = hmac.new(
            date_key,
            self._credentials.region.encode(),
            hashlib.sha256,
        ).digest()
        service_key = hmac.new(region_key, b"s3", hashlib.sha256).digest()
        return hmac.new(service_key, b"aws4_request", hashlib.sha256).digest()


def _safe_error_detail(payload: bytes) -> str:
    try:
        root = ET.fromstring(payload)
        code = root.findtext("Code", default="Unknown")
        message = root.findtext("Message", default="request rejected")
        return f"{code}: {message}"
    except ET.ParseError:
        return "request rejected"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Verify R2 PUT/LIST/GET/DELETE with an ephemeral canary object."
    )
    parser.add_argument(
        "--credentials-file",
        type=Path,
        default=Path(value) if (value := os.environ.get("R2_CREDENTIALS_FILE")) else None,
        help="JSON credential file; defaults to R2_CREDENTIALS_FILE.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.credentials_file is None:
        print("--credentials-file or R2_CREDENTIALS_FILE is required", file=sys.stderr)
        return 2

    credentials = R2Credentials.from_file(args.credentials_file)
    client = R2S3Client(credentials)
    run_id = uuid.uuid4().hex
    key = f"canary/{run_id}.bin"
    protected_key = f"canary/{run_id}.keep"
    payload = secrets.token_bytes(4096)
    protected_payload = secrets.token_bytes(4096)
    digest = hashlib.sha256(payload).hexdigest()

    def exists_exact(object_key: str) -> bool:
        try:
            client.request("HEAD", object_key)
            return True
        except RuntimeError as error:
            if str(error).startswith("R2 S3 HEAD failed with HTTP 404:"):
                return False
            raise

    try:
        # The only objects ever mutated have this invocation's random canary
        # prefix; there is no production generation key or list-based delete.
        client.request("PUT", key, body=payload)
        client.request("PUT", protected_key, body=protected_payload)
        listing = client.request(
            "GET",
            query={"list-type": "2", "prefix": f"canary/{run_id}"},
        )
        listed = {
            element.text
            for element in ET.fromstring(listing).iter()
            if element.tag.rsplit("}", 1)[-1] == "Key"
        }
        restored = client.request("GET", key)
        if (
            listed != {key, protected_key}
            or hashlib.sha256(restored).hexdigest() != digest
            or not exists_exact(key)
            or not exists_exact(protected_key)
        ):
            raise RuntimeError("R2 exact HEAD/GET canary verification failed")

        client.request("DELETE", key)
        if exists_exact(key):
            raise RuntimeError("R2 canary still exists after exact DELETE")
        # Repeat DELETE exactly as after a lost successful response. It must
        # leave the adjacent, independently verified object untouched.
        client.request("DELETE", key)
        if (
            exists_exact(key)
            or not exists_exact(protected_key)
            or client.request("GET", protected_key) != protected_payload
        ):
            raise RuntimeError("R2 retry affected the wrong canary object")

        client.request("DELETE", protected_key)
        if exists_exact(protected_key):
            raise RuntimeError("R2 protected canary cleanup failed")
        print(
            json.dumps(
                {
                    "success": True,
                    "bucket": credentials.bucket,
                    "bytes": len(payload),
                    "sha256_verified": True,
                    "list_verified": True,
                    "head_verified": True,
                    "deleted": True,
                    "delete_retry_verified": True,
                    "other_object_preserved": True,
                },
                separators=(",", ":"),
            )
        )
        return 0
    finally:
        # Cleanup is limited to the two freshly generated disposable keys,
        # including on verification failure. Never delete by prefix.
        for disposable in (key, protected_key):
            try:
                client.request("DELETE", disposable)
            except Exception:
                pass


if __name__ == "__main__":
    raise SystemExit(main())
