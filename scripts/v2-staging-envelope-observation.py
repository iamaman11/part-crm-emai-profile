#!/usr/bin/env python3
"""Sanitize V2 staging Cloudflare/D1 observations without owning provider policy."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import urlsplit

MIGRATION_RE = re.compile(r"^\d{4}_[A-Za-z0-9_.-]+\.sql$")
SOURCE_SHA_RE = re.compile(r"^[0-9a-f]{40}$")
SAFE_ID_RE = re.compile(r"^[A-Za-z0-9_-]+$")
MTLS_SELECTORS = {"certificate", "common_name"}


class ObservationError(ValueError):
    pass


def load_json(path: Path) -> object:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ObservationError(f"invalid_json:{path.name}") from error


def require_object(value: object, label: str) -> dict:
    if not isinstance(value, dict):
        raise ObservationError(f"{label}_must_be_object")
    return value


def cloudflare_result(path: Path, label: str) -> object:
    document = require_object(load_json(path), label)
    if document.get("success") is not True:
        raise ObservationError(f"{label}_unsuccessful")
    errors = document.get("errors", [])
    if not isinstance(errors, list) or errors:
        raise ObservationError(f"{label}_errors_present")
    if "result" not in document:
        raise ObservationError(f"{label}_result_missing")
    return document["result"]


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    try:
        with path.open("rb") as stream:
            for chunk in iter(lambda: stream.read(65536), b""):
                digest.update(chunk)
    except OSError as error:
        raise ObservationError(f"unreadable:{path.name}") from error
    return digest.hexdigest()


def hostname(value: str) -> str:
    raw = value.strip().lower()
    if not raw:
        raise ObservationError("hostname_empty")
    parsed = urlsplit(raw if "://" in raw else f"https://{raw}")
    if not parsed.hostname:
        raise ObservationError("hostname_invalid")
    return parsed.hostname.rstrip(".")


def target_from_manifest(path: Path) -> tuple[str, str]:
    document = require_object(load_json(path), "deploy_manifest")
    control = document.get("control_plane")
    if not isinstance(control, dict):
        control = document
    account_id = control.get("account_id")
    custom_domain = control.get("custom_domain")
    if not isinstance(account_id, str) or len(account_id) != 32:
        raise ObservationError("deploy_manifest_account_invalid")
    if not isinstance(custom_domain, str):
        raise ObservationError("deploy_manifest_custom_domain_invalid")
    return account_id, hostname(custom_domain)


def normalize_scalar(value: object) -> str | int | bool | None:
    if isinstance(value, (str, int, bool)) or value is None:
        return value
    return None


def rule_selectors(value: object) -> list[str]:
    if not isinstance(value, list):
        return []
    selectors: set[str] = set()
    for item in value:
        if not isinstance(item, dict):
            continue
        selectors.update(key for key in item if isinstance(key, str))
    return sorted(selectors)


def sanitize_policy(policy: dict) -> dict:
    include = rule_selectors(policy.get("include"))
    exclude = rule_selectors(policy.get("exclude"))
    require = rule_selectors(policy.get("require"))
    selectors = set(include) | set(exclude) | set(require)
    policy_id = policy.get("id")
    if not isinstance(policy_id, str) or not SAFE_ID_RE.fullmatch(policy_id):
        raise ObservationError("policy_id_invalid")
    return {
        "id": policy_id,
        "decision": normalize_scalar(policy.get("decision")),
        "precedence": normalize_scalar(policy.get("precedence")),
        "include_selectors": include,
        "exclude_selectors": exclude,
        "require_selectors": require,
        "mtls_selector_present": bool(selectors & MTLS_SELECTORS),
    }


def sanitize_applications(apps_path: Path, policies_dir: Path, target_host: str) -> list[dict]:
    result = cloudflare_result(apps_path, "access_apps")
    if not isinstance(result, list):
        raise ObservationError("access_apps_result_must_be_array")
    output: list[dict] = []
    for app in result:
        if not isinstance(app, dict):
            continue
        domain = app.get("domain")
        if not isinstance(domain, str):
            continue
        try:
            app_host = hostname(domain)
        except ObservationError:
            continue
        if app_host != target_host:
            continue
        app_id = app.get("id")
        if not isinstance(app_id, str) or not SAFE_ID_RE.fullmatch(app_id):
            raise ObservationError("access_app_id_invalid")
        policy_path = policies_dir / f"{app_id}.json"
        policy_result = cloudflare_result(policy_path, f"access_policies_{app_id}")
        if not isinstance(policy_result, list):
            raise ObservationError("access_policies_result_must_be_array")
        policies = [sanitize_policy(policy) for policy in policy_result if isinstance(policy, dict)]
        output.append(
            {
                "id": app_id,
                "domain": domain,
                "type": normalize_scalar(app.get("type")),
                "aud": normalize_scalar(app.get("aud")),
                "policies": sorted(policies, key=lambda item: (str(item.get("precedence")), item["id"])),
            }
        )
    return sorted(output, key=lambda item: item["id"])


def parse_observed_at(value: str) -> datetime:
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as error:
        raise ObservationError("observed_at_invalid") from error
    if parsed.tzinfo is None:
        raise ObservationError("observed_at_timezone_required")
    return parsed.astimezone(timezone.utc)


def sanitize_certificates(path: Path, target_host: str, observed_at: datetime) -> list[dict]:
    result = cloudflare_result(path, "access_certificates")
    if not isinstance(result, list):
        raise ObservationError("access_certificates_result_must_be_array")
    output: list[dict] = []
    for certificate in result:
        if not isinstance(certificate, dict):
            continue
        raw_hosts = certificate.get("associated_hostnames", [])
        if not isinstance(raw_hosts, list):
            continue
        matching_hosts: list[str] = []
        for raw_host in raw_hosts:
            if not isinstance(raw_host, str):
                continue
            try:
                if hostname(raw_host) == target_host:
                    matching_hosts.append(raw_host)
            except ObservationError:
                continue
        if not matching_hosts:
            continue
        certificate_id = certificate.get("id")
        if not isinstance(certificate_id, str) or not SAFE_ID_RE.fullmatch(certificate_id):
            raise ObservationError("access_certificate_id_invalid")
        expires_on = certificate.get("expires_on")
        valid_now = None
        if isinstance(expires_on, str):
            try:
                expires = datetime.fromisoformat(expires_on.replace("Z", "+00:00")).astimezone(timezone.utc)
                valid_now = expires > observed_at
            except ValueError:
                valid_now = None
        output.append(
            {
                "id": certificate_id,
                "associated_hostnames": sorted(set(matching_hosts)),
                "expires_on": expires_on if isinstance(expires_on, str) else None,
                "valid_at_observation": valid_now,
            }
        )
    return sorted(output, key=lambda item: item["id"])


def organization(path: Path) -> dict:
    result = cloudflare_result(path, "access_organization")
    result = require_object(result, "access_organization_result")
    auth_domain = result.get("auth_domain")
    if not isinstance(auth_domain, str) or not auth_domain:
        raise ObservationError("access_auth_domain_missing")
    auth_host = hostname(auth_domain)
    return {"auth_domain": auth_domain, "issuer": f"https://{auth_host}"}


def verified_token(path: Path) -> dict:
    result = cloudflare_result(path, "zero_trust_token_verify")
    result = require_object(result, "zero_trust_token_verify_result")
    status = result.get("status")
    token_id = result.get("id")
    if status != "active":
        raise ObservationError("zero_trust_token_not_active")
    if not isinstance(token_id, str) or not SAFE_ID_RE.fullmatch(token_id):
        raise ObservationError("zero_trust_token_id_invalid")
    return {"status": status, "token_id": token_id}


def collect_migration_names(value: object, output: set[str]) -> None:
    if isinstance(value, dict):
        name = value.get("name")
        if isinstance(name, str) and MIGRATION_RE.fullmatch(name):
            output.add(name)
        for nested in value.values():
            collect_migration_names(nested, output)
    elif isinstance(value, list):
        for nested in value:
            collect_migration_names(nested, output)


def d1_state(ledger_path: Path, migrations_dir: Path) -> dict:
    document = load_json(ledger_path)
    remote_set: set[str] = set()
    collect_migration_names(document, remote_set)
    try:
        canonical = sorted(path.name for path in migrations_dir.glob("*.sql") if MIGRATION_RE.fullmatch(path.name))
    except OSError as error:
        raise ObservationError("canonical_migrations_unreadable") from error
    if not canonical:
        raise ObservationError("canonical_migrations_empty")
    remote = sorted(remote_set)
    canonical_set = set(canonical)
    return {
        "migration_count": len(remote),
        "max_migration": remote[-1] if remote else None,
        "has_0031_device_binding_governance": "0031_device_binding_governance.sql" in remote_set,
        "unknown_extra_migrations": sorted(remote_set - canonical_set),
        "missing_canonical_migrations": sorted(canonical_set - remote_set),
        "remote_migrations": remote,
        "canonical_migrations": canonical,
        "raw_response_sha256": sha256_file(ledger_path),
    }


def render(args: argparse.Namespace) -> dict:
    if not SOURCE_SHA_RE.fullmatch(args.source_sha):
        raise ObservationError("source_sha_invalid")
    _account_id, target_host = target_from_manifest(args.deploy_manifest)
    observed_at = parse_observed_at(args.observed_at)
    token = verified_token(args.zero_trust_token_verify)
    apps = sanitize_applications(args.apps, args.policies_dir, target_host)
    certs = sanitize_certificates(args.certificates, target_host, observed_at)
    org = organization(args.organization)
    d1 = d1_state(args.d1_ledger, args.migrations_dir)
    policy_digests = {
        path.stem: sha256_file(path)
        for path in sorted(args.policies_dir.glob("*.json"))
        if SAFE_ID_RE.fullmatch(path.stem)
    }
    return {
        "schema_version": 1,
        "kind": "V2_STAGING_ENVELOPE_OBSERVATION",
        "source_sha": args.source_sha,
        "environment": "staging",
        "observed_at": args.observed_at,
        "target": {"control_plane_hostname": target_host},
        "zero_trust_credential": token,
        "access": {
            "application_count_for_target": len(apps),
            "applications": apps,
            "organization": org,
        },
        "mtls": {
            "matching_certificate_count": len(certs),
            "certificates": certs,
        },
        "d1": d1,
        "raw_response_digests_sha256": {
            "zero_trust_token_verify": sha256_file(args.zero_trust_token_verify),
            "access_apps": sha256_file(args.apps),
            "access_organization": sha256_file(args.organization),
            "access_certificates": sha256_file(args.certificates),
            "access_policies": policy_digests,
        },
        "provider_mutation": False,
        "production_mutation": False,
    }


def self_test() -> bool:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        policies = root / "policies"
        migrations = root / "migrations"
        policies.mkdir()
        migrations.mkdir()
        manifest = root / "manifest.json"
        verify = root / "verify.json"
        apps = root / "apps.json"
        org = root / "org.json"
        certs = root / "certs.json"
        ledger = root / "ledger.json"
        app_id = "app_12345678"
        manifest.write_text(json.dumps({"control_plane": {"account_id": "a" * 32, "custom_domain": "staging.example.test"}}), encoding="utf-8")
        verify.write_text(json.dumps({"success": True, "errors": [], "result": {"id": "token_12345678", "status": "active"}}), encoding="utf-8")
        apps.write_text(json.dumps({"success": True, "errors": [], "result": [
            {"id": app_id, "domain": "staging.example.test", "type": "self_hosted", "aud": "aud-test"},
            {"id": "app_other", "domain": "other.example.test", "type": "self_hosted", "aud": "other"},
        ]}), encoding="utf-8")
        org.write_text(json.dumps({"success": True, "errors": [], "result": {"auth_domain": "team.cloudflareaccess.com", "name": "private-name"}}), encoding="utf-8")
        certs.write_text(json.dumps({"success": True, "errors": [], "result": [
            {"id": "cert_12345678", "associated_hostnames": ["staging.example.test"], "expires_on": "2030-01-01T00:00:00Z", "certificate": "-----BEGIN CERTIFICATE-----DO_NOT_EXPORT"},
        ]}), encoding="utf-8")
        (policies / f"{app_id}.json").write_text(json.dumps({"success": True, "errors": [], "result": [
            {"id": "policy_12345678", "decision": "non_identity", "precedence": 1, "include": [{"certificate": {}}], "require": [{"email": {"email": "person@example.test"}}], "exclude": []},
        ]}), encoding="utf-8")
        for name in ["0030_previous.sql", "0031_device_binding_governance.sql"]:
            (migrations / name).write_text("-- fixture\n", encoding="utf-8")
        ledger.write_text(json.dumps([{"results": [{"name": "0030_previous.sql"}, {"name": "0031_device_binding_governance.sql"}]}]), encoding="utf-8")
        args = argparse.Namespace(
            source_sha="b" * 40,
            observed_at="2026-09-04T17:00:00Z",
            deploy_manifest=manifest,
            zero_trust_token_verify=verify,
            apps=apps,
            organization=org,
            certificates=certs,
            policies_dir=policies,
            d1_ledger=ledger,
            migrations_dir=migrations,
        )
        output = render(args)
        encoded = json.dumps(output, sort_keys=True)
        checks = [
            output["access"]["application_count_for_target"] == 1,
            output["mtls"]["matching_certificate_count"] == 1,
            output["access"]["applications"][0]["policies"][0]["mtls_selector_present"] is True,
            output["d1"]["has_0031_device_binding_governance"] is True,
            output["d1"]["unknown_extra_migrations"] == [],
            output["d1"]["missing_canonical_migrations"] == [],
            "person@example.test" not in encoded,
            "DO_NOT_EXPORT" not in encoded,
            "private-name" not in encoded,
            "a" * 32 not in encoded,
        ]
        if not all(checks):
            return False
        disabled = json.loads(verify.read_text(encoding="utf-8"))
        disabled["result"]["status"] = "disabled"
        verify.write_text(json.dumps(disabled), encoding="utf-8")
        try:
            render(args)
        except ObservationError as error:
            if str(error) != "zero_trust_token_not_active":
                return False
        else:
            return False
    print("V2 staging envelope observation adapter self-test passed.")
    return True


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--source-sha")
    parser.add_argument("--observed-at")
    parser.add_argument("--deploy-manifest", type=Path)
    parser.add_argument("--zero-trust-token-verify", type=Path)
    parser.add_argument("--apps", type=Path)
    parser.add_argument("--organization", type=Path)
    parser.add_argument("--certificates", type=Path)
    parser.add_argument("--policies-dir", type=Path)
    parser.add_argument("--d1-ledger", type=Path)
    parser.add_argument("--migrations-dir", type=Path, default=Path("migrations/d1"))
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.self_test:
        return 0 if self_test() else 1
    required = [
        "source_sha",
        "observed_at",
        "deploy_manifest",
        "zero_trust_token_verify",
        "apps",
        "organization",
        "certificates",
        "policies_dir",
        "d1_ledger",
        "output",
    ]
    missing = [name for name in required if getattr(args, name) is None]
    if missing:
        parser.error("missing required arguments: " + ", ".join(missing))
    try:
        output = render(args)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(output, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    except ObservationError as error:
        print(f"v2-staging-envelope-observation: {error}", flush=True)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
