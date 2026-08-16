#!/usr/bin/env python3
from __future__ import annotations

import json
from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected exactly one occurrence, observed {count}: {old!r}")
    file.write_text(text.replace(old, new, 1), encoding="utf-8")


def patch_authority() -> None:
    path = Path("architecture/credential-authority-ar8b.json")
    payload = json.loads(path.read_text(encoding="utf-8"))
    github = payload["ar8c_operational_lifecycle"]["hosted_reconciliation"]["github"]
    if "live_audit_environments" in github:
        raise SystemExit("live_audit_environments already exists")
    github["live_audit_environments"] = ["staging"]
    path.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def patch_validator() -> None:
    path = ".github/scripts/credential-lifecycle.mjs"
    replace_once(
        path,
        "      expect(github.executor_binding === 'GH_ADMIN_OPERATOR_TOKEN', 'GitHub hosted reconciliation executor must be GH_ADMIN_OPERATOR_TOKEN');\n",
        "      expect(github.executor_binding === 'GH_ADMIN_OPERATOR_TOKEN', 'GitHub hosted reconciliation executor must be GH_ADMIN_OPERATOR_TOKEN');\n"
        "      expect(sameStringSet(github.live_audit_environments, ['staging']), 'GitHub live audit environments must be staging-only during AR-0..AR-17');\n",
    )
    replace_once(
        path,
        "    { name: 'missing hosted binding', expected: 'production environment secret', mutate: (copy) => { copy.ar8c_operational_lifecycle.hosted_reconciliation.github.required_environment_secrets.production.pop(); } },\n",
        "    { name: 'missing hosted binding', expected: 'production environment secret', mutate: (copy) => { copy.ar8c_operational_lifecycle.hosted_reconciliation.github.required_environment_secrets.production.pop(); } },\n"
        "    { name: 'production live audit forbidden during AR', expected: 'live audit environments', mutate: (copy) => { copy.ar8c_operational_lifecycle.hosted_reconciliation.github.live_audit_environments.push('production'); } },\n",
    )
    old = """  for (const name of lifecycle.hosted_reconciliation.github.required_repository_secrets) {
    if (!repoSecrets.has(name)) errors.push(`required GitHub repository secret metadata is missing: ${name}`);
  }
  for (const environment of ['staging', 'production']) {
"""
    new = """  for (const name of lifecycle.hosted_reconciliation.github.required_repository_secrets) {
    if (!repoSecrets.has(name)) errors.push(`required GitHub repository secret metadata is missing: ${name}`);
  }
  for (const environment of lifecycle.hosted_reconciliation.github.live_audit_environments) {
"""
    replace_once(path, old, new)


def patch_projection(path: str) -> None:
    replace_once(
        path,
        '                "executor_binding": github.get("executor_binding"),\n',
        '                "executor_binding": github.get("executor_binding"),\n'
        '                "live_audit_environments": github.get("live_audit_environments"),\n',
    )


def main() -> None:
    patch_authority()
    patch_validator()
    patch_projection("scripts/generate-architecture-inventory.py")
    patch_projection("scripts/check-documentation-authority.py")


if __name__ == "__main__":
    main()
