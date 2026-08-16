#!/usr/bin/env python3
from __future__ import annotations

import json
from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(
            f"{path}: expected exactly one guarded replacement, observed {count}: {old!r}"
        )
    file.write_text(text.replace(old, new, 1), encoding="utf-8")


def patch_authority() -> None:
    path = Path("architecture/credential-authority-ar8b.json")
    payload = json.loads(path.read_text(encoding="utf-8"))
    cloudflare = payload["ar8c_operational_lifecycle"]["hosted_reconciliation"]["cloudflare"]
    expected = {
        "accepted_main_only": True,
        "audit_environment": "staging",
        "read_only": True,
        "api_token_binding": "CLOUDFLARE_API_TOKEN",
        "verify_endpoint": "GET /user/tokens/verify",
        "required_token_status": "active",
        "worker_secret_contract_source": "wrangler.secrets.required",
    }
    if cloudflare != expected:
        raise SystemExit("AR-8C Cloudflare hosted reconciliation baseline drifted")
    cloudflare["deploy_manifest_binding"] = "CLOUDFLARE_DEPLOY_MANIFEST_JSON"
    cloudflare["secret_binding_metadata_endpoint"] = (
        "GET /accounts/{account_id}/workers/scripts/{script_name}/secrets/{secret_name}"
    )
    path.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def patch_node_auditor() -> None:
    path = ".github/scripts/credential-lifecycle.mjs"
    replace_once(
        path,
        "      expect(cloudflare.worker_secret_contract_source === 'wrangler.secrets.required', 'Worker secret contract must remain Wrangler secrets.required');\n",
        "      expect(cloudflare.worker_secret_contract_source === 'wrangler.secrets.required', 'Worker secret contract must remain Wrangler secrets.required');\n"
        "      expect(cloudflare.deploy_manifest_binding === 'CLOUDFLARE_DEPLOY_MANIFEST_JSON', 'Cloudflare deploy manifest binding drifted');\n"
        "      expect(cloudflare.secret_binding_metadata_endpoint === 'GET /accounts/{account_id}/workers/scripts/{script_name}/secrets/{secret_name}', 'Cloudflare Worker secret metadata endpoint drifted');\n",
    )
    replace_once(
        path,
        "    { name: 'missing hosted binding', expected: 'production environment secret', mutate: (copy) => { copy.ar8c_operational_lifecycle.hosted_reconciliation.github.required_environment_secrets.production.pop(); } },\n",
        "    { name: 'missing hosted binding', expected: 'production environment secret', mutate: (copy) => { copy.ar8c_operational_lifecycle.hosted_reconciliation.github.required_environment_secrets.production.pop(); } },\n"
        "    { name: 'missing Cloudflare deploy manifest binding', expected: 'deploy manifest binding', mutate: (copy) => { delete copy.ar8c_operational_lifecycle.hosted_reconciliation.cloudflare.deploy_manifest_binding; } },\n"
        "    { name: 'wrong Cloudflare secret metadata endpoint', expected: 'secret metadata endpoint', mutate: (copy) => { copy.ar8c_operational_lifecycle.hosted_reconciliation.cloudflare.secret_binding_metadata_endpoint = 'POST /forbidden'; } },\n",
    )

    old = '''async function cloudflareLive(authority) {
  const lifecycle = authority.ar8c_operational_lifecycle;
  const token = process.env.CLOUDFLARE_API_TOKEN;
  if (!token) return ['CLOUDFLARE_API_TOKEN is required only for accepted-main staging metadata reconciliation'];
  if (process.env.GITHUB_EVENT_NAME === 'pull_request') return ['Cloudflare credential reconciliation is forbidden for pull_request events'];
  if (process.env.GITHUB_REF !== 'refs/heads/main') return [`Cloudflare credential reconciliation requires refs/heads/main; observed ${process.env.GITHUB_REF ?? '<unset>'}`];
  if (process.env.AR8C_AUDIT_ENVIRONMENT !== lifecycle.hosted_reconciliation.cloudflare.audit_environment) {
    return [`Cloudflare audit environment must be ${lifecycle.hosted_reconciliation.cloudflare.audit_environment}`];
  }
  const response = await fetch('https://api.cloudflare.com/client/v4/user/tokens/verify', {
    headers: { Authorization: `Bearer ${token}` },
  });
  if (!response.ok) return [`Cloudflare token verification failed closed: HTTP ${response.status}`];
  const payload = await response.json();
  if (payload?.success !== true || payload?.result?.status !== lifecycle.hosted_reconciliation.cloudflare.required_token_status) {
    return [`Cloudflare API token status must be ${lifecycle.hosted_reconciliation.cloudflare.required_token_status}`];
  }
  return [];
}
'''
    new = '''function boundedManifestString(document, name, label) {
  const value = document?.[name];
  if (typeof value !== 'string' || value.length === 0 || value.length > 512) {
    throw new Error(`${label}.${name} must be one bounded non-empty string`);
  }
  return value;
}

function stagingDeployTargets(raw, environment) {
  if (!raw) throw new Error('CLOUDFLARE_DEPLOY_MANIFEST_JSON is required only for accepted-main staging metadata reconciliation');
  let payload;
  try {
    payload = JSON.parse(raw);
  } catch {
    throw new Error('protected Cloudflare deploy manifest is not valid JSON');
  }
  if (!object(payload) || payload.schema_version !== 1 || payload.environment !== environment) {
    throw new Error('protected Cloudflare deploy manifest environment/schema mismatch');
  }
  const control = payload.control_plane;
  const resolver = payload.resolver;
  if (!object(control) || !object(resolver)) throw new Error('protected Cloudflare deploy manifest worker sections are missing');
  const accountId = boundedManifestString(control, 'account_id', 'control_plane');
  const controlWorker = boundedManifestString(control, 'worker_name', 'control_plane');
  const resolverAccount = boundedManifestString(resolver, 'account_id', 'resolver');
  const resolverWorker = boundedManifestString(resolver, 'worker_name', 'resolver');
  const resolverService = boundedManifestString(control, 'mailbox_secret_resolver_service', 'control_plane');
  if (resolverAccount !== accountId) throw new Error('resolver and control-plane Cloudflare accounts differ');
  if (!controlWorker.includes(environment)) throw new Error('control-plane Worker is not isolated to the selected environment');
  if (resolverWorker !== `mailbox-secret-resolver-${environment}` || resolverWorker !== resolverService) {
    throw new Error('resolver Worker identity differs from the accepted service binding');
  }
  return {
    control: { accountId, workerName: controlWorker },
    resolver: { accountId, workerName: resolverWorker },
  };
}

async function cloudflareResult(apiPath, token) {
  const response = await fetch(`https://api.cloudflare.com/client/v4${apiPath}`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  if (!response.ok) throw new Error(`Cloudflare metadata API failed closed: HTTP ${response.status}`);
  const payload = await response.json();
  if (payload?.success !== true) throw new Error('Cloudflare metadata API returned success=false');
  return payload.result;
}

function requiredWorkerSecrets(config, environment, label) {
  const required = config?.env?.[environment]?.secrets?.required;
  if (!Array.isArray(required) || required.length === 0 || required.some((value) => typeof value !== 'string' || value.length === 0)) {
    throw new Error(`${label} ${environment} secrets.required is missing or malformed`);
  }
  if (new Set(required).size !== required.length) throw new Error(`${label} ${environment} secrets.required contains duplicates`);
  return required;
}

async function verifyRemoteWorkerSecrets(target, required, token, label) {
  const errors = [];
  for (const name of required) {
    const apiPath = `/accounts/${encodeURIComponent(target.accountId)}/workers/scripts/${encodeURIComponent(target.workerName)}/secrets/${encodeURIComponent(name)}`;
    try {
      const result = await cloudflareResult(apiPath, token);
      if (result?.name !== name) errors.push(`required Cloudflare ${label} Worker secret metadata name mismatch: ${name}`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      errors.push(`required Cloudflare ${label} Worker secret metadata is unavailable: ${name}; ${message}`);
    }
  }
  return errors;
}

async function cloudflareLive(root, authority) {
  const lifecycle = authority.ar8c_operational_lifecycle;
  const cloudflare = lifecycle.hosted_reconciliation.cloudflare;
  const token = process.env.CLOUDFLARE_API_TOKEN;
  const deployManifest = process.env.CLOUDFLARE_DEPLOY_MANIFEST_JSON;
  if (!token) return ['CLOUDFLARE_API_TOKEN is required only for accepted-main staging metadata reconciliation'];
  if (process.env.GITHUB_EVENT_NAME === 'pull_request') return ['Cloudflare credential reconciliation is forbidden for pull_request events'];
  if (process.env.GITHUB_REF !== 'refs/heads/main') return [`Cloudflare credential reconciliation requires refs/heads/main; observed ${process.env.GITHUB_REF ?? '<unset>'}`];
  const environment = cloudflare.audit_environment;
  if (process.env.AR8C_AUDIT_ENVIRONMENT !== environment) return [`Cloudflare audit environment must be ${environment}`];

  let targets;
  let controlConfig;
  let resolverConfig;
  try {
    targets = stagingDeployTargets(deployManifest, environment);
    controlConfig = await loadJson(root, CONTROL_CONFIG_RELATIVE);
    resolverConfig = await loadJson(root, RESOLVER_CONFIG_RELATIVE);
  } catch (error) {
    return [error instanceof Error ? error.message : String(error)];
  }

  let tokenResult;
  try {
    tokenResult = await cloudflareResult('/user/tokens/verify', token);
  } catch (error) {
    return [error instanceof Error ? error.message : String(error)];
  }
  if (tokenResult?.status !== cloudflare.required_token_status) {
    return [`Cloudflare API token status must be ${cloudflare.required_token_status}`];
  }

  let controlRequired;
  let resolverRequired;
  try {
    controlRequired = requiredWorkerSecrets(controlConfig, environment, 'control-plane');
    resolverRequired = requiredWorkerSecrets(resolverConfig, environment, 'resolver');
  } catch (error) {
    return [error instanceof Error ? error.message : String(error)];
  }

  return [
    ...(await verifyRemoteWorkerSecrets(targets.control, controlRequired, token, 'control-plane')),
    ...(await verifyRemoteWorkerSecrets(targets.resolver, resolverRequired, token, 'resolver')),
  ];
}
'''
    replace_once(path, old, new)
    replace_once(path, "    const errors = await cloudflareLive(authority);\n", "    const errors = await cloudflareLive(root, authority);\n")
    replace_once(
        path,
        "    console.log('AR-8C Cloudflare operational API credential is active in read-only staging audit.');\n",
        "    console.log('AR-8C Cloudflare API credential and required Worker secret binding metadata match staging authority.');\n",
    )


def patch_governance_workflow() -> None:
    replace_once(
        ".github/workflows/github-governance-gate.yml",
        "          CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}\n          AR8C_AUDIT_ENVIRONMENT: staging\n",
        "          CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}\n          CLOUDFLARE_DEPLOY_MANIFEST_JSON: ${{ secrets.CLOUDFLARE_DEPLOY_MANIFEST_JSON }}\n          AR8C_AUDIT_ENVIRONMENT: staging\n",
    )


def patch_projection_helpers() -> None:
    for path in (
        "scripts/generate-architecture-inventory.py",
        "scripts/check-documentation-authority.py",
    ):
        replace_once(
            path,
            '                "worker_secret_contract_source": cloudflare.get("worker_secret_contract_source"),\n',
            '                "worker_secret_contract_source": cloudflare.get("worker_secret_contract_source"),\n'
            '                "deploy_manifest_binding": cloudflare.get("deploy_manifest_binding"),\n'
            '                "secret_binding_metadata_endpoint": cloudflare.get("secret_binding_metadata_endpoint"),\n',
        )


def main() -> None:
    patch_authority()
    patch_node_auditor()
    patch_governance_workflow()
    patch_projection_helpers()


if __name__ == "__main__":
    main()
