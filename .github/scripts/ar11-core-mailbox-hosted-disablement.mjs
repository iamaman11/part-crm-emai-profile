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
const OBSERVE_SECRET = 'CLOUDFLARE_OBSERVE_API_TOKEN: ${{ secrets.CLOUDFLARE_OBSERVE_API_TOKEN }}';
const DEPLOY_SECRET = 'CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}';

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
  const mailboxUnits = new Set([
    'mailbox_admin',
    'mailbox_client_binding',
    'mailbox_browser_binding',
    'mailbox_read',
    'mailbox_jobs',
    'outbound_mail',
  ]);
  const disabledResources = new Set([
    'mailbox_jobs',
    'mailbox_jobs_dlq',
    'mailbox_secret_resolver_worker',
    'resolver_d1',
    'resolver_reconciliation_schedule',
    'mailbox_secret_resolver_service',
  ]);
  const profiles = new Map((architecture.release_profiles ?? []).map((row) => [row.profile_id, row]));
  const closures = new Map((architecture.deployment_closures ?? []).map((row) => [row.closure_id, row]));
  for (const profileId of ['rehearsal-core-v1', 'production-core-v1']) {
    const profile = profiles.get(profileId);
    if (!object(profile)) fail(`missing Core profile ${profileId}`);
    const disabled = new Set(profile.disabled_activation_units ?? []);
    for (const unit of mailboxUnits) {
      if (!disabled.has(unit)) fail(`${profileId} unexpectedly enables mailbox unit ${unit}`);
    }
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
}

function validateWorkflow(text) {
  const required = [
    'Prove disabled AR-11 Core mailbox surfaces have no hosted execution authority',
    OBSERVE_SECRET,
    'workers/scripts/$worker_name/settings',
    'MAILBOX_SECRET_RESOLVER',
    'workers/scripts/$resolver_name/schedules',
    '(.result.schedules | length) == 0',
    'workers/scripts/$resolver_name/subdomain',
    '.result.enabled == false',
    '.result.previews_enabled == false',
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

async function validate(root, workflowOverride = null) {
  validateArchitecture(await readJson(root, ARCHITECTURE));
  validateConfigs(await readJson(root, CORE_CONFIG), await readJson(root, RESOLVER_CONFIG));
  const workflow = workflowOverride ?? await readFile(path.join(root, WORKFLOW), 'utf8');
  validateWorkflow(workflow);
}

async function expectRejected(label, workflow, from, to) {
  const mutated = workflow.replace(from, to);
  if (mutated === workflow) fail(`negative fixture did not mutate workflow: ${label}`);
  try {
    await validate(ROOT, mutated);
  } catch {
    return;
  }
  fail(`negative fixture unexpectedly passed: ${label}`);
}

async function selfTest() {
  const workflow = await readFile(path.join(ROOT, WORKFLOW), 'utf8');
  await validate(ROOT, workflow);
  await expectRejected(
    'deploy credential in hosted audit',
    workflow,
    OBSERVE_SECRET,
    DEPLOY_SECRET,
  );
  await expectRejected(
    'missing resolver schedule proof',
    workflow,
    '(.result.schedules | length) == 0',
    '(.result.schedules | length) >= 0',
  );
  await expectRejected(
    'workers.dev enabled accepted',
    workflow,
    '.result.enabled == false',
    '.result.enabled == true',
  );
  await expectRejected(
    'control-plane resolver binding proof removed',
    workflow,
    'MAILBOX_SECRET_RESOLVER',
    'UNREVIEWED_BINDING',
  );
  console.log('AR-11 Core mailbox hosted-disablement negative fixtures rejected as expected.');
}

async function main() {
  if (process.argv.includes('--self-test')) {
    await selfTest();
    return;
  }
  if (process.argv.length > 2) fail(`unknown arguments: ${process.argv.slice(2).join(' ')}`);
  await validate(ROOT);
  console.log('AR-11 Core mailbox hosted-disablement contract passed: Core has no mailbox binding, standalone resolver must have zero cron triggers and workers.dev/previews disabled, and the proof uses observe-only authority.');
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});
