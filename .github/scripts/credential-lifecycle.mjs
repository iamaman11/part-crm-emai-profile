#!/usr/bin/env node

import { readFile } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = path.resolve(SCRIPT_DIR, '..', '..');
const AUTHORITY_RELATIVE = 'architecture/credential-authority-ar8b.json';
const CONTROL_CONFIG_RELATIVE = 'deploy/cloudflare/wrangler.jsonc';
const RESOLVER_CONFIG_RELATIVE = 'deploy/cloudflare/mailbox-secret-resolver.wrangler.jsonc';
const ACCEPTED_BASE = '4e4d1c25226384858ca8905377ee155bedabc6d4';
const IMPLEMENTATION_ISSUE = 314;
const EXPECTED_STATUS = 'CANDIDATE_AR8C_OPERATIONAL_CREDENTIAL_LIFECYCLE';
const CANONICAL_ENVIRONMENTS = ['rehearsal', 'staging', 'production'];
const STAGE_ORDER = ['issue_or_import', 'validate', 'bind', 'switch', 'verify', 'revoke_previous'];
const EXPECTED_REPOSITORY_SECRETS = ['GOVERNANCE_AUDIT_TOKEN', 'GH_ADMIN_OPERATOR_TOKEN'];
const EXPECTED_ENVIRONMENT_SECRETS = [
  'CLOUDFLARE_ACCESS_CLIENT_ID',
  'CLOUDFLARE_ACCESS_CLIENT_SECRET',
  'CLOUDFLARE_API_TOKEN',
  'CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON',
  'CLOUDFLARE_DEPLOY_MANIFEST_JSON',
  'CLOUDFLARE_RESOLVER_SECRETS_JSON',
];
const FORBIDDEN_KEYS = new Set([
  'value',
  'plaintext',
  'secret_value',
  'token_value',
  'credential_value',
  'encrypted_value',
  'private_key',
]);

function sameStringSet(actual, expected) {
  if (!Array.isArray(actual) || actual.some((value) => typeof value !== 'string')) return false;
  const actualSet = new Set(actual);
  const expectedSet = new Set(expected);
  if (actualSet.size !== actual.length || expectedSet.size !== expected.length) return false;
  if (actualSet.size !== expectedSet.size) return false;
  return [...actualSet].every((value) => expectedSet.has(value));
}

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

function object(value) {
  return value && typeof value === 'object' && !Array.isArray(value);
}

function findForbiddenKey(value, prefix = '') {
  if (Array.isArray(value)) {
    for (let index = 0; index < value.length; index += 1) {
      const found = findForbiddenKey(value[index], `${prefix}[${index}]`);
      if (found) return found;
    }
    return null;
  }
  if (!object(value)) return null;
  for (const [key, nested] of Object.entries(value)) {
    const normalized = key.toLowerCase();
    if (FORBIDDEN_KEYS.has(normalized)) return prefix ? `${prefix}.${key}` : key;
    const found = findForbiddenKey(nested, prefix ? `${prefix}.${key}` : key);
    if (found) return found;
  }
  return null;
}

async function loadJson(root, relative) {
  const text = await readFile(path.join(root, relative), 'utf8');
  const payload = JSON.parse(text);
  if (!object(payload)) throw new Error(`${relative} must contain one JSON object`);
  return payload;
}

function ar8cCredentialIds(authority) {
  return (authority?.credentials ?? [])
    .filter((credential) => credential?.future_cutover === 'AR-8C')
    .map((credential) => credential?.id)
    .filter((value) => typeof value === 'string')
    .sort();
}

function validateLifecycle(authority) {
  const errors = [];
  const expect = (condition, message) => {
    if (!condition) errors.push(message);
  };

  expect(authority?.schema_version === 1, 'credential authority schema_version must remain 1');
  expect(authority?.status === 'ACCEPTED_AR8B_CREDENTIAL_METADATA_AUTHORITY', 'AR-8B credential metadata authority must remain accepted');
  expect(authority?.metadata_only === true, 'credential authority must remain metadata_only');
  expect(sameStringSet(authority?.canonical_environments, CANONICAL_ENVIRONMENTS), 'canonical environments must remain rehearsal, staging, production');
  expect(authority?.invariants?.plaintext_in_git === 'FORBIDDEN', 'plaintext_in_git must remain forbidden');
  expect(authority?.invariants?.mutable_authorities_per_concern === 1, 'mutable authorities per concern must remain exactly one');
  expect(authority?.invariants?.production_mutation === false, 'production mutation must remain false during AR-8');
  expect(authority?.invariants?.ar9_blocked === true, 'AR-9 must remain blocked');

  const lifecycle = authority?.ar8c_operational_lifecycle;
  expect(object(lifecycle), 'ar8c_operational_lifecycle must exist');
  if (!object(lifecycle)) return errors;

  expect(lifecycle.schema_version === 1, 'AR-8C lifecycle schema_version must be 1');
  expect(lifecycle.status === EXPECTED_STATUS, `AR-8C lifecycle status must be ${EXPECTED_STATUS}`);
  expect(lifecycle.implementation_issue === IMPLEMENTATION_ISSUE, 'AR-8C implementation issue must be #314');
  expect(lifecycle.accepted_base === ACCEPTED_BASE, 'AR-8C accepted_base drifted');
  expect(lifecycle.metadata_only === true, 'AR-8C lifecycle must be metadata_only');
  expect(lifecycle.production_mutation === false, 'AR-8C production mutation must be false');
  expect(lifecycle.opsctl_mutation === false, 'AR-8C must not grant mutable opsctl credential authority');
  expect(lifecycle.pull_request_privileged_exposure === false, 'privileged credential exposure to pull_request code is forbidden');
  expect(sameStringSet(lifecycle.stage_order, STAGE_ORDER), 'AR-8C stage_order must contain the exact lifecycle stages');
  expect(JSON.stringify(lifecycle.stage_order) === JSON.stringify(STAGE_ORDER), 'AR-8C lifecycle stages must preserve issue/import -> validate -> bind -> switch -> verify -> revoke-previous order');

  const expectedIds = ar8cCredentialIds(authority);
  const concerns = Array.isArray(lifecycle.concerns) ? lifecycle.concerns : [];
  const concernIds = concerns.map((concern) => concern?.id).filter((value) => typeof value === 'string');
  expect(concerns.length === concernIds.length, 'every AR-8C concern must have a string id');
  expect(new Set(concernIds).size === concernIds.length, 'AR-8C concern ids must be unique');
  expect(sameStringSet([...concernIds].sort(), expectedIds), 'AR-8C concerns must equal exactly the credentials assigned future_cutover=AR-8C');

  for (const concern of concerns) {
    if (!object(concern)) {
      errors.push('AR-8C concerns must be objects');
      continue;
    }
    const label = `AR-8C concern ${concern.id ?? '<missing>'}`;
    expect(typeof concern.allowed_mutator === 'string' && concern.allowed_mutator.length > 0, `${label} must have exactly one allowed_mutator string`);
    expect(!Array.isArray(concern.allowed_mutator), `${label} allowed_mutator must not be an array`);
    expect(typeof concern.exposure_boundary === 'string' && concern.exposure_boundary.length > 0, `${label} exposure_boundary is required`);
    expect(typeof concern.recovery === 'string' && concern.recovery.length > 0, `${label} recovery is required`);
    expect(Array.isArray(concern.metadata_evidence) && concern.metadata_evidence.length > 0, `${label} metadata_evidence is required`);
    expect(typeof concern.externally_issued === 'boolean', `${label} externally_issued must be boolean`);
    if (concern.externally_issued === true) {
      expect(typeof concern.external_issuance_constraint === 'string' && concern.external_issuance_constraint.length > 0, `${label} externally issued credential requires an external_issuance_constraint`);
    }
    expect(object(concern.stages), `${label} stages must be an object`);
    if (object(concern.stages)) {
      expect(sameStringSet(Object.keys(concern.stages), STAGE_ORDER), `${label} stages must define every lifecycle stage exactly once`);
      for (const stage of STAGE_ORDER) {
        expect(typeof concern.stages[stage] === 'string' && concern.stages[stage].length > 0, `${label} stage ${stage} must be non-empty`);
      }
    }
    expect(typeof concern.revoke_previous_required === 'boolean', `${label} revoke_previous_required must be boolean`);
    if (concern.revoke_previous_required === true) {
      expect(concern.revoke_previous_requires_verified_replacement === true, `${label} must require replacement verification before revoking previous`);
    }
  }

  const forbidden = findForbiddenKey(lifecycle);
  expect(!forbidden, `AR-8C lifecycle contains forbidden plaintext/value-shaped field ${forbidden ?? ''}`.trim());

  const hosted = lifecycle.hosted_reconciliation;
  expect(object(hosted), 'AR-8C hosted_reconciliation must exist');
  if (object(hosted)) {
    const github = hosted.github;
    expect(object(github), 'AR-8C GitHub hosted reconciliation must exist');
    if (object(github)) {
      expect(github.accepted_main_only === true, 'GitHub hosted reconciliation must be accepted-main only');
      expect(github.pull_request_exposure === false, 'GitHub hosted reconciliation must forbid pull_request exposure');
      expect(github.metadata_only === true, 'GitHub hosted reconciliation must be metadata-only');
      expect(github.readback_values === false, 'GitHub hosted reconciliation must forbid secret value readback');
      expect(github.executor_binding === 'GH_ADMIN_OPERATOR_TOKEN', 'GitHub hosted reconciliation executor must be GH_ADMIN_OPERATOR_TOKEN');
      expect(sameStringSet(github.required_repository_secrets, EXPECTED_REPOSITORY_SECRETS), 'GitHub repository secret metadata requirements drifted');
      for (const environment of ['staging', 'production']) {
        expect(sameStringSet(github.required_environment_secrets?.[environment], EXPECTED_ENVIRONMENT_SECRETS), `GitHub ${environment} environment secret metadata requirements drifted`);
      }
      expect(!('rehearsal' in (github.required_environment_secrets ?? {})), 'AR-8C must not invent rehearsal secret bindings absent from accepted authority');
    }

    const cloudflare = hosted.cloudflare;
    expect(object(cloudflare), 'AR-8C Cloudflare hosted reconciliation must exist');
    if (object(cloudflare)) {
      expect(cloudflare.accepted_main_only === true, 'Cloudflare hosted reconciliation must be accepted-main only');
      expect(cloudflare.audit_environment === 'staging', 'Cloudflare live audit must use staging, not production');
      expect(cloudflare.read_only === true, 'Cloudflare hosted reconciliation must be read-only');
      expect(cloudflare.api_token_binding === 'CLOUDFLARE_API_TOKEN', 'Cloudflare audit token binding drifted');
      expect(cloudflare.verify_endpoint === 'GET /user/tokens/verify', 'Cloudflare token verification endpoint drifted');
      expect(cloudflare.required_token_status === 'active', 'Cloudflare token status must be active');
      expect(cloudflare.worker_secret_contract_source === 'wrangler.secrets.required', 'Worker secret contract must remain Wrangler secrets.required');
    }
  }

  return errors;
}

function validateWranglerClassification(authority, controlConfig, resolverConfig) {
  const errors = [];
  const classified = new Set();
  for (const credential of authority?.credentials ?? []) {
    for (const binding of credential?.bindings ?? []) {
      if (binding?.surface === 'cloudflare_worker_secret' && typeof binding?.name === 'string') {
        classified.add(binding.name);
      }
    }
  }
  for (const [label, config] of [['control-plane', controlConfig], ['resolver', resolverConfig]]) {
    for (const environment of ['staging', 'production']) {
      const required = config?.env?.[environment]?.secrets?.required;
      if (!Array.isArray(required) || required.some((value) => typeof value !== 'string')) {
        errors.push(`${label} ${environment} must declare secrets.required`);
        continue;
      }
      if (new Set(required).size !== required.length) errors.push(`${label} ${environment} secrets.required contains duplicates`);
      for (const name of required) {
        if (!classified.has(name)) errors.push(`${label} ${environment} required Worker secret ${name} is not classified by the credential authority`);
      }
    }
  }
  return errors;
}

async function validateAll(root, authorityOverride = null) {
  const authority = authorityOverride ?? await loadJson(root, AUTHORITY_RELATIVE);
  const controlConfig = await loadJson(root, CONTROL_CONFIG_RELATIVE);
  const resolverConfig = await loadJson(root, RESOLVER_CONFIG_RELATIVE);
  return [...validateLifecycle(authority), ...validateWranglerClassification(authority, controlConfig, resolverConfig)];
}

function report(errors) {
  for (const error of errors) console.error(error);
  return errors.length === 0;
}

async function selfTest(root, authority) {
  const baseline = await validateAll(root, authority);
  if (baseline.length !== 0) {
    console.error('AR-8C self-test requires a valid baseline authority');
    return report(baseline);
  }
  const fixtures = [
    { name: 'missing concern', expected: 'concerns must equal', mutate: (copy) => { copy.ar8c_operational_lifecycle.concerns.pop(); } },
    { name: 'duplicate concern', expected: 'ids must be unique', mutate: (copy) => { copy.ar8c_operational_lifecycle.concerns.push(clone(copy.ar8c_operational_lifecycle.concerns[0])); } },
    { name: 'plaintext field', expected: 'forbidden plaintext', mutate: (copy) => { copy.ar8c_operational_lifecycle.concerns[0].value = 'forbidden'; } },
    { name: 'stage reorder', expected: 'preserve issue/import', mutate: (copy) => { [copy.ar8c_operational_lifecycle.stage_order[4], copy.ar8c_operational_lifecycle.stage_order[5]] = [copy.ar8c_operational_lifecycle.stage_order[5], copy.ar8c_operational_lifecycle.stage_order[4]]; } },
    { name: 'revoke before verify', expected: 'replacement verification', mutate: (copy) => { const concern = copy.ar8c_operational_lifecycle.concerns.find((item) => item.revoke_previous_required); concern.revoke_previous_requires_verified_replacement = false; } },
    { name: 'dual mutator', expected: 'allowed_mutator string', mutate: (copy) => { copy.ar8c_operational_lifecycle.concerns[0].allowed_mutator = ['one', 'two']; } },
    { name: 'missing issuance ceremony', expected: 'external_issuance_constraint', mutate: (copy) => { const concern = copy.ar8c_operational_lifecycle.concerns.find((item) => item.externally_issued); delete concern.external_issuance_constraint; } },
    { name: 'pull request privilege leak', expected: 'pull_request exposure', mutate: (copy) => { copy.ar8c_operational_lifecycle.hosted_reconciliation.github.pull_request_exposure = true; } },
    { name: 'mutable opsctl', expected: 'mutable opsctl', mutate: (copy) => { copy.ar8c_operational_lifecycle.opsctl_mutation = true; } },
    { name: 'production mutation', expected: 'production mutation', mutate: (copy) => { copy.ar8c_operational_lifecycle.production_mutation = true; } },
    { name: 'wrong environment alias', expected: 'staging environment secret', mutate: (copy) => { copy.ar8c_operational_lifecycle.hosted_reconciliation.github.required_environment_secrets.prod = copy.ar8c_operational_lifecycle.hosted_reconciliation.github.required_environment_secrets.staging; delete copy.ar8c_operational_lifecycle.hosted_reconciliation.github.required_environment_secrets.staging; } },
    { name: 'missing hosted binding', expected: 'production environment secret', mutate: (copy) => { copy.ar8c_operational_lifecycle.hosted_reconciliation.github.required_environment_secrets.production.pop(); } },
  ];
  for (const fixture of fixtures) {
    const copy = clone(authority);
    fixture.mutate(copy);
    const errors = await validateAll(root, copy);
    if (errors.length === 0 || !errors.some((error) => error.toLowerCase().includes(fixture.expected.toLowerCase()))) {
      console.error(`negative fixture ${fixture.name} was not rejected as expected: ${JSON.stringify(errors)}`);
      return false;
    }
  }
  console.log('AR-8C operational credential lifecycle negative fixtures passed.');
  return true;
}

async function githubJson(apiPath, token) {
  const response = await fetch(`https://api.github.com${apiPath}`, {
    headers: {
      Accept: 'application/vnd.github+json',
      Authorization: `Bearer ${token}`,
      'User-Agent': 'part-crm-ar8c-credential-audit',
      'X-GitHub-Api-Version': '2022-11-28',
    },
  });
  if (!response.ok) throw new Error(`GitHub metadata API ${apiPath} failed closed: HTTP ${response.status}`);
  return response.json();
}

function secretNames(payload) {
  return new Set((payload?.secrets ?? []).map((secret) => secret?.name).filter((value) => typeof value === 'string'));
}

async function githubLive(authority) {
  const lifecycle = authority.ar8c_operational_lifecycle;
  const token = process.env.GH_ADMIN_OPERATOR_TOKEN;
  const repository = process.env.GITHUB_REPOSITORY || 'iamaman11/part-crm-emai-profile';
  if (!token) return ['GH_ADMIN_OPERATOR_TOKEN is required only for accepted-main metadata reconciliation'];
  if (process.env.GITHUB_EVENT_NAME === 'pull_request') return ['privileged GitHub credential reconciliation is forbidden for pull_request events'];
  if (process.env.GITHUB_REF !== 'refs/heads/main') return [`GitHub credential reconciliation requires refs/heads/main; observed ${process.env.GITHUB_REF ?? '<unset>'}`];
  const [owner, repo] = repository.split('/');
  if (!owner || !repo) return ['GITHUB_REPOSITORY must be owner/name'];
  const errors = [];
  const repoSecrets = secretNames(await githubJson(`/repos/${owner}/${repo}/actions/secrets?per_page=100`, token));
  for (const name of lifecycle.hosted_reconciliation.github.required_repository_secrets) {
    if (!repoSecrets.has(name)) errors.push(`required GitHub repository secret metadata is missing: ${name}`);
  }
  for (const environment of ['staging', 'production']) {
    const encoded = encodeURIComponent(environment);
    const payload = await githubJson(`/repos/${owner}/${repo}/environments/${encoded}/secrets?per_page=100`, token);
    const names = secretNames(payload);
    for (const name of lifecycle.hosted_reconciliation.github.required_environment_secrets[environment]) {
      if (!names.has(name)) errors.push(`required GitHub ${environment} environment secret metadata is missing: ${name}`);
    }
  }
  return errors;
}

async function cloudflareLive(authority) {
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

function parseArgs(argv) {
  const command = argv[2] ?? 'contract';
  let root = DEFAULT_ROOT;
  for (let index = 3; index < argv.length; index += 1) {
    if (argv[index] === '--root') {
      if (!argv[index + 1]) throw new Error('--root requires a value');
      root = path.resolve(argv[index + 1]);
      index += 1;
      continue;
    }
    throw new Error(`unknown argument: ${argv[index]}`);
  }
  return { command, root };
}

async function main() {
  const { command, root } = parseArgs(process.argv);
  const authority = await loadJson(root, AUTHORITY_RELATIVE);
  if (command === 'contract') {
    const errors = await validateAll(root, authority);
    if (!report(errors)) return 1;
    console.log('AR-8C operational credential lifecycle contract is internally consistent.');
    return 0;
  }
  if (command === 'self-test') return (await selfTest(root, authority)) ? 0 : 1;
  if (command === 'github-live') {
    const contractErrors = await validateAll(root, authority);
    if (!report(contractErrors)) return 1;
    const errors = await githubLive(authority);
    if (!report(errors)) return 1;
    console.log('AR-8C GitHub credential binding metadata matches the accepted authority.');
    return 0;
  }
  if (command === 'cloudflare-live') {
    const contractErrors = await validateAll(root, authority);
    if (!report(contractErrors)) return 1;
    const errors = await cloudflareLive(authority);
    if (!report(errors)) return 1;
    console.log('AR-8C Cloudflare operational API credential is active in read-only staging audit.');
    return 0;
  }
  console.error(`unknown command: ${command}; expected contract, self-test, github-live, or cloudflare-live`);
  return 2;
}

main()
  .then((code) => { process.exitCode = code; })
  .catch((error) => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  });
