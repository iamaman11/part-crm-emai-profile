#!/usr/bin/env python3
"""Observe current Worker release identity from its existing version-bound runtime configuration.

Explicit --observe-version permits scoped read-only Cloudflare metadata GETs and captures only
public identity bindings in the caller-owned deployment observation. Offline consumers use that
same capture. This adapter never authorizes or mutates a release and never reads secret values.
"""
from __future__ import annotations

import argparse
import copy
import json
import os
import re
import sys
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any, Callable

RELEASE_ID = re.compile(r"release-set-v3-sha256-[0-9a-f]{64}")
PROFILE_ID = re.compile(r"[a-z0-9][a-z0-9-]*")
SHA256 = re.compile(r"[0-9a-f]{64}")
VERSION_ID = re.compile(r"[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}")
ANNOTATION = re.compile(r"release_set=([^\s]+)\s+profile=([^\s]+)")
IDENTITY_BINDINGS = {
    "TARGET_AUTHORIZATION_OBSERVATION", "CANONICAL_ENVIRONMENT",
    "CAPABILITY_PROFILE_ID", "CAPABILITY_PROFILE_DIGEST",
}
MAX_JSON_BYTES = 1024 * 1024
TOKEN_ENVS = {"CLOUDFLARE_API_TOKEN", "CLOUDFLARE_OBSERVE_API_TOKEN"}


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req: Any, fp: Any, code: int, msg: str, headers: Any, newurl: str) -> None:
        return None


def unique_members(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate observation member")
        result[key] = value
    return result


def decode(raw: bytes) -> Any:
    if len(raw) > MAX_JSON_BYTES:
        raise ValueError("observation exceeds byte bound")
    try:
        def reject_constant(_: str) -> None:
            raise ValueError("non-finite observation number")
        value = json.loads(raw, object_pairs_hook=unique_members, parse_constant=reject_constant)
        pending = [(value, 0)]
        count = 0
        while pending:
            item, depth = pending.pop()
            count += 1
            if depth > 32 or count > 20000:
                raise ValueError("observation exceeds structural bound")
            children = item.values() if isinstance(item, dict) else item if isinstance(item, list) else ()
            pending.extend((child, depth + 1) for child in children)
        return value
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError) as error:
        raise ValueError("observation JSON is invalid") from error


def active_version(deployment: dict[str, Any]) -> str:
    versions = deployment.get("versions")
    if (not isinstance(deployment.get("id"), str) or not deployment["id"]
        or not isinstance(versions, list) or len(versions) != 1
        or not isinstance(versions[0], dict) or versions[0].get("percentage") != 100):
        raise ValueError("one fully active deployment version is required")
    version_id = versions[0].get("version_id")
    if not isinstance(version_id, str) or VERSION_ID.fullmatch(version_id) is None:
        raise ValueError("current version id is invalid")
    return version_id


def extract_identity(value: Any) -> tuple[str | None, str | None]:
    if not isinstance(value, dict):
        raise ValueError("deployment observation must be one object")
    if not value:
        return None, None
    version_id = active_version(value)
    version = value.get("current_version")
    if not isinstance(version, dict) or version.get("id") != version_id:
        raise ValueError("current version metadata is missing or mismatched")
    resources = version.get("resources")
    if not isinstance(resources, dict):
        raise ValueError("current version resources are missing")
    script = resources.get("script")
    if not isinstance(script, dict) or not isinstance(script.get("etag"), str) or SHA256.fullmatch(script["etag"]) is None:
        raise ValueError("current script identity is invalid")
    bindings = resources.get("bindings")
    if not isinstance(bindings, list):
        raise ValueError("current identity bindings are missing")
    public: dict[str, str] = {}
    for binding in bindings:
        if not isinstance(binding, dict) or not isinstance(binding.get("name"), str) or binding["name"] not in IDENTITY_BINDINGS:
            continue
        name = binding["name"]
        if name in public or binding.get("type") != "plain_text" or not isinstance(binding.get("text"), str):
            raise ValueError("current public identity binding is invalid or duplicated")
        public[name] = binding["text"]
    if set(public) != IDENTITY_BINDINGS:
        raise ValueError("current public identity binding is missing")
    fields = public["TARGET_AUTHORIZATION_OBSERVATION"].split("|")
    if (len(fields) != 5 or fields[0] != "target-v1"
        or fields[1] != public["CANONICAL_ENVIRONMENT"] or fields[1] not in {"staging", "production"}
        or fields[2] != public["CAPABILITY_PROFILE_ID"] or PROFILE_ID.fullmatch(fields[2]) is None
        or fields[3] != public["CAPABILITY_PROFILE_DIGEST"] or SHA256.fullmatch(fields[3]) is None
        or RELEASE_ID.fullmatch(fields[4]) is None):
        raise ValueError("current runtime target observation is inconsistent or unsupported")
    identity = (fields[4], fields[2])
    # A message is provenance, never a substitute for missing current runtime configuration.
    for metadata in (value, version):
        annotations = metadata.get("annotations", {})
        if not isinstance(annotations, dict):
            raise ValueError("deployment annotations are invalid")
        message = annotations.get("workers/message", "")
        if isinstance(message, str) and "release_set=" in message:
            match = ANNOTATION.fullmatch(message.strip())
            if match is None or match.groups() != identity:
                raise ValueError("release annotation conflicts with current runtime identity")
    return identity


def api_get(path: str, token: str) -> Any:
    request = urllib.request.Request(
        "https://api.cloudflare.com/client/v4" + path,
        headers={"Authorization": "Bearer " + token}, method="GET",
    )
    try:
        with urllib.request.build_opener(NoRedirect).open(request, timeout=20) as response:
            document = decode(response.read(MAX_JSON_BYTES + 1))
    except urllib.error.HTTPError as error:
        raise ValueError(f"current metadata GET failed HTTP {error.code}") from error
    except (OSError, ValueError) as error:
        raise ValueError("current metadata GET failed") from error
    if not isinstance(document, dict) or document.get("success") is not True or document.get("errors") != [] or "result" not in document:
        raise ValueError("current metadata API response is unsuccessful")
    return document["result"]


def capture_current_version(
    deployment: Any, account_id: str, worker_name: str, get: Callable[[str], Any],
) -> dict[str, Any]:
    if not isinstance(deployment, dict):
        raise ValueError("deployment observation must be one object")
    if not deployment:
        return deployment
    version_id = active_version(deployment)
    base = f"/accounts/{account_id}/workers/scripts/{worker_name}"
    version = get(base + "/versions/" + version_id)
    if not isinstance(version, dict) or version.get("id") != version_id:
        raise ValueError("observed current version id mismatch")
    resources = version.get("resources")
    if not isinstance(resources, dict) or not isinstance(resources.get("bindings"), list):
        raise ValueError("observed current version resources missing")
    # Do not persist other plain vars (including owner email), secret bindings or raw API data.
    bindings = []
    for binding in resources["bindings"]:
        if isinstance(binding, dict) and isinstance(binding.get("name"), str) and binding["name"] in IDENTITY_BINDINGS:
            saved = {"name": binding["name"], "type": binding.get("type")}
            if binding.get("type") == "plain_text":
                saved["text"] = binding.get("text")
            bindings.append(saved)
    observed = copy.deepcopy(deployment)
    annotations = version.get("annotations", {})
    if isinstance(annotations, dict):
        # Version notes can contain arbitrary operator text. Retain only a public release marker.
        message = annotations.get("workers/message")
        if isinstance(message, str) and "release_set=" in message:
            match = ANNOTATION.fullmatch(message.strip())
            valid = match and RELEASE_ID.fullmatch(match[1]) and PROFILE_ID.fullmatch(match[2])
            annotations = {"workers/message": message if valid else "release_set=[invalid]"}
        else:
            annotations = {}
    script = resources.get("script")
    observed["current_version"] = {
        "id": version_id, "annotations": annotations,
        "resources": {"script": {"etag": script.get("etag") if isinstance(script, dict) else None}, "bindings": bindings},
    }
    after = get(base + "/deployments")
    latest = after.get("deployments") if isinstance(after, dict) else None
    if (not isinstance(latest, list) or not latest or not isinstance(latest[0], dict)
        or latest[0].get("id") != deployment["id"]
        or latest[0].get("versions") != deployment["versions"]):
        raise ValueError("current deployment changed during version observation")
    return observed


def self_test() -> None:
    release = "release-set-v3-sha256-" + "b" * 64
    profile, digest = "rehearsal-core-v2", "c" * 64
    version_id = "00000000-0000-4000-8000-000000000001"
    public = {"TARGET_AUTHORIZATION_OBSERVATION": f"target-v1|staging|{profile}|{digest}|{release}",
              "CANONICAL_ENVIRONMENT": "staging", "CAPABILITY_PROFILE_ID": profile,
              "CAPABILITY_PROFILE_DIGEST": digest}
    deployment = {"id": "current-deployment", "versions": [{"version_id": version_id, "percentage": 100}],
                  "annotations": {"workers/triggered_by": "secret"}}
    version = {"id": version_id, "resources": {"script": {"etag": "d" * 64}, "bindings": [
        {"name": name, "type": "plain_text", "text": text} for name, text in public.items()
    ] + [{"name": "PRIVATE_CONFIG", "type": "plain_text", "text": "must-not-persist"},
         {"name": "SECRET", "type": "secret_text", "text": "must-not-persist"}]}}
    get = lambda path: version if "/versions/" in path else {"deployments": [deployment]}
    captured = capture_current_version(deployment, "a" * 32, "worker", get)
    assert extract_identity(captured) == (release, profile)
    assert "must-not-persist" not in json.dumps(captured)
    assert extract_identity({}) == (None, None)
    assert capture_current_version({}, "a" * 32, "worker", lambda _: (_ for _ in ()).throw(AssertionError("unexpected GET"))) == {}
    annotated = copy.deepcopy(captured)
    annotated["annotations"]["workers/message"] = f"release_set={release} profile={profile}"
    assert extract_identity(annotated) == (release, profile)
    invalid = [deployment, {"message": f"release_set={release} profile={profile}"}]
    for mutation in ("version", "traffic", "split", "missing", "duplicate", "type", "digest", "profile", "environment", "schema", "release", "annotation", "etag"):
        value = copy.deepcopy(captured)
        bindings = value["current_version"]["resources"]["bindings"]
        if mutation == "version": value["current_version"]["id"] = "other"
        elif mutation == "traffic": value["versions"][0]["percentage"] = 50
        elif mutation == "split": value["versions"].append(value["versions"][0].copy())
        elif mutation == "missing": bindings.pop()
        elif mutation == "duplicate": bindings.append(bindings[0].copy())
        elif mutation == "type": bindings[0]["type"] = "secret_text"
        elif mutation == "digest": bindings[-1]["text"] = "e" * 64
        elif mutation == "profile": bindings[-2]["text"] = "other-profile"
        elif mutation == "environment": bindings[1]["text"] = "production"
        elif mutation == "schema": bindings[0]["text"] = bindings[0]["text"].replace("target-v1", "target-v2")
        elif mutation == "release": bindings[0]["text"] = bindings[0]["text"].replace("release-set-v3", "release-set-v4")
        elif mutation == "annotation": value["annotations"]["workers/message"] = f"release_set={'release-set-v3-sha256-' + 'e' * 64} profile={profile}"
        elif mutation == "etag": value["current_version"]["resources"]["script"]["etag"] = "bad"
        invalid.append(value)
    for value in invalid:
        try: extract_identity(value)
        except ValueError: pass
        else: raise AssertionError("invalid live identity accepted")
    for raw in (b'{"id":1,"id":2}', b'NaN', b'[' * 40 + b'0' + b']' * 40, b' ' * (MAX_JSON_BYTES + 1)):
        try: decode(raw)
        except ValueError: pass
        else: raise AssertionError("invalid JSON bound accepted")
    for result in ({"id": "wrong"}, version):
        def drift(path: str) -> Any:
            return result if "/versions/" in path else {"deployments": [{**deployment, "id": "changed"}]}
        try: capture_current_version(deployment, "a" * 32, "worker", drift)
        except ValueError: pass
        else: raise AssertionError("version/deployment drift accepted")
    from unittest.mock import patch
    for status in (401, 403, 404, 500):
        error = urllib.error.HTTPError("https://api.cloudflare.com", status, "must-not-log", None, None)
        with patch.object(urllib.request.OpenerDirector, "open", side_effect=error):
            try: api_get("/accounts/metadata", "must-not-log")
            except ValueError as failure:
                assert str(status) in str(failure) and "must-not-log" not in str(failure)
            else: raise AssertionError("metadata API failure accepted")
    print("AR-11 current-version identity adapter self-test passed.")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--status", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--observe-version", action="store_true")
    parser.add_argument("--account-id")
    parser.add_argument("--worker-name")
    parser.add_argument("--token-env", choices=sorted(TOKEN_ENVS))
    args = parser.parse_args()
    try:
        if args.self_test:
            self_test()
            return 0
        if args.status is None or args.output is None:
            raise ValueError("status and output are required")
        if (args.status.is_symlink() or not args.status.is_file() or args.status.stat().st_size > MAX_JSON_BYTES
            or args.output.is_symlink() or args.output.exists()):
            raise ValueError("observation paths are invalid or output already exists")
        value = decode(args.status.read_bytes())
        if args.observe_version:
            if (not isinstance(args.account_id, str) or re.fullmatch(r"[0-9a-f]{32}", args.account_id) is None
                or not isinstance(args.worker_name, str) or re.fullmatch(r"[A-Za-z0-9_-]{1,128}", args.worker_name) is None
                or args.token_env not in TOKEN_ENVS):
                raise ValueError("exact read-only version observation scope is required")
            if value:
                token = os.environ.get(args.token_env, "")
                if not token or any(char.isspace() for char in token):
                    raise ValueError("read-only observation credential is unavailable")
                value = capture_current_version(value, args.account_id, args.worker_name, lambda path: api_get(path, token))
            # Capture the current provider closure before enforcing the identity verdict.
            args.status.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n", encoding="utf-8")
        elif any((args.account_id, args.worker_name, args.token_env)):
            raise ValueError("provider scope requires explicit version observation")
        release_id, profile_id = extract_identity(value)
        args.output.write_text(json.dumps({"schema_version": 2, "kind": "DEPLOYMENT_IDENTITY_OBSERVATION",
                                          "release_set_id": release_id, "capability_profile_id": profile_id},
                                         sort_keys=True, indent=2) + "\n", encoding="utf-8")
        return 0
    except (OSError, ValueError) as error:
        print(f"AR-11 deployment identity adapter error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
