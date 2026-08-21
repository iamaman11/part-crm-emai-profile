#!/usr/bin/env python3
"""Fail closed when canonical GitHub Actions secret references drift from credential authority."""

from __future__ import annotations

import argparse
import json
import re
import tempfile
from dataclasses import dataclass, replace
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE_AUTHORITY = Path("architecture/credential-authority-ar8b.json")
AR11_EXTENSION = Path("architecture/credential-authority-ar11-extension.json")
REGISTRY = Path("architecture/github-actions-registry.json")
SECRET_RE = re.compile(r"\$\{\{\s*secrets\.([A-Z][A-Z0-9_]*)")
JOB_RE = re.compile(r"^  ([A-Za-z0-9_-]+):\s*(?:#.*)?$")
ENV_RE = re.compile(r"^    environment:\s*(.*?)\s*(?:#.*)?$")
DYNAMIC_ENV_RE = re.compile(r"^\$\{\{\s*inputs\.([A-Za-z0-9_-]+)\s*\}\}$")
WORKFLOW_PREFIX = ".github/workflows/"
SECRET_SURFACES = {"github_actions_secret", "github_environment_secret"}


@dataclass(frozen=True)
class Binding:
    credential_id: str
    name: str
    surface: str
    consumers: frozenset[str]
    environments: frozenset[str]
    declaration_only: bool
    required_usage: bool


@dataclass(frozen=True)
class DynamicEnvironmentContract:
    credential_id: str
    workflow: str
    expression: str
    allowed_environments: frozenset[str]
    required_guard_fragments: tuple[str, ...]


@dataclass(frozen=True)
class Reference:
    workflow: str
    line: int
    job: str | None
    environment: str | None
    secret: str


def load_json(root: Path, relative: Path) -> dict:
    payload = json.loads((root / relative).read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise ValueError(f"{relative} must contain one JSON object")
    return payload


def load_authorities(root: Path) -> tuple[list[dict], dict]:
    base = load_json(root, BASE_AUTHORITY)
    extension = load_json(root, AR11_EXTENSION)
    if extension.get("kind") != "AR11_ADDITIVE_CREDENTIAL_CAPABILITY_EXTENSION":
        raise ValueError("AR-11 credential extension kind is invalid")
    if extension.get("parent_authority") != BASE_AUTHORITY.as_posix():
        raise ValueError("AR-11 credential extension must point to the accepted parent authority")
    if extension.get("competing_registry") is not False or extension.get("metadata_only") is not True:
        raise ValueError("AR-11 credential extension must remain metadata-only and non-competing")
    return [base, extension], extension


def canonical_workflows(registry: dict) -> list[str]:
    rows = registry.get("active_registrations")
    if not isinstance(rows, list):
        raise ValueError("canonical Actions registry active_registrations must be an array")
    paths: list[str] = []
    for row in rows:
        if not isinstance(row, dict) or not isinstance(row.get("path"), str):
            raise ValueError("canonical Actions registry contains a malformed active registration")
        path = row["path"]
        if not path.startswith(WORKFLOW_PREFIX) or not re.search(r"\.ya?ml$", path, re.I):
            raise ValueError(f"canonical Actions registry contains invalid workflow path: {path}")
        paths.append(path)
    if len(paths) != len(set(paths)):
        raise ValueError("canonical Actions registry contains duplicate workflow paths")
    return sorted(paths)


def bindings_from_authorities(
    authorities: list[dict], extension: dict
) -> tuple[dict[str, Binding], dict[tuple[str, str], DynamicEnvironmentContract], list[str]]:
    errors: list[str] = []
    result: dict[str, Binding] = {}
    base_credential_ids: set[str] = set()

    for authority_index, authority in enumerate(authorities):
        credentials = authority.get("credentials")
        if not isinstance(credentials, list):
            errors.append("credential authority credentials must be an array")
            continue
        for credential in credentials:
            if not isinstance(credential, dict):
                errors.append("credential authority contains a non-object credential")
                continue
            credential_id = credential.get("id")
            consumers_raw = credential.get("consumers", [])
            if not isinstance(credential_id, str) or not isinstance(consumers_raw, list) or any(
                not isinstance(value, str) for value in consumers_raw
            ):
                errors.append("every credential requires a string id and string-array consumers")
                continue
            if authority_index == 0:
                base_credential_ids.add(credential_id)
            workflow_consumers = frozenset(value for value in consumers_raw if value.startswith(WORKFLOW_PREFIX))
            scope = credential.get("environment_scope")
            scope_environments = scope.get("environments", []) if isinstance(scope, dict) else []
            if not isinstance(scope_environments, list) or any(not isinstance(value, str) for value in scope_environments):
                errors.append(f"credential {credential_id} has malformed environment_scope.environments")
                continue
            bindings = credential.get("bindings", [])
            if not isinstance(bindings, list):
                errors.append(f"credential {credential_id} bindings must be an array")
                continue
            for binding in bindings:
                if not isinstance(binding, dict) or binding.get("surface") not in SECRET_SURFACES:
                    continue
                name = binding.get("name")
                if not isinstance(name, str) or not re.fullmatch(r"[A-Z][A-Z0-9_]*", name):
                    errors.append(f"credential {credential_id} has malformed GitHub secret binding name")
                    continue
                environments_raw = binding.get("environments", scope_environments)
                if not isinstance(environments_raw, list) or any(not isinstance(value, str) for value in environments_raw):
                    errors.append(f"credential {credential_id} binding {name} has malformed environments")
                    continue
                classified = Binding(
                    credential_id=credential_id,
                    name=name,
                    surface=binding["surface"],
                    consumers=workflow_consumers,
                    environments=frozenset(environments_raw),
                    declaration_only=binding.get("declaration_only") is True,
                    required_usage=binding.get("required_usage") is True,
                )
                if name in result:
                    errors.append(
                        f"GitHub secret binding {name} is classified by multiple credential authorities: "
                        f"{result[name].credential_id}, {credential_id}"
                    )
                else:
                    result[name] = classified

    dynamic_contracts: dict[tuple[str, str], DynamicEnvironmentContract] = {}
    rows = extension.get("existing_credential_extensions", [])
    if not isinstance(rows, list):
        errors.append("AR-11 existing_credential_extensions must be an array")
        return result, dynamic_contracts, errors

    seen_credentials: set[str] = set()
    for row in rows:
        if not isinstance(row, dict):
            errors.append("AR-11 existing credential extension must be an object")
            continue
        credential_id = row.get("credential_id")
        add_consumers = row.get("add_consumers")
        rationale = row.get("rationale")
        if (
            not isinstance(credential_id, str)
            or credential_id not in base_credential_ids
            or credential_id in seen_credentials
            or not isinstance(add_consumers, list)
            or not add_consumers
            or any(
                not isinstance(value, str) or not value.startswith(WORKFLOW_PREFIX)
                for value in add_consumers
            )
            or len(add_consumers) != len(set(add_consumers))
            or not isinstance(rationale, str)
            or not rationale
        ):
            errors.append(f"AR-11 existing credential extension is malformed/unreviewed: {credential_id!r}")
            continue
        seen_credentials.add(credential_id)

        target_names = [name for name, binding in result.items() if binding.credential_id == credential_id]
        if not target_names:
            errors.append(f"AR-11 credential extension has no GitHub secret binding target: {credential_id}")
            continue
        for name in target_names:
            result[name] = replace(
                result[name],
                consumers=frozenset(set(result[name].consumers) | set(add_consumers)),
            )

        contracts = row.get("dynamic_environment_contracts", [])
        if not isinstance(contracts, list):
            errors.append(f"AR-11 dynamic environment contracts must be an array: {credential_id}")
            continue
        for contract in contracts:
            if not isinstance(contract, dict):
                errors.append(f"AR-11 dynamic environment contract must be an object: {credential_id}")
                continue
            workflow = contract.get("workflow")
            expression = contract.get("expression")
            allowed_raw = contract.get("allowed_environments")
            guards_raw = contract.get("required_guard_fragments")
            if (
                not isinstance(workflow, str)
                or workflow not in add_consumers
                or not isinstance(expression, str)
                or DYNAMIC_ENV_RE.fullmatch(expression) is None
                or not isinstance(allowed_raw, list)
                or not allowed_raw
                or any(not isinstance(value, str) or not value for value in allowed_raw)
                or len(allowed_raw) != len(set(allowed_raw))
                or not isinstance(guards_raw, list)
                or not guards_raw
                or any(not isinstance(value, str) or not value for value in guards_raw)
            ):
                errors.append(
                    f"AR-11 dynamic environment contract is malformed/unreviewed: {credential_id} {workflow!r}"
                )
                continue
            allowed = frozenset(allowed_raw)
            for name in target_names:
                if result[name].surface == "github_environment_secret" and allowed != result[name].environments:
                    errors.append(
                        f"AR-11 dynamic environment scope must exactly match accepted binding scope: "
                        f"{credential_id} {workflow} allowed={sorted(allowed)} "
                        f"binding={sorted(result[name].environments)}"
                    )
            key = (credential_id, workflow)
            if key in dynamic_contracts:
                errors.append(f"duplicate AR-11 dynamic environment contract: {credential_id} {workflow}")
                continue
            dynamic_contracts[key] = DynamicEnvironmentContract(
                credential_id=credential_id,
                workflow=workflow,
                expression=expression,
                allowed_environments=allowed,
                required_guard_fragments=tuple(guards_raw),
            )
    return result, dynamic_contracts, errors


def _yaml_scalar(value: str) -> str | None:
    value = value.strip()
    if not value:
        return None
    if len(value) >= 2 and value[0] == value[-1] and value[0] in {"'", '"'}:
        value = value[1:-1]
    return value or None


def workflow_job_metadata(text: str) -> tuple[dict[int, str | None], dict[str, str | None]]:
    line_jobs: dict[int, str | None] = {}
    environments: dict[str, str | None] = {}
    in_jobs = False
    current_job: str | None = None
    for number, line in enumerate(text.splitlines(), start=1):
        if line == "jobs:":
            in_jobs = True
            current_job = None
            line_jobs[number] = None
            continue
        if in_jobs and line and not line.startswith(" ") and not line.startswith("#"):
            in_jobs = False
            current_job = None
        if in_jobs:
            job_match = JOB_RE.match(line)
            if job_match:
                current_job = job_match.group(1)
                environments.setdefault(current_job, None)
            elif current_job is not None:
                env_match = ENV_RE.match(line)
                if env_match:
                    environments[current_job] = _yaml_scalar(env_match.group(1))
        line_jobs[number] = current_job if in_jobs else None
    return line_jobs, environments


def scan_references(
    root: Path, workflows: list[str]
) -> tuple[list[Reference], dict[str, str], list[str]]:
    references: list[Reference] = []
    texts: dict[str, str] = {}
    errors: list[str] = []
    for workflow in workflows:
        path = root / workflow
        if not path.is_file():
            errors.append(f"canonical workflow is missing from repository: {workflow}")
            continue
        text = path.read_text(encoding="utf-8")
        texts[workflow] = text
        line_jobs, environments = workflow_job_metadata(text)
        for number, line in enumerate(text.splitlines(), start=1):
            for match in SECRET_RE.finditer(line):
                job = line_jobs.get(number)
                references.append(
                    Reference(
                        workflow=workflow,
                        line=number,
                        job=job,
                        environment=environments.get(job) if job else None,
                        secret=match.group(1),
                    )
                )
    return references, texts, errors


def validate(root: Path) -> list[str]:
    authorities, extension = load_authorities(root)
    workflows = canonical_workflows(load_json(root, REGISTRY))
    bindings, dynamic_contracts, errors = bindings_from_authorities(authorities, extension)
    references, workflow_texts, scan_errors = scan_references(root, workflows)
    errors.extend(scan_errors)
    usage: dict[str, set[str]] = {name: set() for name in bindings}

    for (credential_id, workflow), contract in dynamic_contracts.items():
        if workflow not in workflows:
            errors.append(
                f"governed dynamic environment consumer is not a canonical active workflow: "
                f"{credential_id} {workflow}"
            )
            continue
        text = workflow_texts.get(workflow, "")
        missing = [fragment for fragment in contract.required_guard_fragments if fragment not in text]
        if missing:
            errors.append(
                f"dynamic environment contract guard missing: {credential_id} {workflow}: {missing}"
            )

    for reference in references:
        binding = bindings.get(reference.secret)
        label = f"{reference.workflow}:{reference.line} secrets.{reference.secret}"
        if binding is None:
            errors.append(f"unclassified permanent workflow secret reference: {label}")
            continue
        usage[reference.secret].add(reference.workflow)
        if reference.workflow not in binding.consumers:
            errors.append(
                f"wrong secret consumer: {label} is classified by {binding.credential_id} but consumer is not authorized"
            )
        if binding.surface == "github_environment_secret":
            if reference.environment in binding.environments:
                continue
            contract = dynamic_contracts.get((binding.credential_id, reference.workflow))
            if (
                contract is None
                or reference.environment != contract.expression
                or contract.allowed_environments != binding.environments
            ):
                errors.append(
                    f"wrong secret environment: {label} executes in {reference.environment!r}; "
                    f"allowed literals={sorted(binding.environments)} and no matching governed dynamic environment contract"
                )

    for name, binding in bindings.items():
        if binding.required_usage and not binding.declaration_only and not usage[name]:
            errors.append(
                f"classified GitHub secret binding is required but unused by canonical workflows: {name} ({binding.credential_id})"
            )
    return errors


def self_test() -> bool:
    base = {
        "credentials": [
            {
                "id": "cloudflare.platform-api",
                "consumers": [],
                "environment_scope": {
                    "kind": "environment",
                    "environments": ["staging", "production"],
                },
                "bindings": [
                    {
                        "surface": "github_environment_secret",
                        "name": "PLATFORM_TOKEN",
                        "environments": ["staging", "production"],
                    }
                ],
            }
        ]
    }
    extension = {
        "kind": "AR11_ADDITIVE_CREDENTIAL_CAPABILITY_EXTENSION",
        "parent_authority": BASE_AUTHORITY.as_posix(),
        "metadata_only": True,
        "competing_registry": False,
        "credentials": [
            {
                "id": "cloudflare.observe",
                "consumers": [".github/workflows/ok.yml"],
                "environment_scope": {"kind": "environment", "environments": ["staging"]},
                "bindings": [
                    {
                        "surface": "github_environment_secret",
                        "name": "OBSERVE_TOKEN",
                        "environments": ["staging"],
                        "required_usage": True,
                    }
                ],
            }
        ],
        "existing_credential_extensions": [
            {
                "credential_id": "cloudflare.platform-api",
                "add_consumers": [".github/workflows/dynamic.yml"],
                "dynamic_environment_contracts": [
                    {
                        "workflow": ".github/workflows/dynamic.yml",
                        "expression": "${{ inputs.environment }}",
                        "allowed_environments": ["staging", "production"],
                        "required_guard_fragments": [
                            "needs: authorize",
                            "environment: ${{ inputs.environment }}",
                            "TARGET_ENVIRONMENT: ${{ inputs.environment }}",
                            "test \"$TARGET_ENVIRONMENT\" = \"staging\" || test \"$TARGET_ENVIRONMENT\" = \"production\"",
                        ],
                    }
                ],
                "rationale": "negative fixture proves bounded reusable protected-environment projection",
            }
        ],
    }
    registry = {
        "active_registrations": [
            {"path": ".github/workflows/dynamic.yml", "category": "PERMANENT_REQUIRED"},
            {"path": ".github/workflows/ok.yml", "category": "CURRENT_MANUAL_OPERATION"},
        ]
    }
    dynamic_ok = (
        "jobs:\n"
        "  authorize:\n"
        "    steps:\n"
        "      - run: test true\n"
        "  migrate:\n"
        "    needs: authorize\n"
        "    environment: ${{ inputs.environment }}\n"
        "    env:\n"
        "      TARGET_ENVIRONMENT: ${{ inputs.environment }}\n"
        "      TOKEN: ${{ secrets.PLATFORM_TOKEN }}\n"
        "    steps:\n"
        "      - run: test \"$TARGET_ENVIRONMENT\" = \"staging\" || test \"$TARGET_ENVIRONMENT\" = \"production\"\n"
    )
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        (root / "architecture").mkdir()
        (root / ".github/workflows").mkdir(parents=True)
        (root / BASE_AUTHORITY).write_text(json.dumps(base), encoding="utf-8")
        (root / AR11_EXTENSION).write_text(json.dumps(extension), encoding="utf-8")
        (root / REGISTRY).write_text(json.dumps(registry), encoding="utf-8")
        workflow = root / ".github/workflows/ok.yml"
        dynamic = root / ".github/workflows/dynamic.yml"
        workflow.write_text(
            "jobs:\n  observe:\n    environment: staging\n    env:\n      TOKEN: ${{ secrets.OBSERVE_TOKEN }}\n",
            encoding="utf-8",
        )
        dynamic.write_text(dynamic_ok, encoding="utf-8")
        if validate(root):
            return False

        dynamic.write_text(dynamic_ok.replace("needs: authorize\n", ""), encoding="utf-8")
        if not any("dynamic environment contract guard missing" in error for error in validate(root)):
            return False
        dynamic.write_text(dynamic_ok, encoding="utf-8")

        dynamic.write_text(
            dynamic_ok.replace("${{ inputs.environment }}", "${{ inputs.unbounded }}"),
            encoding="utf-8",
        )
        if not any("wrong secret environment" in error for error in validate(root)):
            return False
        dynamic.write_text(dynamic_ok, encoding="utf-8")

        workflow.write_text(
            "jobs:\n  observe:\n    environment: staging\n    env:\n      TOKEN: ${{ secrets.UNKNOWN_TOKEN }}\n",
            encoding="utf-8",
        )
        if not any("unclassified permanent workflow secret reference" in error for error in validate(root)):
            return False

        workflow.write_text(
            "jobs:\n  observe:\n    environment: production\n    env:\n      TOKEN: ${{ secrets.OBSERVE_TOKEN }}\n",
            encoding="utf-8",
        )
        if not any("wrong secret environment" in error for error in validate(root)):
            return False

        extension["credentials"][0]["consumers"] = [".github/workflows/other.yml"]
        (root / AR11_EXTENSION).write_text(json.dumps(extension), encoding="utf-8")
        workflow.write_text(
            "jobs:\n  observe:\n    environment: staging\n    env:\n      TOKEN: ${{ secrets.OBSERVE_TOKEN }}\n",
            encoding="utf-8",
        )
        if not any("wrong secret consumer" in error for error in validate(root)):
            return False
    print("Workflow secret authority negative fixtures passed.")
    return True


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return 0 if self_test() else 1
    try:
        errors = validate(args.root.resolve())
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(error)
        return 1
    for error in errors:
        print(error)
    if errors:
        return 1
    print("Canonical workflow secret references match credential authority and environment boundaries.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
