#!/usr/bin/env python3
"""Fail closed when canonical GitHub Actions secret references drift from credential authority."""

from __future__ import annotations

import argparse
import json
import re
import tempfile
from dataclasses import dataclass, field, replace
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE_AUTHORITY = Path("architecture/credential-authority-ar8b.json")
AR11_EXTENSION = Path("architecture/credential-authority-ar11-extension.json")
REGISTRY = Path("architecture/github-actions-registry.json")
SECRET_RE = re.compile(r"\$\{\{\s*secrets\.([A-Z][A-Z0-9_]*)")
JOB_RE = re.compile(r"^  ([A-Za-z0-9_-]+):\s*(?:#.*)?$")
JOB_FIELD_RE = re.compile(r"^    ([A-Za-z0-9_-]+):\s*(.*?)\s*(?:#.*)?$")
JOB_ENV_KEY_RE = re.compile(r"^      ([A-Za-z_][A-Za-z0-9_]*):\s*(.*?)\s*(?:#.*)?$")
RUN_RE = re.compile(r"^        run:\s*([|>][0-9+-]*)?\s*(?:#.*)?$")
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


@dataclass
class Job:
    name: str
    start_line: int
    end_line: int = 0
    environment: str | None = None
    needs: set[str] = field(default_factory=set)
    env: dict[str, str] = field(default_factory=dict)
    run_lines: list[str] = field(default_factory=list)


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
    if extension.get("production_mutation") is not False:
        raise ValueError("AR-11 credential extension must keep production_mutation=false")
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
            or any(not isinstance(value, str) or not value.startswith(WORKFLOW_PREFIX) for value in add_consumers)
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
            result[name] = replace(result[name], consumers=frozenset(set(result[name].consumers) | set(add_consumers)))

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
                errors.append(f"AR-11 dynamic environment contract is malformed/unreviewed: {credential_id} {workflow!r}")
                continue
            allowed = frozenset(allowed_raw)
            for name in target_names:
                if result[name].surface == "github_environment_secret" and allowed != result[name].environments:
                    errors.append(
                        f"AR-11 dynamic environment scope must exactly match accepted binding scope: "
                        f"{credential_id} {workflow} allowed={sorted(allowed)} binding={sorted(result[name].environments)}"
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


def _leading_spaces(line: str) -> int:
    return len(line) - len(line.lstrip(" "))


def _parse_needs(value: str) -> set[str]:
    scalar = _yaml_scalar(value)
    if scalar is None:
        return set()
    if scalar.startswith("[") and scalar.endswith("]"):
        result = set()
        for item in scalar[1:-1].split(","):
            parsed = _yaml_scalar(item)
            if parsed:
                result.add(parsed)
        return result
    if re.fullmatch(r"[A-Za-z0-9_-]+", scalar):
        return {scalar}
    return set()


def parse_jobs(text: str) -> tuple[dict[str, Job], dict[int, str | None], list[str]]:
    lines = text.splitlines()
    jobs: dict[str, Job] = {}
    line_jobs: dict[int, str | None] = {}
    errors: list[str] = []
    in_jobs = False
    current: Job | None = None
    in_job_env = False
    run_indent: int | None = None

    for number, line in enumerate(lines, start=1):
        if line == "jobs:":
            in_jobs = True
            current = None
            line_jobs[number] = None
            continue
        if in_jobs and line and not line.startswith(" ") and not line.startswith("#"):
            if current is not None:
                current.end_line = number - 1
            in_jobs = False
            current = None
            in_job_env = False
            run_indent = None

        if not in_jobs:
            line_jobs[number] = None
            continue

        job_match = JOB_RE.match(line)
        if job_match:
            if current is not None:
                current.end_line = number - 1
            name = job_match.group(1)
            if name in jobs:
                errors.append(f"duplicate workflow job id: {name}")
            current = Job(name=name, start_line=number)
            jobs[name] = current
            in_job_env = False
            run_indent = None
            line_jobs[number] = name
            continue

        line_jobs[number] = current.name if current else None
        if current is None:
            continue

        indent = _leading_spaces(line)
        stripped = line.strip()
        if run_indent is not None:
            if not stripped:
                current.run_lines.append("")
                continue
            if indent > run_indent:
                current.run_lines.append(stripped)
                continue
            run_indent = None

        field_match = JOB_FIELD_RE.match(line)
        if field_match:
            key, raw = field_match.groups()
            in_job_env = key == "env" and raw == ""
            if key == "environment":
                current.environment = _yaml_scalar(raw)
            elif key == "needs":
                current.needs = _parse_needs(raw)
            continue

        if in_job_env:
            if indent <= 4 and stripped:
                in_job_env = False
            else:
                env_match = JOB_ENV_KEY_RE.match(line)
                if env_match:
                    key, raw = env_match.groups()
                    scalar = _yaml_scalar(raw)
                    if scalar is not None:
                        current.env[key] = scalar
                    continue

        if RUN_RE.match(line):
            run_indent = 8

    if current is not None:
        current.end_line = len(lines)
    return jobs, line_jobs, errors


def scan_references(
    root: Path, workflows: list[str]
) -> tuple[list[Reference], dict[str, str], dict[str, dict[str, Job]], list[str]]:
    references: list[Reference] = []
    texts: dict[str, str] = {}
    jobs_by_workflow: dict[str, dict[str, Job]] = {}
    errors: list[str] = []
    for workflow in workflows:
        path = root / workflow
        if not path.is_file():
            errors.append(f"canonical workflow is missing from repository: {workflow}")
            continue
        text = path.read_text(encoding="utf-8")
        texts[workflow] = text
        jobs, line_jobs, parse_errors = parse_jobs(text)
        jobs_by_workflow[workflow] = jobs
        errors.extend(f"{workflow}: {error}" for error in parse_errors)
        for number, line in enumerate(text.splitlines(), start=1):
            for match in SECRET_RE.finditer(line):
                job_name = line_jobs.get(number)
                job = jobs.get(job_name) if job_name else None
                references.append(
                    Reference(
                        workflow=workflow,
                        line=number,
                        job=job_name,
                        environment=job.environment if job else None,
                        secret=match.group(1),
                    )
                )
    return references, texts, jobs_by_workflow, errors


def _parse_structural_guards(contract: DynamicEnvironmentContract) -> tuple[str | None, str | None, str | None, list[str]]:
    dependency: str | None = None
    target_binding: str | None = None
    guard_command: str | None = None
    errors: list[str] = []
    seen_environment = False
    for fragment in contract.required_guard_fragments:
        if fragment.startswith("needs: "):
            candidate = fragment.removeprefix("needs: ").strip()
            if not re.fullmatch(r"[A-Za-z0-9_-]+", candidate) or dependency is not None:
                errors.append(f"invalid/duplicate needs guard fragment: {fragment}")
            else:
                dependency = candidate
        elif fragment.startswith("environment: "):
            value = fragment.removeprefix("environment: ").strip()
            if seen_environment or value != contract.expression:
                errors.append(f"dynamic environment guard must bind exact contract expression: {fragment}")
            seen_environment = True
        elif fragment.startswith("TARGET_ENVIRONMENT: "):
            value = fragment.removeprefix("TARGET_ENVIRONMENT: ").strip()
            if target_binding is not None or value != contract.expression:
                errors.append(f"TARGET_ENVIRONMENT guard must bind exact contract expression: {fragment}")
            else:
                target_binding = value
        elif fragment.startswith('test "$TARGET_ENVIRONMENT"'):
            if guard_command is not None:
                errors.append("duplicate TARGET_ENVIRONMENT guard command")
            else:
                guard_command = fragment
        else:
            errors.append(f"unsupported dynamic environment guard fragment; structural proof required: {fragment}")
    if dependency is None:
        errors.append("dynamic environment contract must declare exactly one needs dependency")
    if not seen_environment:
        errors.append("dynamic environment contract must declare exact environment expression")
    if target_binding is None:
        errors.append("dynamic environment contract must declare exact TARGET_ENVIRONMENT binding")
    if guard_command is None:
        errors.append("dynamic environment contract must declare executable TARGET_ENVIRONMENT guard command")
    return dependency, target_binding, guard_command, errors


def _validate_dynamic_contract(
    contract: DynamicEnvironmentContract,
    bindings: dict[str, Binding],
    references: list[Reference],
    jobs: dict[str, Job],
) -> list[str]:
    errors: list[str] = []
    dependency, _target_binding, guard_command, structural_errors = _parse_structural_guards(contract)
    errors.extend(
        f"dynamic environment contract structural declaration invalid: {contract.credential_id} {contract.workflow}: {error}"
        for error in structural_errors
    )
    if structural_errors:
        return errors

    credential_secrets = {
        name for name, binding in bindings.items()
        if binding.credential_id == contract.credential_id and binding.surface == "github_environment_secret"
    }
    credential_refs = [
        ref for ref in references
        if ref.workflow == contract.workflow and ref.secret in credential_secrets
    ]
    protected_job_names = {
        ref.job for ref in credential_refs
        if ref.job is not None and ref.environment == contract.expression
    }
    if len(protected_job_names) != 1:
        errors.append(
            f"dynamic environment contract must bind exactly one protected secret-consuming job: "
            f"{contract.credential_id} {contract.workflow} jobs={sorted(name for name in protected_job_names if name)}"
        )
        return errors
    protected_name = next(iter(protected_job_names))
    protected = jobs.get(protected_name)
    guard = jobs.get(dependency or "")
    if protected is None or guard is None:
        errors.append(
            f"dynamic environment contract required jobs are missing: {contract.credential_id} {contract.workflow} "
            f"protected={protected_name} guard={dependency}"
        )
        return errors
    if protected.environment != contract.expression:
        errors.append(f"protected job environment expression drifted: {contract.workflow} {protected.name}")
    if dependency not in protected.needs:
        errors.append(
            f"protected job must depend on pre-Environment guard job: {contract.workflow} {protected.name} needs {dependency}"
        )
    if guard.environment is not None:
        errors.append(
            f"pre-Environment guard job must not bind any GitHub Environment: {contract.workflow} {guard.name}"
        )
    if guard.env.get("TARGET_ENVIRONMENT") != contract.expression:
        errors.append(
            f"pre-Environment guard TARGET_ENVIRONMENT must equal contract expression: {contract.workflow} {guard.name}"
        )
    if protected.env.get("TARGET_ENVIRONMENT") != contract.expression:
        errors.append(
            f"protected job TARGET_ENVIRONMENT must equal contract expression: {contract.workflow} {protected.name}"
        )
    if guard_command not in guard.run_lines:
        errors.append(
            f"pre-Environment guard command is not executable in guard job: {contract.workflow} {guard.name}"
        )
    guard_refs = [ref for ref in references if ref.workflow == contract.workflow and ref.job == guard.name]
    if guard_refs:
        errors.append(
            f"pre-Environment guard job must be credential-free: {contract.workflow} {guard.name} "
            f"references={[ref.secret for ref in guard_refs]}"
        )
    return errors


def validate(root: Path) -> list[str]:
    authorities, extension = load_authorities(root)
    workflows = canonical_workflows(load_json(root, REGISTRY))
    bindings, dynamic_contracts, errors = bindings_from_authorities(authorities, extension)
    references, _workflow_texts, jobs_by_workflow, scan_errors = scan_references(root, workflows)
    errors.extend(scan_errors)
    usage: dict[str, set[str]] = {name: set() for name in bindings}

    for (credential_id, workflow), contract in dynamic_contracts.items():
        if workflow not in workflows:
            errors.append(
                f"governed dynamic environment consumer is not a canonical active workflow: {credential_id} {workflow}"
            )
            continue
        errors.extend(
            _validate_dynamic_contract(
                contract,
                bindings,
                references,
                jobs_by_workflow.get(workflow, {}),
            )
        )

    for reference in references:
        binding = bindings.get(reference.secret)
        label = f"{reference.workflow}:{reference.line} secrets.{reference.secret}"
        if reference.job is None:
            errors.append(f"secret reference outside a parsed workflow job is forbidden: {label}")
            continue
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
                "environment_scope": {"kind": "environment", "environments": ["staging", "production"]},
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
        "production_mutation": False,
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
        "    runs-on: ubuntu-latest\n"
        "    env:\n"
        "      TARGET_ENVIRONMENT: ${{ inputs.environment }}\n"
        "    steps:\n"
        "      - name: Guard\n"
        "        run: |\n"
        "          set -euo pipefail\n"
        "          test \"$TARGET_ENVIRONMENT\" = \"staging\" || test \"$TARGET_ENVIRONMENT\" = \"production\"\n"
        "  migrate:\n"
        "    needs: authorize\n"
        "    environment: ${{ inputs.environment }}\n"
        "    env:\n"
        "      TARGET_ENVIRONMENT: ${{ inputs.environment }}\n"
        "      TOKEN: ${{ secrets.PLATFORM_TOKEN }}\n"
        "    steps:\n"
        "      - run: test true\n"
    )

    def has_error(root: Path, needle: str) -> bool:
        return any(needle in error for error in validate(root))

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

        dynamic.write_text(dynamic_ok.replace("    needs: authorize\n", "    # needs: authorize\n"), encoding="utf-8")
        if not has_error(root, "must depend on pre-Environment guard job"):
            return False
        dynamic.write_text(dynamic_ok, encoding="utf-8")

        dynamic.write_text(
            dynamic_ok.replace(
                "          test \"$TARGET_ENVIRONMENT\" = \"staging\" || test \"$TARGET_ENVIRONMENT\" = \"production\"\n",
                "          # test \"$TARGET_ENVIRONMENT\" = \"staging\" || test \"$TARGET_ENVIRONMENT\" = \"production\"\n",
                1,
            ),
            encoding="utf-8",
        )
        if not has_error(root, "guard command is not executable"):
            return False
        dynamic.write_text(dynamic_ok, encoding="utf-8")

        dynamic.write_text(dynamic_ok.replace("  authorize:\n", "  authorize:\n    environment: staging\n", 1), encoding="utf-8")
        if not has_error(root, "guard job must not bind any GitHub Environment"):
            return False
        dynamic.write_text(dynamic_ok, encoding="utf-8")

        dynamic.write_text(
            dynamic_ok.replace(
                "      TARGET_ENVIRONMENT: ${{ inputs.environment }}\n",
                "      TARGET_ENVIRONMENT: ${{ inputs.unbounded }}\n",
                1,
            ),
            encoding="utf-8",
        )
        if not has_error(root, "pre-Environment guard TARGET_ENVIRONMENT"):
            return False
        dynamic.write_text(dynamic_ok, encoding="utf-8")

        dynamic.write_text(dynamic_ok.replace("environment: ${{ inputs.environment }}", "environment: ${{ inputs.unbounded }}", 1), encoding="utf-8")
        if not has_error(root, "wrong secret environment"):
            return False
        dynamic.write_text(dynamic_ok, encoding="utf-8")

        workflow.write_text(
            "jobs:\n  observe:\n    environment: staging\n    env:\n      TOKEN: ${{ secrets.UNKNOWN_TOKEN }}\n",
            encoding="utf-8",
        )
        if not has_error(root, "unclassified permanent workflow secret reference"):
            return False

        workflow.write_text(
            "jobs:\n  observe:\n    environment: production\n    env:\n      TOKEN: ${{ secrets.OBSERVE_TOKEN }}\n",
            encoding="utf-8",
        )
        if not has_error(root, "wrong secret environment"):
            return False

        extension["credentials"][0]["consumers"] = [".github/workflows/other.yml"]
        (root / AR11_EXTENSION).write_text(json.dumps(extension), encoding="utf-8")
        workflow.write_text(
            "jobs:\n  observe:\n    environment: staging\n    env:\n      TOKEN: ${{ secrets.OBSERVE_TOKEN }}\n",
            encoding="utf-8",
        )
        if not has_error(root, "wrong secret consumer"):
            return False

        extension["credentials"][0]["consumers"] = [".github/workflows/ok.yml"]
        extension["existing_credential_extensions"][0]["dynamic_environment_contracts"][0]["allowed_environments"] = ["staging", "production", "preview"]
        (root / AR11_EXTENSION).write_text(json.dumps(extension), encoding="utf-8")
        if not has_error(root, "dynamic environment scope must exactly match"):
            return False

        extension["existing_credential_extensions"][0]["dynamic_environment_contracts"][0]["allowed_environments"] = ["staging", "production"]
        extension["production_mutation"] = True
        (root / AR11_EXTENSION).write_text(json.dumps(extension), encoding="utf-8")
        try:
            validate(root)
        except ValueError as error:
            if "production_mutation=false" not in str(error):
                return False
        else:
            return False

    print("Workflow secret authority structural negative fixtures passed.")
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
    print("Canonical workflow secret references match credential authority and structural environment boundaries.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
