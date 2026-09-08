#!/usr/bin/env node

import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = path.resolve(SCRIPT_DIR, '..', '..');
const POLICY_RELATIVE = 'architecture/github-actions-registry.json';
const WORKFLOWS_RELATIVE = '.github/workflows';
const POLICY_KEYS = [
  'active_projection',
  'authority',
  'duplicate_registration_ids_forbidden',
  'duplicate_registration_paths_forbidden',
  'historical_inactive_registrations_allowed',
  'repository',
  'schema_version',
  'unexpected_active_registrations_forbidden',
];

function sameStringSet(actual, expected) {
  if (!Array.isArray(actual) || !Array.isArray(expected)) return false;
  if (actual.some((value) => typeof value !== 'string') || expected.some((value) => typeof value !== 'string')) return false;
  const a = new Set(actual);
  const b = new Set(expected);
  if (a.size !== actual.length || b.size !== expected.length || a.size !== b.size) return false;
  return [...a].every((value) => b.has(value));
}

async function loadPolicy(root) {
  const text = await readFile(path.join(root, POLICY_RELATIVE), 'utf8');
  const value = JSON.parse(text);
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(`${POLICY_RELATIVE} must contain one JSON object`);
  return value;
}

async function trackedWorkflowPaths(root) {
  const entries = await readdir(path.join(root, WORKFLOWS_RELATIVE), { withFileTypes: true });
  return entries
    .filter((entry) => entry.isFile() && /\.ya?ml$/i.test(entry.name))
    .map((entry) => `${WORKFLOWS_RELATIVE}/${entry.name}`)
    .sort();
}

function validatePolicy(policy) {
  const errors = [];
  const expect = (condition, message) => { if (!condition) errors.push(message); };
  expect(policy?.schema_version === 2, 'registry policy schema_version must be 2');
  expect(sameStringSet(Object.keys(policy ?? {}), POLICY_KEYS), 'registry policy schema v2 fields must remain minimal and source-derived');
  expect(policy?.authority === 'github-governance', 'registry policy must remain subordinate to github-governance');
  expect(policy?.repository === 'iamaman11/part-crm-emai-profile', 'registry policy repository drifted');
  expect(policy?.active_projection === 'tracked_workflow_files', 'registry active projection must derive from tracked workflow files');
  expect(policy?.historical_inactive_registrations_allowed === true, 'historical inactive registrations must remain allowed');
  expect(policy?.unexpected_active_registrations_forbidden === true, 'unexpected active registrations must remain forbidden');
  expect(policy?.duplicate_registration_paths_forbidden === true, 'duplicate registration paths must remain forbidden');
  expect(policy?.duplicate_registration_ids_forbidden === true, 'duplicate registration ids must remain forbidden');
  expect(!Object.hasOwn(policy ?? {}, 'active_registrations'), 'registry policy must not reintroduce manual active_registrations authority');
  expect(!Object.hasOwn(policy ?? {}, 'cleanup_issue'), 'registry policy must not use a historical cleanup Issue as current authority');
  expect(!Object.hasOwn(policy ?? {}, 'baseline_evidence'), 'registry policy must not use a historical baseline snapshot as current authority');
  return errors;
}

function validateTrackedWorkflowSurface(trackedPaths) {
  const errors = [];
  if (!Array.isArray(trackedPaths)) return ['tracked workflow projection must be an array'];
  if (trackedPaths.length === 0) errors.push('tracked workflow projection must contain at least one workflow');
  if (trackedPaths.some((value) => typeof value !== 'string' || !value.startsWith(`${WORKFLOWS_RELATIVE}/`) || !/\.ya?ml$/i.test(value))) {
    errors.push('tracked workflow projection contains an invalid workflow path');
  }
  if (new Set(trackedPaths).size !== trackedPaths.length) errors.push('tracked workflow projection paths must be unique');
  return errors;
}

function validateLiveRegistry(expectedPaths, workflows) {
  const errors = [];
  if (!Array.isArray(workflows)) return ['live Actions registry response must be an array'];
  const expected = new Set(expectedPaths);
  const ids = new Set();
  const byPath = new Map();
  for (const workflow of workflows) {
    if (!Number.isInteger(workflow?.id)) {
      errors.push('live Actions registry contains a workflow without an integer id');
      continue;
    }
    if (ids.has(workflow.id)) errors.push(`duplicate workflow registration id ${workflow.id}`);
    ids.add(workflow.id);
    if (typeof workflow?.path !== 'string' || typeof workflow?.state !== 'string') {
      errors.push(`workflow registration ${workflow.id} is missing path/state metadata`);
      continue;
    }
    const list = byPath.get(workflow.path) ?? [];
    list.push(workflow);
    byPath.set(workflow.path, list);
  }
  for (const [workflowPath, registrations] of byPath) {
    if (registrations.length > 1) errors.push(`duplicate registration path: ${workflowPath}`);
  }
  for (const expectedPath of expected) {
    const registrations = byPath.get(expectedPath) ?? [];
    const active = registrations.filter((entry) => entry.state === 'active');
    if (active.length !== 1) errors.push(`current workflow must have exactly one active registration: ${expectedPath}; observed ${active.length}`);
  }
  const unexpectedActive = workflows.filter((entry) => entry?.state === 'active' && !expected.has(entry?.path));
  for (const workflow of unexpectedActive) errors.push(`unexpected active workflow registration ${workflow.id}: ${workflow.path}`);
  const activeCount = workflows.filter((entry) => entry?.state === 'active').length;
  if (activeCount !== expected.size) errors.push(`live active registration count must equal tracked workflow count ${expected.size}; observed ${activeCount}`);
  return errors;
}

function planRegistryReconciliation(expectedPaths, workflows) {
  const liveErrors = validateLiveRegistry(expectedPaths, workflows);
  if (liveErrors.length === 0) return { disable: [], blockers: [] };
  if (!Array.isArray(workflows)) return { disable: [], blockers: liveErrors };

  const expected = new Set(expectedPaths);
  const allowedError = (error) => error.startsWith('unexpected active workflow registration ') || error.startsWith('live active registration count must equal tracked workflow count ');
  const blockers = liveErrors.filter((error) => !allowedError(error));
  const disable = workflows
    .filter((entry) => entry?.state === 'active' && Number.isInteger(entry?.id) && typeof entry?.path === 'string' && !expected.has(entry.path))
    .map((entry) => ({ id: entry.id, path: entry.path }))
    .sort((left, right) => left.id - right.id);

  if (blockers.length > 0) return { disable: [], blockers };
  if (disable.length === 0) return { disable: [], blockers: ['registry drift is not safely reducible to unexpected active registrations'] };

  const activeCount = workflows.filter((entry) => entry?.state === 'active').length;
  if (activeCount !== expected.size + disable.length) {
    return { disable: [], blockers: [`unexpected active workflow count is not the only active-count excess: expected ${expected.size} + ${disable.length}, observed ${activeCount}`] };
  }
  return { disable, blockers: [] };
}

async function githubRequest(apiPath, token, { method = 'GET', expectedStatus = 200 } = {}) {
  const response = await fetch(`https://api.github.com${apiPath}`, {
    method,
    headers: { Accept: 'application/vnd.github+json', Authorization: `Bearer ${token}`, 'User-Agent': 'part-crm-actions-registry-governance', 'X-GitHub-Api-Version': '2022-11-28' },
  });
  if (response.status !== expectedStatus) {
    const body = (await response.text()).slice(0, 1000);
    throw new Error(`GitHub API ${method} ${apiPath} failed closed: HTTP ${response.status}: ${body}`);
  }
  if (expectedStatus === 204) return null;
  return response.json();
}

async function listLiveWorkflows(repository, token) {
  const workflows = [];
  let totalCount = null;
  for (let page = 1; page <= 100; page += 1) {
    const payload = await githubRequest(`/repos/${repository}/actions/workflows?per_page=100&page=${page}`, token);
    if (!Array.isArray(payload?.workflows) || !Number.isInteger(payload?.total_count)) throw new Error('GitHub Actions workflow list returned an invalid payload');
    if (totalCount === null) totalCount = payload.total_count;
    if (payload.total_count !== totalCount) throw new Error('GitHub Actions registry total_count changed during pagination');
    workflows.push(...payload.workflows);
    if (payload.workflows.length < 100) break;
    if (page === 100) throw new Error('GitHub Actions registry pagination exceeded fail-closed page bound');
  }
  if (workflows.length !== totalCount) throw new Error(`GitHub Actions registry pagination mismatch: expected ${totalCount}, observed ${workflows.length}`);
  return workflows;
}

async function disableWorkflowRegistration(repository, workflowId, token) {
  await githubRequest(`/repos/${repository}/actions/workflows/${workflowId}/disable`, token, { method: 'PUT', expectedStatus: 204 });
}

async function readEphemeralGitHubTokenFromStdin() {
  process.stdin.setEncoding('utf8');
  let value = '';
  for await (const chunk of process.stdin) value += chunk;
  const token = value.trim();
  if (!token) throw new Error('ephemeral GitHub token must be provided on stdin for live Actions registry access');
  return token;
}

function report(errors) { for (const error of errors) console.error(error); return errors.length === 0; }
function clone(value) { return JSON.parse(JSON.stringify(value)); }

function selfTest(policy, trackedPaths) {
  const baseline = trackedPaths.map((workflowPath, index) => ({ id: 1000 + index, path: workflowPath, state: 'active' }));
  if (validateLiveRegistry(trackedPaths, baseline).length !== 0) {
    console.error('registry self-test requires a valid synthetic baseline');
    return false;
  }
  const fixtures = [
    { name: 'unexpected active workflow', expected: 'unexpected active workflow registration', mutate: (rows) => rows.push({ id: 99991, path: '.github/workflows/unclassified.yml', state: 'active' }) },
    { name: 'tracked workflow disabled', expected: 'exactly one active registration', mutate: (rows) => { rows[0].state = 'disabled_manually'; } },
    { name: 'duplicate registration path', expected: 'duplicate registration path', mutate: (rows) => rows.push({ id: 99992, path: rows[0].path, state: 'disabled_manually' }) },
    { name: 'duplicate registration id', expected: 'duplicate workflow registration id', mutate: (rows) => rows.push({ id: rows[0].id, path: '.github/workflows/other.yml', state: 'disabled_manually' }) },
  ];
  for (const fixture of fixtures) {
    const rows = clone(baseline);
    fixture.mutate(rows);
    const errors = validateLiveRegistry(trackedPaths, rows);
    if (!errors.some((error) => error.includes(fixture.expected))) {
      console.error(`negative fixture ${fixture.name} was not rejected as expected: ${JSON.stringify(errors)}`);
      return false;
    }
  }

  const staleOnly = clone(baseline);
  staleOnly.push({ id: 99981, path: '.github/workflows/stale-helper.yml', state: 'active' });
  const stalePlan = planRegistryReconciliation(trackedPaths, staleOnly);
  if (stalePlan.blockers.length !== 0 || stalePlan.disable.length !== 1 || stalePlan.disable[0].id !== 99981) {
    console.error(`safe stale-registration reconciliation fixture did not produce the exact plan: ${JSON.stringify(stalePlan)}`);
    return false;
  }

  const missingTracked = clone(staleOnly);
  missingTracked[0].state = 'disabled_manually';
  const missingPlan = planRegistryReconciliation(trackedPaths, missingTracked);
  if (missingPlan.disable.length !== 0 || !missingPlan.blockers.some((error) => error.includes('exactly one active registration'))) {
    console.error(`missing tracked workflow reconciliation fixture did not fail closed: ${JSON.stringify(missingPlan)}`);
    return false;
  }

  const duplicatePath = clone(staleOnly);
  duplicatePath.push({ id: 99982, path: duplicatePath[0].path, state: 'disabled_manually' });
  const duplicatePathPlan = planRegistryReconciliation(trackedPaths, duplicatePath);
  if (duplicatePathPlan.disable.length !== 0 || !duplicatePathPlan.blockers.some((error) => error.includes('duplicate registration path'))) {
    console.error(`duplicate path reconciliation fixture did not fail closed: ${JSON.stringify(duplicatePathPlan)}`);
    return false;
  }

  const duplicateId = clone(staleOnly);
  duplicateId.push({ id: duplicateId[0].id, path: '.github/workflows/historical-other.yml', state: 'disabled_manually' });
  const duplicateIdPlan = planRegistryReconciliation(trackedPaths, duplicateId);
  if (duplicateIdPlan.disable.length !== 0 || !duplicateIdPlan.blockers.some((error) => error.includes('duplicate workflow registration id'))) {
    console.error(`duplicate id reconciliation fixture did not fail closed: ${JSON.stringify(duplicateIdPlan)}`);
    return false;
  }

  const manualAuthorityFixture = clone(policy);
  manualAuthorityFixture.active_registrations = trackedPaths.map((workflowPath) => ({ path: workflowPath, category: 'PERMANENT_REQUIRED' }));
  if (!validatePolicy(manualAuthorityFixture).some((error) => error.includes('manual active_registrations authority') || error.includes('fields must remain minimal'))) {
    console.error('manual active-registry authority reintroduction fixture was not rejected');
    return false;
  }

  const schemaDowngradeFixture = clone(policy);
  schemaDowngradeFixture.schema_version = 1;
  if (!validatePolicy(schemaDowngradeFixture).some((error) => error.includes('schema_version must be 2'))) {
    console.error('registry policy schema downgrade fixture was not rejected');
    return false;
  }

  if (!validateTrackedWorkflowSurface([]).some((error) => error.includes('at least one workflow'))) {
    console.error('empty tracked workflow projection fixture was not rejected');
    return false;
  }

  console.log('GitHub Actions registry source-derived negative and reconciliation fixtures passed.');
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
  const policy = await loadPolicy(root);
  const trackedPaths = await trackedWorkflowPaths(root);
  const contractErrors = [
    ...validatePolicy(policy),
    ...validateTrackedWorkflowSurface(trackedPaths),
  ];
  if (!report(contractErrors)) return 1;
  if (command === 'contract') {
    console.log(`GitHub Actions registry policy derives ${trackedPaths.length} active paths from the tracked workflow source.`);
    return 0;
  }
  if (command === 'self-test') return selfTest(policy, trackedPaths) ? 0 : 1;
  if (command === 'live' || command === 'reconcile') {
    const token = await readEphemeralGitHubTokenFromStdin();
    const repository = process.env.GITHUB_REPOSITORY || policy.repository;
    if (repository !== policy.repository) {
      console.error(`GITHUB_REPOSITORY must be ${policy.repository}; observed ${repository}`);
      return 1;
    }
    const workflows = await listLiveWorkflows(repository, token);
    if (command === 'live') {
      const errors = validateLiveRegistry(trackedPaths, workflows);
      if (!report(errors)) return 1;
      console.log(`GitHub Actions registry matches tracked workflow source: ${trackedPaths.length} active, historical inactive registrations tolerated.`);
      return 0;
    }

    const plan = planRegistryReconciliation(trackedPaths, workflows);
    if (!report(plan.blockers)) return 1;
    if (plan.disable.length === 0) {
      console.log(`GitHub Actions registry already matches tracked workflow source: ${trackedPaths.length} active; no reconciliation required.`);
      return 0;
    }
    for (const workflow of plan.disable) {
      console.log(`Disabling stale untracked workflow registration ${workflow.id}: ${workflow.path}`);
      await disableWorkflowRegistration(repository, workflow.id, token);
    }
    const reconciled = await listLiveWorkflows(repository, token);
    const errors = validateLiveRegistry(trackedPaths, reconciled);
    if (!report(errors)) return 1;
    console.log(`GitHub Actions registry reconciled exactly: disabled ${plan.disable.length} stale untracked registration(s); ${trackedPaths.length} tracked workflows active.`);
    return 0;
  }
  console.error(`unknown command: ${command}; expected contract, self-test, live, or reconcile`);
  return 2;
}

main().then((code) => { process.exitCode = code; }).catch((error) => { console.error(error instanceof Error ? error.message : error); process.exitCode = 1; });
