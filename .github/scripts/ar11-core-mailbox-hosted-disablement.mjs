#!/usr/bin/env node

import { readFile } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const WORKFLOW = '.github/workflows/github-governance-gate.yml';
const RESOLVER_CONFIG = 'deploy/cloudflare/mailbox-secret-resolver.wrangler.jsonc';
const CORE_CONFIG = 'deploy/cloudflare/wrangler.jsonc';
const ARCHITECTURE = 'architecture/release-architecture-ar11.json';
const CREDENTIAL_EXTENSION = 'architecture/credential-authority-ar11-extension.json';
const OBSERVE_CREDENTIAL_ID = 'cloudflare.staging-observation-api';
const OBSERVE_SECRET = 'CLOUDFLARE_OBSERVE_API_TOKEN: ${{ secrets.CLOUDFLARE_OBSERVE_API_TOKEN }}';
const DEPLOY_SECRET = 'CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}';
const HOSTED_READ_CALLS = Object.freeze([
  'GET /accounts/{account_id}/workers/scripts/{worker_name}/settings',
  'GET /accounts/{account_id}/workers/scripts/{resolver_name}/schedules',
  'GET /accounts/{account_id}/workers/scripts/{resolver_name}/subdomain',
]);

function fail(message) {
  throw new Error(`AR-11 Core mailbox hosted-disablement contract rejected: ${message}`);
}

function object(value) {
  return value && typeof value === 'object' && !Array.isArray(value);
}

function requireFragment(text, fragment, label) {
  if (!text.includes(fragment)) fail(`${label} missing required fragment: ${fragment}`);
}

function validateArchitecture(architecture) {
  if (!object(architecture) || architecture.kind !== 'AR11_RELEASE_ARCHITECTURE_SOURCE') {
    fail('release architecture identity drifted');
  }
  if (architecture.production_mutation !== false || architecture.production_ready !== false) {
    fail('AR-11 must remain production-blocked');
  }
  for (const forbidden of ['activation_units', 'release_profiles', 'execution_surfaces']) {
    if (forbidden in architecture) {
      fail(`${forbidden} must remain owned by crates/capability-policy, not AR-11 JSON`);
    }
  }
  const projection = architecture.capability_policy_projection;
  if (!object(projection)
      || projection.semantic_owner !== 'crates/capability-policy'
      || projection.typed_snapshot !== 'capability-policy::snapshot_v1'
      || projection.generated_manifest !== 'capability-policy-v1.json'
      || projection.generated_manifest_role !== 'IMMUTABLE_RELEASE_SET_PROJECTION_ONLY'
      || projection.manifest_semantic_input !== false
      || projection.runtime_authorization_from_manifest !== false) {
    fail('capability-policy ownership/projection boundary drifted');
  }

  const disabledResources = new Set([
    'mailbox_jobs',
    'mailbox_jobs_dlq',
    'mailbox_secret_resolver_worker',
    'resolver_d1',
    'resolver_reconciliation_schedule',
    'mailbox_secret_resolver_service',
  ]);
  const closures = new Map((architecture.deployment_closures ?? []).map((row) => [row.closure_id, row]));
  for (const profileId of ['rehearsal-core-v1', 'production-core-v1']) {
    const closure = closures.get(profileId);
    if (!object(closure)) fail(`missing Core deployment closure ${profileId}`);
    const optional = new Set(closure.optional_or_disabled_resources ?? []);
    for (const resource of disabledResources) {
      if (!optional.has(resource)) fail(`${profileId} does not keep ${resource} optional/disabled`);
    }
    const requiredBindings = new Set(closure.required_bindings ?? []);
    if (requiredBindings.has('MAILBOX_SECRET_RESOLVER') || requiredBindings.has('MAILBOX_JOBS')) {
      fail(`${profileId} requires a mailbox operational binding`);
    }
    const requiredCredentials = new Set(closure.required_credentials ?? []);
    if (requiredCredentials.has('MAILBOX_RESOLVER_CALLER_AUTH_KEY')) {
      fail(`${profileId} requires a mailbox resolver caller credential`);
    }
  }
}

function validateCredentialAuthority(extension) {
  if (!object(extension) || extension.kind !== 'AR11_ADDITIVE_CREDENTIAL_CAPABILITY_EXTENSION') {
    fail('AR-11 credential extension identity drifted');
  }
  if (extension.production_mutation !== false || extension.metadata_only !== true) {
    fail('AR-11 credential extension must remain metadata-only and production-blocked');
  }
  const credentials = Array.isArray(extension.credentials) ? extension.credentials : [];
  const credential = credentials.find((row) => row?.id === OBSERVE_CREDENTIAL_ID);
  if (!object(credential)) fail(`missing ${OBSERVE_CREDENTIAL_ID} authority`);
  if (credential.allowed_mutator !== 'NONE' || credential.mutation_allowed !== false || credential.provider_mutation_forbidden !== true) {
    fail('observe credential unexpectedly gained mutation authority');
  }
  const environments = credential?.environment_scope?.environments;
  if (!Array.isArray(environments) || environments.length !== 1 || environments[0] !== 'staging') {
    fail('observe credential must remain staging-only');
  }
  const permissions = new Set(credential.required_provider_permissions ?? []);
  if (!permissions.has('Workers Scripts Read') || permissions.has('Workers Scripts Write') || permissions.has('Workers Scripts Edit')) {
    fail('hosted disablement audit must rely on Workers Scripts Read without write/edit permission');
  }
  const consumers = new Set(credential.consumers ?? []);
  if (!consumers.has(WORKFLOW)) fail('GitHub Governance Gate is not an authorized observe-credential consumer');
  const calls = new Set(credential?.runtime_verification?.required_read_calls ?? []);
  for (const call of HOSTED_READ_CALLS) {
    if (!calls.has(call)) fail(`observe credential authority does not declare hosted disablement read: ${call}`);
  }
}

function validateConfigs(core, resolver) {
  for (const environment of ['staging', 'production']) {
    const vars = core?.env?.[environment]?.vars;
    if (!object(vars)) fail(`Core ${environment} vars are missing`);
    const serialized = JSON.stringify(core.env[environment]);
    if (serialized.includes('MAILBOX_SECRET_RESOLVER') || serialized.includes('MAILBOX_JOBS')) {
      fail(`Core ${environment} config contains a mailbox operational binding`);
    }
  }
  const staging = resolver?.env?.staging;
  if (!object(staging) || typeof staging.name !== 'string' || staging.name.length === 0) {
    fail('resolver staging script name is missing');
  }
  if (resolver.workers_dev !== false || staging.workers_dev !== false) {
    fail('resolver workers.dev must remain disabled in repository config');
  }
  if (!Array.isArray(staging.routes) || staging.routes.length !== 0) {
    fail('resolver staging routes must remain empty');
  }
  if (!object(staging.triggers)
      || !Array.isArray(staging.triggers.crons)
      || staging.triggers.crons.length !== 0) {
    fail('resolver staging cron triggers must remain explicitly disabled');
  }
}

function validateWorkflow(text) {
  const required = [
    'Prove disabled AR-11 Core mailbox surfaces have no hosted execution authority',
    OBSERVE_SECRET,
    'workers/scripts/$worker_name/settings',
    'MAILBOX_SECRET_RESOLVER',
    'MAILBOX_JOBS',
    'MAILBOX_RESOLVER_CALLER_AUTH_KEY',
    'workers/scripts/$resolver_name/schedules',
    '(.env.staging.triggers.crons | length) == 0',
    '(.result.schedules | length) == 0',
    'workers/scripts/$resolver_name/subdomain',
    '.result.enabled == false',
    '.result.previews_enabled == false',
    'wrangler@4.94.0 secret list --name "$worker_name" --format json',
    'AR11_CORE_MAILBOX_HOSTED_DISABLED',
  ];
  for (const fragment of required) requireFragment(text, fragment, 'GitHub Governance Gate');

  const jobStart = text.indexOf('\n  operational-credential-state:');
  if (jobStart < 0) fail('Operational Credential Hosted State job is missing');
  const body = text.slice(jobStart);
  if (body.includes(DEPLOY_SECRET)) {
    fail('Operational Credential Hosted State must never materialize deploy-capable Cloudflare credential');
  }
  const auditStart = body.indexOf('Prove disabled AR-11 Core mailbox surfaces have no hosted execution authority');
  if (auditStart < 0) fail('hosted mailbox-disablement audit step is missing');
  const auditBody = body.slice(auditStart);
  const settings = auditBody.indexOf('workers/scripts/$worker_name/settings');
  const schedules = auditBody.indexOf('workers/scripts/$resolver_name/schedules');
  const subdomain = auditBody.indexOf('workers/scripts/$resolver_name/subdomain');
  if (!(settings >= 0 && schedules > settings && subdomain > schedules)) {
    fail('hosted disablement audit ordering drifted');
  }
}

async function readJson(root, relative) {
  const value = JSON.parse(await readFile(path.join(root, relative), 'utf8'));
  if (!object(value)) fail(`${relative} must contain one JSON object`);
  return value;
}

async function validate(root, workflowOverride = null, credentialOverride = null, architectureOverride = null) {
  validateArchitecture(architectureOverride ?? await readJson(root, ARCHITECTURE));
  validateCredentialAuthority(credentialOverride ?? await readJson(root, CREDENTIAL_EXTENSION));
  validateConfigs(await readJson(root, CORE_CONFIG), await readJson(root, RESOLVER_CONFIG));
  const workflow = workflowOverride ?? await readFile(path.join(root, WORKFLOW), 'utf8');
  validateWorkflow(workflow);
}

async function expectWorkflowRejected(label, workflow, from, to) {
  const mutated = workflow.replace(from, to);
  if (mutated === workflow) fail(`negative fixture did not mutate workflow: ${label}`);
  try {
    await validate(ROOT, mutated);
  } catch {
    return;
  }
  fail(`negative fixture unexpectedly passed: ${label}`);
}

async function expectCredentialRejected(label, credential, mutate) {
  const candidate = structuredClone(credential);
  mutate(candidate);
  try {
    await validate(ROOT, null, candidate);
  } catch {
    return;
  }
  fail(`negative credential fixture unexpectedly passed: ${label}`);
}

async function expectArchitectureRejected(label, architecture, mutate) {
  const candidate = structuredClone(architecture);
  mutate(candidate);
  try {
    await validate(ROOT, null, null, candidate);
  } catch {
    return;
  }
  fail(`negative architecture fixture unexpectedly passed: ${label}`);
}

async function selfTest() {
  const workflow = await readFile(path.join(ROOT, WORKFLOW), 'utf8');
  const credential = await readJson(ROOT, CREDENTIAL_EXTENSION);
  const architecture = await readJson(ROOT, ARCHITECTURE);
  await validate(ROOT, workflow, credential, architecture);
  await expectWorkflowRejected(
    'deploy credential in hosted audit',
    workflow,
    OBSERVE_SECRET,
    DEPLOY_SECRET,
  );
  await expectWorkflowRejected(
    'missing resolver schedule proof',
    workflow,
    '(.result.schedules | length) == 0',
    '(.result.schedules | length) >= 0',
  );
  await expectWorkflowRejected(
    'missing source cron-disablement proof',
    workflow,
    '(.env.staging.triggers.crons | length) == 0',
    '(.env.staging.triggers.crons | length) >= 0',
  );
  await expectWorkflowRejected(
    'workers.dev enabled accepted',
    workflow,
    '.result.enabled == false',
    '.result.enabled == true',
  );
  await expectWorkflowRejected(
    'control-plane resolver binding proof removed',
    workflow,
    'MAILBOX_SECRET_RESOLVER',
    'UNREVIEWED_BINDING',
  );
  await expectWorkflowRejected(
    'control-plane caller-auth secret proof removed',
    workflow,
    'MAILBOX_RESOLVER_CALLER_AUTH_KEY',
    'UNREVIEWED_CALLER_AUTH_SECRET',
  );
  await expectWorkflowRejected(
    'unsupported Wrangler secret-list JSON flag',
    workflow,
    'secret list --name "$worker_name" --format json',
    'secret list --name "$worker_name" --json',
  );
  await expectCredentialRejected('observe credential gains mutator', credential, (copy) => {
    copy.credentials[0].allowed_mutator = 'github-governance-gate';
  });
  await expectCredentialRejected('resolver schedules read omitted', credential, (copy) => {
    copy.credentials[0].runtime_verification.required_read_calls = copy.credentials[0].runtime_verification.required_read_calls.filter(
      (value) => value !== HOSTED_READ_CALLS[1],
    );
  });
  await expectCredentialRejected('Workers Scripts Read removed', credential, (copy) => {
    copy.credentials[0].required_provider_permissions = copy.credentials[0].required_provider_permissions.filter(
      (value) => value !== 'Workers Scripts Read',
    );
  });
  await expectArchitectureRejected('legacy release profile catalog restored', architecture, (copy) => {
    copy.release_profiles = [];
  });
  await expectArchitectureRejected('Core mailbox credential restored', architecture, (copy) => {
    copy.deployment_closures.find((row) => row.closure_id === 'production-core-v1').required_credentials.push('MAILBOX_RESOLVER_CALLER_AUTH_KEY');
  });
  await expectArchitectureRejected('Core mailbox resource made required', architecture, (copy) => {
    const closure = copy.deployment_closures.find((row) => row.closure_id === 'production-core-v1');
    closure.optional_or_disabled_resources = closure.optional_or_disabled_resources.filter((value) => value !== 'mailbox_jobs');
  });
  console.log('AR-11 Core mailbox hosted-disablement workflow, operational topology, and credential-authority negative fixtures rejected as expected.');
}

async function main() {
  if (process.argv.includes('--self-test')) {
    await selfTest();
    return;
  }
  if (process.argv.length > 2) fail(`unknown arguments: ${process.argv.slice(2).join(' ')}`);
  await validate(ROOT);
  console.log('AR-11 Core mailbox hosted-disablement contract passed: capability semantics remain owned by typed capability-policy, Core has no mailbox binding/credential, standalone resolver has zero cron triggers and workers.dev/previews disabled, and every hosted read is declared by observe-only credential authority.');
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});
