#!/usr/bin/env node

import { readFile } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = path.resolve(SCRIPT_DIR, '..', '..');
const RUNTIME_AUTHORITY = 'architecture/runtime-cutover-ar10.json';
const AR7_AUTHORITY = 'architecture/github-governance-ar7.json';
const ACTIONS_REGISTRY = 'architecture/github-actions-registry.json';
const EXPECTED_CONTEXTS = [
  'Real Camoufox cold-launch proof',
  'Profile Bridge Windows regression',
];

function sameStringSet(actual, expected) {
  if (!Array.isArray(actual) || !Array.isArray(expected)) return false;
  const left = new Set(actual);
  const right = new Set(expected);
  return left.size === actual.length
    && right.size === expected.length
    && left.size === right.size
    && [...left].every((value) => typeof value === 'string' && right.has(value));
}

async function loadJson(root, relative) {
  const payload = JSON.parse(await readFile(path.join(root, relative), 'utf8'));
  if (!payload || typeof payload !== 'object' || Array.isArray(payload)) {
    throw new Error(`${relative} must contain one JSON object`);
  }
  return payload;
}

async function validateContract(root) {
  const errors = [];
  const expect = (condition, message) => { if (!condition) errors.push(message); };
  const runtime = await loadJson(root, RUNTIME_AUTHORITY);
  const ar7 = await loadJson(root, AR7_AUTHORITY);
  const registry = await loadJson(root, ACTIONS_REGISTRY);
  const extension = runtime.github_governance_extension ?? {};

  expect(runtime.schema_version === 1, 'AR-10 runtime authority schema_version must be 1');
  expect(runtime.owning_slice === 'AR-10', 'AR-10 runtime authority ownership drifted');
  expect(runtime.production_mutation === false, 'AR-10 governance extension may not mutate production');
  expect(extension.base_authority === AR7_AUTHORITY, 'AR-10 governance extension must compose from accepted AR-7 authority');
  expect(extension.mode === 'ADDITIVE_REQUIRED_CHECKS_ONLY', 'AR-10 governance extension must be additive-only');
  expect(extension.permanent_workflow === '.github/workflows/camoufox-runtime-gate.yml', 'AR-10 governance extension workflow drifted');
  expect(sameStringSet(extension.required_job_contexts, EXPECTED_CONTEXTS), 'AR-10 required Camoufox contexts drifted');
  expect(extension.historical_ar7_required_checks_may_be_removed === false, 'AR-10 may not remove accepted AR-7 required checks');
  expect(extension.hosted_main_protection_required_before_guarded_merge === true, 'AR-10 must require hosted protection before merge');
  expect(extension.live_validator === '.github/scripts/ar10-github-governance-extension.mjs', 'AR-10 live validator path drifted');

  expect(ar7.status === 'ACCEPTED_AR7_GITHUB_GOVERNANCE', 'accepted AR-7 governance provenance drifted');
  expect(Array.isArray(ar7?.main_governance?.required_checks) && ar7.main_governance.required_checks.length > 0, 'accepted AR-7 required check baseline is missing');

  const registration = Array.isArray(registry.active_registrations)
    ? registry.active_registrations.find((entry) => entry?.path === extension.permanent_workflow)
    : undefined;
  expect(registration?.category === 'PERMANENT_REQUIRED', 'Camoufox runtime workflow must remain PERMANENT_REQUIRED');

  const workflow = await readFile(path.join(root, extension.permanent_workflow), 'utf8');
  for (const context of EXPECTED_CONTEXTS) {
    expect(workflow.includes(`name: ${context}`), `Camoufox workflow lost required job context: ${context}`);
  }

  return { errors, runtime, ar7 };
}

function protectionCheckNames(protection) {
  const status = protection?.required_status_checks ?? {};
  const names = new Set();
  for (const context of status.contexts ?? []) {
    if (typeof context === 'string') names.add(context);
  }
  for (const check of status.checks ?? []) {
    if (typeof check?.context === 'string') names.add(check.context);
  }
  return names;
}

async function githubJson(apiPath, token) {
  const response = await fetch(`https://api.github.com${apiPath}`, {
    headers: {
      Accept: 'application/vnd.github+json',
      Authorization: `Bearer ${token}`,
      'User-Agent': 'part-crm-ar10-governance-audit',
      'X-GitHub-Api-Version': '2022-11-28',
    },
  });
  if (!response.ok) {
    const body = (await response.text()).slice(0, 1000);
    throw new Error(`GitHub API ${apiPath} failed closed: HTTP ${response.status}: ${body}`);
  }
  return response.json();
}

async function liveAudit(root) {
  const { errors, runtime, ar7 } = await validateContract(root);
  if (errors.length !== 0) return errors;
  const token = process.env.GOVERNANCE_AUDIT_TOKEN;
  const repository = process.env.GITHUB_REPOSITORY || 'iamaman11/part-crm-emai-profile';
  if (!token) return ['GOVERNANCE_AUDIT_TOKEN is required for AR-10 live Administration:read audit'];
  if (repository !== 'iamaman11/part-crm-emai-profile') return [`unexpected repository: ${repository}`];

  const branch = await githubJson(`/repos/${repository}/branches/main`, token);
  if (branch?.protected !== true) errors.push('live main branch is not protected');
  const protection = await githubJson(`/repos/${repository}/branches/main/protection`, token);
  if (protection?.required_status_checks?.strict !== true) {
    errors.push('live main required status checks must remain strict');
  }
  const names = protectionCheckNames(protection);
  for (const baseline of ar7.main_governance.required_checks) {
    if (!names.has(baseline)) errors.push(`live main lost accepted AR-7 required check: ${baseline}`);
  }
  for (const context of runtime.github_governance_extension.required_job_contexts) {
    if (!names.has(context)) errors.push(`live main is missing AR-10 required check: ${context}`);
  }
  return errors;
}

async function selfTest(root) {
  const { errors, runtime } = await validateContract(root);
  if (errors.length !== 0) return errors;
  const mutated = structuredClone(runtime);
  mutated.github_governance_extension.required_job_contexts.pop();
  if (sameStringSet(mutated.github_governance_extension.required_job_contexts, EXPECTED_CONTEXTS)) {
    return ['AR-10 governance missing-context negative fixture unexpectedly passed'];
  }
  const weakened = structuredClone(runtime);
  weakened.github_governance_extension.historical_ar7_required_checks_may_be_removed = true;
  if (weakened.github_governance_extension.historical_ar7_required_checks_may_be_removed !== true) {
    return ['AR-10 governance weakening fixture did not mutate'];
  }
  console.log('AR-10 GitHub governance extension negative fixtures passed.');
  return [];
}

function report(errors) {
  for (const error of errors) console.error(error);
  return errors.length === 0;
}

async function main() {
  const command = process.argv[2] ?? 'contract';
  const root = DEFAULT_ROOT;
  if (command === 'contract') {
    const { errors } = await validateContract(root);
    if (!report(errors)) return 1;
    console.log('AR-10 additive GitHub governance extension is internally consistent.');
    return 0;
  }
  if (command === 'self-test') return report(await selfTest(root)) ? 0 : 1;
  if (command === 'live') {
    const errors = await liveAudit(root);
    if (!report(errors)) return 1;
    console.log('Live main protection contains the accepted AR-7 baseline plus AR-10 Camoufox required checks.');
    return 0;
  }
  console.error(`unknown command: ${command}; expected contract, self-test, or live`);
  return 2;
}

main()
  .then((code) => { process.exitCode = code; })
  .catch((error) => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  });
