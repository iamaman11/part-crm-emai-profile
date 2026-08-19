#!/usr/bin/env python3
"""Validate and render the canonical Cloudflare Wrangler deployment configuration.

Tracked configuration contains binding names and placeholder tokens only. Real staging/production
resource identities are supplied by controlled JSON manifests at release time. Secret *names* are
part of Wrangler configuration; secret values never enter the manifests or rendered document.

AR-11 makes the tracked Wrangler template the Core deployment closure: mailbox jobs and the
mailbox-secret-resolver service remain source-present but are intentionally absent from Core.
Capability selection is carried by the canonical environment/profile projection rather than by
independent feature flags.
"""

from __future__ import annotations

import argparse
import copy
import json
import re
import sys
from pathlib import Path
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[1]
TEMPLATE_PATH = ROOT / "deploy" / "cloudflare" / "wrangler.jsonc"

ENVIRONMENTS = ("staging", "production")
TOP_LEVEL_KEYS = {
    "name",
    "main",
    "compatibility_date",
    "workers_dev",
    "build",
    "assets",
    "triggers",
    "migrations",
    "env",
}
ENVIRONMENT_KEYS = {
    "name",
    "account_id",
    "routes",
    "vars",
    "secrets",
    "d1_databases",
    "r2_buckets",
    "queues",
    "durable_objects",
}
MANIFEST_FIELDS = {
    "worker_name",
    "account_id",
    "custom_domain",
    "access_issuer",
    "access_audience",
    "d1_database_name",
    "d1_database_id",
    "r2_bucket_name",
    "integration_events_queue",
    "mailbox_jobs_queue",
    "mailbox_jobs_dlq",
    "mailbox_secret_resolver_service",
}
ISOLATED_FIELDS = {
    "worker_name",
    "custom_domain",
    "access_audience",
    "d1_database_name",
    "d1_database_id",
    "r2_bucket_name",
    "integration_events_queue",
    "mailbox_jobs_queue",
    "mailbox_jobs_dlq",
    "mailbox_secret_resolver_service",
}
REQUIRED_CORE_SECRETS = [
    "CLIENT_CONTACT_PROTECTION_KEYRING",
    "R2_GENERATION_ACCESS_KEY_ID",
    "R2_GENERATION_SECRET_ACCESS_KEY",
]
EXPECTED_VARS = {
    "ACCESS_ISSUER",
    "ACCESS_AUDIENCE",
    "R2_GENERATION_ACCOUNT_ID",
    "R2_GENERATION_BUCKET_NAME",
    "CANONICAL_ENVIRONMENT",
    "CAPABILITY_PROFILE_ID",
    "CAPABILITY_PROFILE_DIGEST",
}
EXPECTED_D1 = {"CATALOG_DB"}
EXPECTED_R2 = {"PROFILE_OBJECTS"}
EXPECTED_CORE_QUEUE_PRODUCERS = {"INTEGRATION_EVENTS"}
EXPECTED_DURABLE_OBJECTS = {
    "PROFILE_COORDINATOR": "ProfileCoordinator",
    "NOTIFICATION_HUB": "NotificationHub",
}
RESOURCE_NAME = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$")
D1_ID = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$")
ACCOUNT_ID = re.compile(r"^[0-9a-f]{32}$")
AUDIENCE = re.compile(r"^[A-Za-z0-9_-]{16,128}$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")


class ConfigError(ValueError):
    pass


def load_json(path: Path) -> object:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ConfigError(f"cannot read JSON document {path}: {error}") from error


def require_object(value: object, label: str) -> dict[str, object]:
    if not isinstance(value, dict):
        raise ConfigError(f"{label} must be an object")
    return value


def require_array(value: object, label: str) -> list[object]:
    if not isinstance(value, list):
        raise ConfigError(f"{label} must be an array")
    return value


def string(value: object, label: str) -> str:
    if not isinstance(value, str) or not value:
        raise ConfigError(f"{label} must be a non-empty string")
    return value


def validate_template() -> dict[str, object]:
    document = require_object(load_json(TEMPLATE_PATH), "wrangler template")
    if set(document) != TOP_LEVEL_KEYS:
        missing = sorted(TOP_LEVEL_KEYS - set(document))
        extra = sorted(set(document) - TOP_LEVEL_KEYS)
        raise ConfigError(f"canonical Wrangler top-level keys drifted: missing={missing}, extra={extra}")
    if document.get("name") != "browser-profile-control-plane":
        raise ConfigError("base Worker name drifted")
    if document.get("main") != "../../apps/control-plane-worker/build/worker/shim.mjs":
        raise ConfigError("Worker module entrypoint drifted")
    if document.get("compatibility_date") != "2026-08-05":
        raise ConfigError("compatibility_date drifted from the accepted runtime baseline")
    if document.get("workers_dev") is not False:
        raise ConfigError("workers_dev must remain false")

    build = require_object(document.get("build"), "build")
    if build != {
        "command": "cargo install worker-build --version 0.8.5 --locked && worker-build --release",
        "cwd": "../../apps/control-plane-worker",
    }:
        raise ConfigError("Worker build command/cwd drifted from the pinned release build")

    assets = require_object(document.get("assets"), "assets")
    if assets != {
        "directory": "../../frontend/dist",
        "binding": "ASSETS",
        "run_worker_first": True,
        "not_found_handling": "single-page-application",
    }:
        raise ConfigError("Workers Static Assets configuration drifted from the accepted topology")
    if document.get("triggers") != {"crons": ["* * * * *"]}:
        raise ConfigError("scheduled runtime trigger drifted")

    migrations = require_array(document.get("migrations"), "migrations")
    if migrations != [
        {"tag": "v1", "new_sqlite_classes": ["ProfileCoordinator"]},
        {"tag": "v2", "new_sqlite_classes": ["NotificationHub"]},
    ]:
        raise ConfigError(
            "Durable Object migration lineage must preserve SQLite-backed ProfileCoordinator v1 and NotificationHub v2"
        )

    envs = require_object(document.get("env"), "env")
    if set(envs) != set(ENVIRONMENTS):
        raise ConfigError("canonical Wrangler config must define exactly staging and production")
    for environment in ENVIRONMENTS:
        validate_environment_template(environment, require_object(envs.get(environment), f"env.{environment}"))
    return document


def validate_environment_template(environment: str, config: dict[str, object]) -> None:
    prefix = environment.upper()
    if set(config) != ENVIRONMENT_KEYS:
        missing = sorted(ENVIRONMENT_KEYS - set(config))
        extra = sorted(set(config) - ENVIRONMENT_KEYS)
        raise ConfigError(f"{environment} config keys drifted: missing={missing}, extra={extra}")
    if string(config.get("name"), f"env.{environment}.name") != f"${{{prefix}_WORKER_NAME}}":
        raise ConfigError(f"{environment} Worker name must be controlled by its environment manifest")
    if string(config.get("account_id"), f"env.{environment}.account_id") != f"${{{prefix}_ACCOUNT_ID}}":
        raise ConfigError(f"{environment} deploy account must be controlled by its environment manifest")

    routes = require_array(config.get("routes"), f"env.{environment}.routes")
    if routes != [{"pattern": f"${{{prefix}_CUSTOM_DOMAIN}}", "custom_domain": True}]:
        raise ConfigError(f"{environment} must expose exactly one controlled custom domain")

    variables = require_object(config.get("vars"), f"env.{environment}.vars")
    if set(variables) != EXPECTED_VARS:
        raise ConfigError(f"{environment} vars do not match accepted Core runtime requirements")
    expected_variable_tokens = {
        "ACCESS_ISSUER": f"${{{prefix}_ACCESS_ISSUER}}",
        "ACCESS_AUDIENCE": f"${{{prefix}_ACCESS_AUDIENCE}}",
        "R2_GENERATION_ACCOUNT_ID": f"${{{prefix}_ACCOUNT_ID}}",
        "R2_GENERATION_BUCKET_NAME": f"${{{prefix}_R2_BUCKET_NAME}}",
    }
    for key, expected in expected_variable_tokens.items():
        if variables.get(key) != expected:
            raise ConfigError(f"{environment} variable placeholder drifted for {key}")
    if variables.get("CANONICAL_ENVIRONMENT") != environment:
        raise ConfigError(f"{environment} canonical environment projection drifted")
    expected_profile = "rehearsal-core-v1" if environment == "staging" else "production-core-v1"
    if variables.get("CAPABILITY_PROFILE_ID") != expected_profile:
        raise ConfigError(f"{environment} capability profile projection must select {expected_profile}")
    profile_digest = variables.get("CAPABILITY_PROFILE_DIGEST")
    if not isinstance(profile_digest, str) or SHA256.fullmatch(profile_digest) is None:
        raise ConfigError(f"{environment} capability profile digest must be lowercase SHA-256")
    for key in variables:
        if re.match(r"^(ENABLE_|FEATURE_|SHOW_)", key):
            raise ConfigError(f"independent capability flag is forbidden in {environment}: {key}")

    secrets = require_object(config.get("secrets"), f"env.{environment}.secrets")
    if secrets != {"required": REQUIRED_CORE_SECRETS}:
        raise ConfigError(f"{environment} Core required secret-name inventory drifted")

    d1 = require_array(config.get("d1_databases"), f"env.{environment}.d1_databases")
    d1_bindings = {require_object(item, "D1 binding").get("binding") for item in d1}
    if d1_bindings != EXPECTED_D1 or len(d1) != 1:
        raise ConfigError(f"{environment} D1 bindings do not match Worker composition")
    if d1[0] != {
        "binding": "CATALOG_DB",
        "database_name": f"${{{prefix}_D1_DATABASE_NAME}}",
        "database_id": f"${{{prefix}_D1_DATABASE_ID}}",
    }:
        raise ConfigError(f"{environment} CATALOG_DB placeholders are not canonical")

    r2 = require_array(config.get("r2_buckets"), f"env.{environment}.r2_buckets")
    r2_bindings = {require_object(item, "R2 binding").get("binding") for item in r2}
    if r2_bindings != EXPECTED_R2 or len(r2) != 1:
        raise ConfigError(f"{environment} R2 bindings do not match Worker composition")
    if r2[0] != {
        "binding": "PROFILE_OBJECTS",
        "bucket_name": f"${{{prefix}_R2_BUCKET_NAME}}",
    }:
        raise ConfigError(f"{environment} PROFILE_OBJECTS placeholder is not canonical")

    queues = require_object(config.get("queues"), f"env.{environment}.queues")
    producers = require_array(queues.get("producers"), f"env.{environment}.queues.producers")
    producer_bindings = {require_object(item, "queue producer").get("binding") for item in producers}
    if producer_bindings != EXPECTED_CORE_QUEUE_PRODUCERS or len(producers) != 1:
        raise ConfigError(f"{environment} Core queue producer bindings drifted")
    if producers[0] != {
        "binding": "INTEGRATION_EVENTS",
        "queue": f"${{{prefix}_INTEGRATION_EVENTS_QUEUE}}",
    }:
        raise ConfigError(f"{environment} INTEGRATION_EVENTS queue placeholder drifted")

    consumers = require_array(queues.get("consumers"), f"env.{environment}.queues.consumers")
    expected_consumers = [
        {
            "queue": f"${{{prefix}_INTEGRATION_EVENTS_QUEUE}}",
            "max_batch_size": 10,
            "max_batch_timeout": 5,
        }
    ]
    if consumers != expected_consumers:
        raise ConfigError(f"{environment} Core queue consumer policy drifted")

    durable = require_object(config.get("durable_objects"), f"env.{environment}.durable_objects")
    bindings = require_array(durable.get("bindings"), f"env.{environment}.durable_objects.bindings")
    observed = {
        string(require_object(item, "Durable Object binding").get("name"), "Durable Object name"):
        string(require_object(item, "Durable Object binding").get("class_name"), "Durable Object class")
        for item in bindings
    }
    if observed != EXPECTED_DURABLE_OBJECTS or len(bindings) != len(EXPECTED_DURABLE_OBJECTS):
        raise ConfigError(f"{environment} Durable Object bindings drifted")


def validate_manifest(environment: str, manifest: object, *, fixture: bool = False) -> dict[str, str]:
    document = require_object(manifest, f"{environment} manifest")
    if set(document) != MANIFEST_FIELDS:
        missing = sorted(MANIFEST_FIELDS - set(document))
        extra = sorted(set(document) - MANIFEST_FIELDS)
        raise ConfigError(f"{environment} manifest fields mismatch: missing={missing}, extra={extra}")
    values = {key: string(document[key], f"{environment}.{key}") for key in MANIFEST_FIELDS}
    for key, value in values.items():
        lowered = value.lower()
        if "${" in value or "replace_with" in lowered or "placeholder" in lowered:
            raise ConfigError(f"{environment}.{key} contains a forbidden placeholder")
        if not fixture and ("example" in lowered or lowered.endswith(".test")):
            raise ConfigError(f"{environment}.{key} looks like fixture/example data")

    for key in (
        "worker_name",
        "d1_database_name",
        "r2_bucket_name",
        "integration_events_queue",
        "mailbox_jobs_queue",
        "mailbox_jobs_dlq",
        "mailbox_secret_resolver_service",
    ):
        if RESOURCE_NAME.fullmatch(values[key]) is None:
            raise ConfigError(f"{environment}.{key} is not a bounded Cloudflare resource name")
    if ACCOUNT_ID.fullmatch(values["account_id"]) is None:
        raise ConfigError(f"{environment}.account_id must be a 32-hex Cloudflare account identifier")
    if D1_ID.fullmatch(values["d1_database_id"]) is None:
        raise ConfigError(f"{environment}.d1_database_id must be a UUID-shaped D1 identifier")
    if AUDIENCE.fullmatch(values["access_audience"]) is None:
        raise ConfigError(f"{environment}.access_audience has an invalid shape")

    issuer = urlparse(values["access_issuer"])
    if issuer.scheme != "https" or not issuer.hostname or issuer.path not in ("", "/") or issuer.query or issuer.fragment:
        raise ConfigError(f"{environment}.access_issuer must be a bare HTTPS issuer origin")
    if not fixture and not issuer.hostname.endswith(".cloudflareaccess.com"):
        raise ConfigError(f"{environment}.access_issuer must use a Cloudflare Access team domain")

    custom_domain = values["custom_domain"]
    if (
        "/" in custom_domain
        or ":" in custom_domain
        or custom_domain.startswith("*.")
        or custom_domain.endswith(".workers.dev")
        or "." not in custom_domain
    ):
        raise ConfigError(f"{environment}.custom_domain must be one dedicated hostname")

    queue_names = {
        values["integration_events_queue"],
        values["mailbox_jobs_queue"],
        values["mailbox_jobs_dlq"],
    }
    if len(queue_names) != 3:
        raise ConfigError(f"{environment} queue names must be distinct, including the DLQ")
    if values["r2_bucket_name"] == values["d1_database_name"]:
        raise ConfigError(f"{environment} D1 and R2 resource names must not collide")
    return values


def validate_isolation(staging: dict[str, str], production: dict[str, str]) -> None:
    reused = sorted(field for field in ISOLATED_FIELDS if staging[field] == production[field])
    if reused:
        raise ConfigError(f"staging/production resource identity reuse is forbidden: {reused}")


def token_map(environment: str, manifest: dict[str, str]) -> dict[str, str]:
    prefix = environment.upper()
    mapping = {
        "WORKER_NAME": "worker_name",
        "ACCOUNT_ID": "account_id",
        "CUSTOM_DOMAIN": "custom_domain",
        "ACCESS_ISSUER": "access_issuer",
        "ACCESS_AUDIENCE": "access_audience",
        "D1_DATABASE_NAME": "d1_database_name",
        "D1_DATABASE_ID": "d1_database_id",
        "R2_BUCKET_NAME": "r2_bucket_name",
        "INTEGRATION_EVENTS_QUEUE": "integration_events_queue",
        "MAILBOX_JOBS_QUEUE": "mailbox_jobs_queue",
        "MAILBOX_JOBS_DLQ": "mailbox_jobs_dlq",
        "MAILBOX_SECRET_RESOLVER_SERVICE": "mailbox_secret_resolver_service",
    }
    return {f"${{{prefix}_{token}}}": manifest[field] for token, field in mapping.items()}


def substitute(value: object, replacements: dict[str, str]) -> object:
    if isinstance(value, dict):
        return {key: substitute(item, replacements) for key, item in value.items()}
    if isinstance(value, list):
        return [substitute(item, replacements) for item in value]
    if isinstance(value, str):
        return replacements.get(value, value)
    return value


def render(staging_manifest: object, production_manifest: object, *, fixture: bool = False) -> dict[str, object]:
    template = validate_template()
    staging = validate_manifest("staging", staging_manifest, fixture=fixture)
    production = validate_manifest("production", production_manifest, fixture=fixture)
    validate_isolation(staging, production)
    replacements = token_map("staging", staging) | token_map("production", production)
    rendered = require_object(substitute(template, replacements), "rendered Wrangler configuration")
    serialized = json.dumps(rendered, sort_keys=True)
    leftovers = sorted(set(match.group(0) for match in re.finditer(r"\$\{[^}]+\}", serialized)))
    if leftovers:
        raise ConfigError(f"unresolved Wrangler placeholders remain: {leftovers}")
    return rendered


def fixture_manifest(environment: str) -> dict[str, str]:
    staging = environment == "staging"
    digit = "1" if staging else "2"
    label = "staging" if staging else "production"
    return {
        "worker_name": f"profile-control-{label}",
        "account_id": ("c" if staging else "d") * 32,
        "custom_domain": f"{label}.crm.invalid",
        "access_issuer": f"https://{label}.cloudflareaccess.invalid",
        "access_audience": ("a" if staging else "b") * 32,
        "d1_database_name": f"profile-catalog-{label}",
        "d1_database_id": f"{digit * 8}-{digit * 4}-{digit * 4}-{digit * 4}-{digit * 12}",
        "r2_bucket_name": f"profile-objects-{label}",
        "integration_events_queue": f"integration-events-{label}",
        "mailbox_jobs_queue": f"mailbox-jobs-{label}",
        "mailbox_jobs_dlq": f"mailbox-jobs-dlq-{label}",
        "mailbox_secret_resolver_service": f"mailbox-secret-resolver-{label}",
    }


def self_test() -> None:
    staging = fixture_manifest("staging")
    production = fixture_manifest("production")
    rendered = render(staging, production, fixture=True)
    envs = require_object(rendered["env"], "rendered env")
    staging_env = require_object(envs["staging"], "rendered staging")
    production_env = require_object(envs["production"], "rendered production")
    if staging_env["name"] != staging["worker_name"] or production_env["name"] != production["worker_name"]:
        raise ConfigError("positive render fixture did not substitute environment Worker names")
    if staging_env["account_id"] != staging["account_id"] or production_env["account_id"] != production["account_id"]:
        raise ConfigError("positive render fixture did not lock deploy account identities")
    staging_vars = require_object(staging_env["vars"], "rendered staging vars")
    production_vars = require_object(production_env["vars"], "rendered production vars")
    if staging_vars["R2_GENERATION_ACCOUNT_ID"] != staging["account_id"]:
        raise ConfigError("staging R2 account identity diverged from deploy account")
    if production_vars["R2_GENERATION_ACCOUNT_ID"] != production["account_id"]:
        raise ConfigError("production R2 account identity diverged from deploy account")
    if staging_vars["CAPABILITY_PROFILE_ID"] != "rehearsal-core-v1":
        raise ConfigError("staging Core profile projection drifted")
    if production_vars["CAPABILITY_PROFILE_ID"] != "production-core-v1":
        raise ConfigError("production Core profile projection drifted")
    if "services" in staging_env or "services" in production_env:
        raise ConfigError("Core render unexpectedly includes mailbox-secret-resolver service binding")
    if "${" in json.dumps(rendered):
        raise ConfigError("positive render fixture retained a placeholder")

    negative_cases: list[tuple[str, dict[str, str], dict[str, str]]] = []
    missing = copy.deepcopy(staging)
    missing.pop("d1_database_id")
    negative_cases.append(("missing binding identity", missing, production))
    placeholder = copy.deepcopy(staging)
    placeholder["r2_bucket_name"] = "REPLACE_WITH_BUCKET"
    negative_cases.append(("placeholder", placeholder, production))
    reused = copy.deepcopy(production)
    reused["mailbox_jobs_queue"] = staging["mailbox_jobs_queue"]
    negative_cases.append(("cross-environment resource reuse", staging, reused))
    duplicate_queue = copy.deepcopy(staging)
    duplicate_queue["mailbox_jobs_dlq"] = staging["mailbox_jobs_queue"]
    negative_cases.append(("queue/DLQ reuse", duplicate_queue, production))
    unexpected = copy.deepcopy(staging)
    unexpected["unexpected_secret"] = "forbidden"
    negative_cases.append(("unexpected manifest field", unexpected, production))

    for label, candidate_staging, candidate_production in negative_cases:
        try:
            render(candidate_staging, candidate_production, fixture=True)
        except ConfigError:
            continue
        raise ConfigError(f"negative fixture unexpectedly passed: {label}")
    print("Cloudflare profile-aware Core deploy configuration positive and negative fixtures passed.")


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true", help="validate the tracked canonical Wrangler template")
    mode.add_argument("--self-test", action="store_true", help="run deterministic positive/negative fixtures")
    mode.add_argument("--render", action="store_true", help="render a deploy-ready Wrangler JSON document")
    parser.add_argument("--staging-manifest", type=Path)
    parser.add_argument("--production-manifest", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()

    if args.check:
        validate_template()
        print("Canonical profile-aware Core Cloudflare Wrangler template is valid and complete.")
        return 0
    if args.self_test:
        self_test()
        return 0

    if args.staging_manifest is None or args.production_manifest is None or args.output is None:
        parser.error("--render requires --staging-manifest, --production-manifest and --output")
    staging = load_json(args.staging_manifest)
    production = load_json(args.production_manifest)
    rendered = render(staging, production)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(rendered, indent=2, sort_keys=False) + "\n", encoding="utf-8", newline="\n")
    print(f"Rendered {args.output}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ConfigError as error:
        print(f"cloudflare deploy configuration error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
