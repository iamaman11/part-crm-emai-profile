#!/usr/bin/env node

import { readFileSync, writeFileSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import process from 'node:process';

const EXTENSION_PATH = 'architecture/credential-authority-ar11-extension.json';
const DEFAULT_ATTESTATION = 'docs/evidence/ar11-fc6-cloudflare-observe-token-policy.json';
const KIND = 'AR11_CLOUDFLARE_OBSERVE_TOKEN_POLICY_ATTESTATION';
const OBSERVE_CREDENTIAL_ID = 'cloudflare.staging-observation-api';
const REQUIRED_OBSERVATIONS = Object.freeze([
  'workers_deployments_read',
  'd1_catalog_read',
  'r2_bucket_read',
  'queue_read',
  'worker_secret_names_read',
]);
const TOKEN_ID = /^[A-Za-z0-9_-]{8,128}$/;
const ACCOUNT_ID = /^[0-9a-f]{32}$/;

function fail(message) {
  throw new Error(`AR-11 observe credential readiness rejected: ${message}`);
}

function readJson(path) {
  let value;
  try {
    value = JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    fail(`${path} is unavailable or invalid JSON: ${error instanceof Error ? error.message : error}`);
  }
  if (!value || typeof value !== 'object' || Array.isArray(value)) fail(`${path} must contain one JSON object`);
  return value;
}

function requiredPolicy(extension) {
  if (extension?.production_mutation !== false) fail('credential extension must keep production_mutation=false');
  const credentials = extension?.credentials;
  if (!Array.isArray(credentials)) fail('credential extension credentials must be an array');
  const credential = credentials.find((row) => row?.id === OBSERVE_CREDENTIAL_ID);
  if (!credential) fail(`credential ${OBSERVE_CREDENTIAL_ID} is missing`);
  const required = credential?.required_provider_permissions;
  const forbidden = credential?.forbidden_provider_permission_classes;
  if (!Array.isArray(required) || required.length === 0 || required.some((value) => typeof value !== 'string' || !value)) {
    fail('observe credential required provider permissions are malformed');
  }
  if (!Array.isArray(forbidden) || forbidden.length === 0 || forbidden.some((value) => typeof value !== 'string' || !value)) {
    fail('observe credential forbidden provider permissions are malformed');
  }
  const scope = credential?.environment_scope?.environments;
  if (!Array.isArray(scope) || scope.length !== 1 || scope[0] !== 'staging') fail('observe credential must remain staging-only');
  if (credential?.mutation_allowed !== false || credential?.provider_mutation_forbidden !== true || credential?.allowed_mutator !== 'NONE') {
    fail('observe credential mutation capability must remain forbidden');
  }
  return { required: [...new Set(required)].sort(), forbidden: [...new Set(forbidden)].sort() };
}

function deploymentAccountId(manifest) {
  const value = manifest?.control_plane?.account_id ?? manifest?.account_id;
  if (typeof value !== 'string' || !ACCOUNT_ID.test(value)) fail('deploy manifest account id is missing/malformed');
  return value;
}

function validateAttestation(attestation, policy, accountId) {
  const keys = [
    'schema_version',
    'kind',
    'environment',
    'token_id',
    'account_id',
    'permission_names',
    'production_scope',
    'mutation_capability',
    'token_management_capability',
    'plaintext_token_included',
    'attestation_source',
  ];
  if (JSON.stringify(Object.keys(attestation)) !== JSON.stringify(keys)) fail('issuance-policy attestation keys/order drifted');
  if (attestation.schema_version !== 1 || attestation.kind !== KIND) fail('issuance-policy attestation identity drifted');
  if (attestation.environment !== 'staging') fail('issuance-policy attestation must be staging-only');
  if (typeof attestation.token_id !== 'string' || !TOKEN_ID.test(attestation.token_id)) fail('attestation token_id is malformed');
  if (attestation.account_id !== accountId) fail('attestation account scope does not match canonical staging account');
  if (!Array.isArray(attestation.permission_names) || attestation.permission_names.some((value) => typeof value !== 'string')) fail('attestation permission_names is malformed');
  const actualPermissions = [...new Set(attestation.permission_names)].sort();
  if (actualPermissions.length !== attestation.permission_names.length) fail('attestation permission_names contains duplicates');
  if (JSON.stringify(actualPermissions) !== JSON.stringify(policy.required)) {
    fail(`attestation permissions must exactly match required read-only permissions; expected=${JSON.stringify(policy.required)} actual=${JSON.stringify(actualPermissions)}`);
  }
  for (const forbidden of policy.forbidden) {
    if (actualPermissions.includes(forbidden)) fail(`attestation contains forbidden permission ${forbidden}`);
  }
  if (attestation.production_scope !== false) fail('attestation must prove no production scope');
  if (attestation.mutation_capability !== false) fail('attestation must prove no mutation capability');
  if (attestation.token_management_capability !== false) fail('attestation must prove no token-management capability');
  if (attestation.plaintext_token_included !== false) fail('attestation must not contain plaintext token material');
  if (attestation.attestation_source !== 'CLOUDFLARE_TOKEN_ISSUANCE_POLICY') fail('attestation source must be Cloudflare token issuance policy');
  return actualPermissions;
}

function validateVerify(verify, tokenId) {
  if (verify?.success !== true || !Array.isArray(verify?.errors) || verify.errors.length !== 0) fail('Cloudflare token verify response is not successful');
  const result = verify?.result;
  if (!result || typeof result !== 'object' || Array.isArray(result)) fail('Cloudflare token verify result is malformed');
  if (result.id !== tokenId) fail('verified token id does not match issuance-policy attestation');
  if (result.status !== 'active') fail(`verified token status must be active, got ${String(result.status)}`);
}

function validateObservations(observations) {
  if (observations?.schema_version !== 1 || observations?.kind !== 'AR11_OBSERVE_CREDENTIAL_READ_OBSERVATIONS') fail('read observations identity drifted');
  for (const name of REQUIRED_OBSERVATIONS) {
    if (observations[name] !== true) fail(`required read observation did not succeed: ${name}`);
  }
  if (observations.mutation_probe !== 'FORBIDDEN_NOT_EXECUTED') fail('mutation probe must remain forbidden and unexecuted');
}

function evaluate({ extension, attestation, verify, deployManifest, observations }) {
  const policy = requiredPolicy(extension);
  const accountId = deploymentAccountId(deployManifest);
  const permissions = validateAttestation(attestation, policy, accountId);
  validateVerify(verify, attestation.token_id);
  validateObservations(observations);
  return {
    schema_version: 1,
    kind: 'AR11_OBSERVE_CREDENTIAL_READINESS',
    decision: 'READY',
    environment: 'staging',
    token_id: attestation.token_id,
    account_id: accountId,
    permission_names: permissions,
    mutation_probe: 'FORBIDDEN_NOT_EXECUTED',
    production_mutation: false,
  };
}

function argsFrom(argv) {
  const result = { attestation: DEFAULT_ATTESTATION, extension: EXTENSION_PATH };
  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i];
    if (token === '--self-test') result.selfTest = true;
    else if (['--extension', '--attestation', '--verify', '--deploy-manifest', '--observations', '--output'].includes(token)) {
      const value = argv[++i];
      if (!value) fail(`${token} requires a path`);
      result[token.slice(2).replaceAll('-', '_')] = value;
    } else fail(`unknown argument ${token}`);
  }
  return result;
}

function selfTest() {
  const required = ['Workers Scripts Read', 'D1 Read', 'Workers R2 Storage Read', 'Queues Read'];
  const extension = {
    production_mutation: false,
    credentials: [{
      id: OBSERVE_CREDENTIAL_ID,
      environment_scope: { environments: ['staging'] },
      allowed_mutator: 'NONE',
      mutation_allowed: false,
      provider_mutation_forbidden: true,
      required_provider_permissions: required,
      forbidden_provider_permission_classes: ['Workers Scripts Write', 'D1 Write', 'Workers R2 Storage Write', 'Queues Write', 'API Tokens Write'],
    }],
  };
  const accountId = 'a'.repeat(32);
  const attestation = {
    schema_version: 1,
    kind: KIND,
    environment: 'staging',
    token_id: 'observe-token-id-1234',
    account_id: accountId,
    permission_names: [...required].sort(),
    production_scope: false,
    mutation_capability: false,
    token_management_capability: false,
    plaintext_token_included: false,
    attestation_source: 'CLOUDFLARE_TOKEN_ISSUANCE_POLICY',
  };
  const verify = { success: true, errors: [], result: { id: attestation.token_id, status: 'active' } };
  const deployManifest = { control_plane: { account_id: accountId } };
  const observations = {
    schema_version: 1,
    kind: 'AR11_OBSERVE_CREDENTIAL_READ_OBSERVATIONS',
    workers_deployments_read: true,
    d1_catalog_read: true,
    r2_bucket_read: true,
    queue_read: true,
    worker_secret_names_read: true,
    mutation_probe: 'FORBIDDEN_NOT_EXECUTED',
  };
  const ready = evaluate({ extension, attestation, verify, deployManifest, observations });
  if (ready.decision !== 'READY' || ready.production_mutation !== false) fail('positive readiness fixture failed');

  const expectReject = (mutate, marker) => {
    const candidate = structuredClone({ extension, attestation, verify, deployManifest, observations });
    mutate(candidate);
    try { evaluate(candidate); fail(`negative fixture unexpectedly passed: ${marker}`); }
    catch (error) { if (!String(error).includes(marker)) throw error; }
  };
  expectReject((v) => { v.attestation.permission_names.push('Workers Scripts Write'); }, 'exactly match required');
  expectReject((v) => { v.attestation.production_scope = true; }, 'no production scope');
  expectReject((v) => { v.attestation.mutation_capability = true; }, 'no mutation capability');
  expectReject((v) => { v.attestation.token_management_capability = true; }, 'no token-management capability');
  expectReject((v) => { v.verify.result.id = 'different-token-id'; }, 'verified token id');
  expectReject((v) => { v.verify.result.status = 'disabled'; }, 'status must be active');
  expectReject((v) => { v.deployManifest.control_plane.account_id = 'b'.repeat(32); }, 'account scope');
  expectReject((v) => { v.observations.d1_catalog_read = false; }, 'd1_catalog_read');
  expectReject((v) => { v.observations.mutation_probe = 'EXECUTED'; }, 'forbidden and unexecuted');
  expectReject((v) => { v.extension.production_mutation = true; }, 'production_mutation=false');

  const directory = mkdtempSync(join(tmpdir(), 'ar11-observe-readiness-'));
  try {
    writeFileSync(join(directory, 'ready.json'), `${JSON.stringify(ready, null, 2)}\n`);
    JSON.parse(readFileSync(join(directory, 'ready.json'), 'utf8'));
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
  console.log('AR-11 observe credential readiness positive and fail-closed negative fixtures passed.');
}

function main() {
  const args = argsFrom(process.argv.slice(2));
  if (args.selfTest) { selfTest(); return; }
  for (const key of ['extension', 'attestation', 'verify', 'deploy_manifest', 'observations']) {
    if (!args[key]) fail(`--${key.replaceAll('_', '-')} is required`);
  }
  const result = evaluate({
    extension: readJson(args.extension),
    attestation: readJson(args.attestation),
    verify: readJson(args.verify),
    deployManifest: readJson(args.deploy_manifest),
    observations: readJson(args.observations),
  });
  const output = `${JSON.stringify(result, null, 2)}\n`;
  if (args.output) writeFileSync(args.output, output);
  process.stdout.write(output);
}

try { main(); }
catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}
