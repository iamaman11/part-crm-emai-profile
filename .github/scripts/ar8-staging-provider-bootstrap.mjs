#!/usr/bin/env node

import { readFile } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = path.resolve(SCRIPT_DIR, '..', '..');
const CONTRACT_RELATIVE = 'architecture/ar8-staging-provider-bootstrap-contract.json';
const CREDENTIAL_AUTHORITY_RELATIVE = 'architecture/credential-authority-ar8b.json';
const BOOTSTRAP_AUTHORITY_RELATIVE = 'architecture/pre2j-d3-resolver-bootstrap-authority.json';
const EXPECTED_OUTPUTS = [
  'CLOUDFLARE_ACCESS_CLIENT_ID',
  'CLOUDFLARE_ACCESS_CLIENT_SECRET',
  'CLOUDFLARE_API_TOKEN',
  'CLOUDFLARE_DEPLOY_MANIFEST_JSON',
];
const EXPECTED_TRANSITIONAL_BUNDLES = [
  'CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON',
  'CLOUDFLARE_RESOLVER_SECRETS_JSON',
];
const EXPECTED_RUNTIME_PROJECT_SECRETS = [
  'CLIENT_CONTACT_PROTECTION_KEYRING',
  'MAILBOX_RESOLVER_CALLER_AUTH_KEY',
  'MAILBOX_RESOLVER_ENCRYPTION_KEYRING',
  'MAILBOX_RESOLVER_HANDLE_HMAC_KEY',
];
const EXPECTED_PROVIDER_SECRETS = [
  'CLOUDFLARE_ACCESS_CLIENT_ID',
  'CLOUDFLARE_ACCESS_CLIENT_SECRET',
  'CLOUDFLARE_API_TOKEN',
  'GOOGLE_OAUTH_CLIENT_SECRET',
  'MICROSOFT_OAUTH_CLIENT_SECRET',
  'R2_GENERATION_ACCESS_KEY_ID',
  'R2_GENERATION_SECRET_ACCESS_KEY',
];
const EXPECTED_CREATION_ORDER = [
  'accepted_resolver_release_artifact',
  'dedicated_resolver_d1',
  'catalog_d1_and_d3a_bootstrap',
  'profile_r2_and_queues',
  'r2_s3_credentials',
  'resolver_worker_with_secrets',
  'access_application_service_auth_policy_and_token',
  'github_staging_environment_inputs',
  'd3_control_plane_first_deploy_with_secrets_and_custom_domain',
  'd3_staging_smoke_and_attestation',
];
const REQUIRED_AR8C_IDS = [
  'cloudflare.access-service-auth',
  'cloudflare.deploy-manifest-protected-input',
  'cloudflare.platform-api',
  'profile-generation.r2-access',
];
const FORBIDDEN_VALUE_KEYS = new Set([
  'value',
  'plaintext',
  'secret_value',
  'token_value',
  'credential_value',
  'private_key',
]);

function object(value) {
  return value && typeof value === 'object' && !Array.isArray(value);
}

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

function sameStringSet(actual, expected) {
  if (!Array.isArray(actual) || actual.some((item) => typeof item !== 'string')) return false;
  const left = new Set(actual);
  const right = new Set(expected);
  return left.size === actual.length && right.size === expected.length && left.size === right.size && [...left].every((item) => right.has(item));
}

function sameArray(actual, expected) {
  return Array.isArray(actual) && JSON.stringify(actual) === JSON.stringify(expected);
}

function findForbiddenValueKey(value, prefix = '') {
  if (Array.isArray(value)) {
    for (let index = 0; index < value.length; index += 1) {
      const found = findForbiddenValueKey(value[index], `${prefix}[${index}]`);
      if (found) return found;
    }
    return null;
  }
  if (!object(value)) return null;
  for (const [key, nested] of Object.entries(value)) {
    if (FORBIDDEN_VALUE_KEYS.has(key.toLowerCase())) return prefix ? `${prefix}.${key}` : key;
    const found = findForbiddenValueKey(nested, prefix ? `${prefix}.${key}` : key);
    if (found) return found;
  }
  return null;
}

async function loadJson(root, relative) {
  const payload = JSON.parse(await readFile(path.join(root, relative), 'utf8'));
  if (!object(payload)) throw new Error(`${relative} must contain one JSON object`);
  return payload;
}

function validate(contract, credentialAuthority, bootstrapAuthority) {
  const errors = [];
  const expect = (condition, message) => { if (!condition) errors.push(message); };

  expect(contract.schema_version === 1, 'bootstrap contract schema_version must be 1');
  expect(contract.status === 'CANDIDATE_AR8C_HOSTED_PREREQUISITE', 'bootstrap contract status drifted');
  expect(contract.parent_issue === 308, 'bootstrap contract must remain under AR-8 umbrella #308');
  expect(contract.implementation_issue === 314, 'bootstrap contract must remain an AR-8C prerequisite under #314');
  expect(contract.environment === 'staging', 'bootstrap contract must be staging-only');
  expect(contract.role === 'EXECUTION_PREREQUISITE_ONLY_NOT_CREDENTIAL_REGISTRY', 'bootstrap contract must not become a competing credential registry');
  expect(contract.canonical_credential_authority === CREDENTIAL_AUTHORITY_RELATIVE, 'canonical credential authority reference drifted');
  expect(contract.canonical_architecture_inventory === 'architecture/inventory.json', 'canonical architecture inventory reference drifted');
  expect(contract.accepted_bootstrap_design_authority === BOOTSTRAP_AUTHORITY_RELATIVE, 'accepted bootstrap authority reference drifted');

  const invariants = contract.invariants;
  expect(object(invariants), 'bootstrap invariants are required');
  if (object(invariants)) {
    expect(invariants.plaintext_in_git === 'FORBIDDEN', 'plaintext in Git must remain forbidden');
    expect(invariants.placeholder_or_guessed_provider_state === 'FORBIDDEN', 'guessed/placeholder provider state must remain forbidden');
    expect(invariants.secret_value_readback === 'FORBIDDEN', 'secret value readback must remain forbidden');
    expect(invariants.production_mutation === false, 'production mutation must remain forbidden');
    expect(invariants.terraform === false, 'Terraform must remain outside this project authority');
    expect(invariants.automatic_resource_provisioning === false, 'routine/automatic provider provisioning must remain forbidden');
    expect(invariants.explicit_external_staging_bootstrap_mutation === true, 'explicit external staging bootstrap mutation must remain authorized');
    expect(invariants.read_only_discovery_before_mutation === true, 'provider discovery must run before mutation');
    expect(invariants.reuse_matching_accepted_resources === true, 'matching accepted resources must be reused');
    expect(invariants.bootstrap_credential_must_differ_from_steady_state === true, 'bootstrap and steady-state credentials must stay distinct');
    expect(invariants.bootstrap_credential_must_be_revoked_after_verified_cutover === true, 'bootstrap credential must be revoked after verified cutover');
    expect(invariants.routine_deploy_must_not_rotate_runtime_secrets === true, 'routine deploy must not become runtime-secret rotation authority');
    expect(invariants.ar8c_reconciliation_remains_read_only === true, 'AR-8C reconciliation must remain read-only');
    expect(invariants.ar9_blocked_until_full_ar8_acceptance === true, 'AR-9 must remain blocked until full AR-8 acceptance');
  }

  const model = contract.bootstrap_model;
  expect(object(model), 'bootstrap_model is required');
  if (object(model)) {
    expect(model.discovery?.mode === 'READ_ONLY_FIRST', 'bootstrap discovery must be READ_ONLY_FIRST');
    expect(model.bootstrap_authentication?.kind === 'EPHEMERAL_EXTERNAL_PROVIDER_AUTHORITY', 'bootstrap authentication must stay external and ephemeral');
    expect(sameStringSet(model.bootstrap_authentication?.forbidden_bindings, ['CLOUDFLARE_API_TOKEN']), 'bootstrap credential must never be bound as CLOUDFLARE_API_TOKEN');
    expect(sameStringSet(model.project_generated_secret_classes, EXPECTED_RUNTIME_PROJECT_SECRETS), 'project-generated staging secret classes drifted');
    expect(sameStringSet(model.provider_issued_secret_classes, EXPECTED_PROVIDER_SECRETS), 'provider-issued staging secret classes drifted');
    expect(sameArray(model.resource_creation_order, EXPECTED_CREATION_ORDER), 'bootstrap resource creation order drifted from accepted authority');
  }

  const outputs = contract.staging_outputs;
  expect(object(outputs), 'staging_outputs are required');
  if (object(outputs)) {
    expect(outputs.github_environment === 'staging', 'bootstrap outputs must target staging GitHub Environment only');
    expect(sameStringSet(outputs.required_new_or_verified_bindings, EXPECTED_OUTPUTS), 'AR-8C hosted prerequisite output bindings drifted');
    expect(sameStringSet(outputs.transitional_existing_bundle_bindings, EXPECTED_TRANSITIONAL_BUNDLES), 'transitional bundle bindings drifted');
    expect(typeof outputs.steady_state_direction === 'string' && outputs.steady_state_direction.includes('removed from routine application deployment exposure'), 'steady-state bundle normalization must remain explicit');
  }

  expect(contract.ar8c_handoff?.mode === 'READ_ONLY_METADATA_RECONCILIATION', 'handoff must return to read-only AR-8C reconciliation');
  expect(contract.ar8c_handoff?.next_executor === '.github/scripts/credential-lifecycle.mjs cloudflare-live', 'AR-8C reconciliation executor drifted');
  expect(Array.isArray(contract.remaining_ar8_after_ar8c) && contract.remaining_ar8_after_ar8c.length === 5, 'full remaining AR-8 sequence must stay explicit');

  expect(bootstrapAuthority.schema_version === 1, 'accepted bootstrap authority schema drifted');
  expect(sameArray(bootstrapAuthority.staging_creation_order, EXPECTED_CREATION_ORDER), 'contract creation order must match accepted #256 authority');
  expect(bootstrapAuthority.rules?.automatic_resource_provisioning_forbidden === true, 'accepted authority must continue forbidding automatic resource provisioning');
  expect(bootstrapAuthority.rules?.bootstrap_token_as_deploy_token_forbidden === true, 'accepted authority must continue forbidding bootstrap token reuse as deploy token');
  expect(bootstrapAuthority.rules?.staging_first === true, 'accepted authority must remain staging-first');
  expect(bootstrapAuthority.rules?.production_ready_must_remain_false === true, 'accepted bootstrap authority must not claim production readiness');

  expect(credentialAuthority.status === 'ACCEPTED_AR8B_CREDENTIAL_METADATA_AUTHORITY', 'canonical credential authority must remain accepted AR-8B authority');
  expect(credentialAuthority.canonical_inventory === 'architecture/inventory.json', 'credential authority must continue projecting into canonical architecture inventory');
  expect(credentialAuthority.invariants?.competing_registry === 'FORBIDDEN', 'competing credential registry must remain forbidden');
  expect(credentialAuthority.invariants?.production_mutation === false, 'credential authority must continue forbidding production mutation');
  const ar8cIds = new Set((credentialAuthority.credentials ?? []).filter((item) => item?.future_cutover === 'AR-8C').map((item) => item?.id));
  for (const id of REQUIRED_AR8C_IDS) expect(ar8cIds.has(id), `canonical credential authority is missing AR-8C concern ${id}`);

  const forbidden = findForbiddenValueKey(contract);
  expect(!forbidden, `bootstrap contract contains forbidden value-shaped field ${forbidden ?? ''}`.trim());
  return errors;
}

function report(errors) {
  for (const error of errors) console.error(error);
  return errors.length === 0;
}

async function validateAll(root, contractOverride = null) {
  const [contract, credentialAuthority, bootstrapAuthority] = await Promise.all([
    contractOverride ?? loadJson(root, CONTRACT_RELATIVE),
    loadJson(root, CREDENTIAL_AUTHORITY_RELATIVE),
    loadJson(root, BOOTSTRAP_AUTHORITY_RELATIVE),
  ]);
  return validate(contract, credentialAuthority, bootstrapAuthority);
}

async function selfTest(root) {
  const contract = await loadJson(root, CONTRACT_RELATIVE);
  const baseline = await validateAll(root, contract);
  if (baseline.length !== 0) return report(baseline);
  const fixtures = [
    { name: 'production mutation', expected: 'production mutation', mutate: (copy) => { copy.invariants.production_mutation = true; } },
    { name: 'automatic provisioning', expected: 'automatic provider provisioning', mutate: (copy) => { copy.invariants.automatic_resource_provisioning = true; } },
    { name: 'bootstrap deploy-token reuse', expected: 'bootstrap and steady-state', mutate: (copy) => { copy.invariants.bootstrap_credential_must_differ_from_steady_state = false; } },
    { name: 'missing hosted output', expected: 'output bindings drifted', mutate: (copy) => { copy.staging_outputs.required_new_or_verified_bindings.pop(); } },
    { name: 'creation-order drift', expected: 'creation order drifted', mutate: (copy) => { [copy.bootstrap_model.resource_creation_order[1], copy.bootstrap_model.resource_creation_order[2]] = [copy.bootstrap_model.resource_creation_order[2], copy.bootstrap_model.resource_creation_order[1]]; } },
    { name: 'routine secret rotation', expected: 'routine deploy', mutate: (copy) => { copy.invariants.routine_deploy_must_not_rotate_runtime_secrets = false; } },
    { name: 'credential registry split', expected: 'competing credential registry', mutate: (copy) => { copy.role = 'SECOND_CREDENTIAL_REGISTRY'; } },
    { name: 'value-shaped field', expected: 'forbidden value-shaped', mutate: (copy) => { copy.bootstrap_model.secret_value = 'forbidden'; } },
  ];
  for (const fixture of fixtures) {
    const copy = clone(contract);
    fixture.mutate(copy);
    const errors = await validateAll(root, copy);
    if (errors.length === 0 || !errors.some((error) => error.toLowerCase().includes(fixture.expected.toLowerCase()))) {
      console.error(`negative fixture ${fixture.name} was not rejected as expected: ${JSON.stringify(errors)}`);
      return false;
    }
  }
  console.log('AR-8 staging provider bootstrap negative fixtures passed.');
  return true;
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
  if (command === 'contract') {
    const errors = await validateAll(root);
    if (!report(errors)) return 1;
    console.log('AR-8 staging provider bootstrap prerequisite contract is internally consistent.');
    return 0;
  }
  if (command === 'self-test') return (await selfTest(root)) ? 0 : 1;
  console.error(`unknown command: ${command}; expected contract or self-test`);
  return 2;
}

main()
  .then((code) => { process.exitCode = code; })
  .catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
