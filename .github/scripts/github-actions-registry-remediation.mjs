#!/usr/bin/env node

import { readFile } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = path.resolve(SCRIPT_DIR, '..', '..');
const REGISTRY_RELATIVE = 'architecture/github-actions-registry.json';
const REMEDIATION_RELATIVE = 'architecture/github-actions-registry-remediation.json';
const EXPECTED_REPOSITORY = 'iamaman11/part-crm-emai-profile';
const ALLOWED_STALE_PATH = /^\.github\/workflows\/(?:ar9-|ar10-|ar11-).+\.ya?ml$/i;

async function loadJson(root, relative) {
  const value = JSON.parse(await readFile(path.join(root, relative), 'utf8'));
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${relative} must contain one JSON object`);
  }
  return value;
}

function validateContract(registry, remediation) {
  const errors = [];
  const expect = (condition, message) => { if (!condition) errors.push(message); };
  const canonical = Array.isArray(registry?.active_registrations) ? registry.active_registrations : [];
  const canonicalPaths = new Set(canonical.map((entry) => entry?.path));
  const rows = Array.isArray(remediation?.remediations) ? remediation.remediations : [];

  expect(registry?.schema_version === 1, 'canonical Actions registry schema_version must be 1');
  expect(registry?.repository === EXPECTED_REPOSITORY, 'canonical Actions registry repository drifted');
  expect(canonical.length === 22, 'canonical Actions registry must retain exactly 22 active registrations');
  expect(remediation?.schema_version === 1, 'remediation schema_version must be 1');
  expect(remediation?.kind === 'GITHUB_ACTIONS_STALE_REGISTRATION_REMEDIATION', 'remediation kind drifted');
  expect(remediation?.status === 'ONE_SHOT_CURRENT', 'remediation must remain explicitly one-shot current');
  expect(remediation?.tracking_issue === 375, 'remediation must remain owned by issue #375');
  expect(remediation?.repository === EXPECTED_REPOSITORY, 'remediation repository drifted');
  expect(remediation?.canonical_registry === REGISTRY_RELATIVE, 'remediation canonical registry binding drifted');
  expect(remediation?.expected_canonical_active_count === canonical.length, 'remediation canonical active count drifted');
  expect(remediation?.expected_stale_active_count === 17, 'remediation must classify exactly 17 stale active registrations');
  expect(remediation?.expected_pre_remediation_active_count === canonical.length + rows.length, 'pre-remediation active count must equal canonical + stale');
  expect(remediation?.allowed_mutation === 'DISABLE_ALLOWLISTED_WORKFLOW_REGISTRATIONS_ONLY', 'remediation mutation boundary drifted');
  expect(remediation?.production_mutation === false, 'Actions registry remediation must never imply production mutation');
  expect(remediation?.cleanup_required_after_success === true, 'one-shot remediation must require source cleanup after success');
  expect(rows.length === 17, 'remediation allowlist must contain exactly 17 entries');

  const ids = new Set();
  const paths = new Set();
  for (const row of rows) {
    expect(Number.isInteger(row?.id) && row.id > 0, 'every remediation entry must have a positive integer workflow id');
    expect(typeof row?.path === 'string' && ALLOWED_STALE_PATH.test(row.path), `remediation path must be bounded to historical AR-9/10/11 workflow debt: ${row?.path}`);
    if (Number.isInteger(row?.id)) {
      expect(!ids.has(row.id), `duplicate remediation workflow id ${row.id}`);
      ids.add(row.id);
    }
    if (typeof row?.path === 'string') {
      expect(!paths.has(row.path), `duplicate remediation workflow path ${row.path}`);
      paths.add(row.path);
      expect(!canonicalPaths.has(row.path), `remediation may not disable canonical workflow path ${row.path}`);
    }
  }
  return errors;
}

function indexLive(workflows) {
  if (!Array.isArray(workflows)) throw new Error('live Actions registry response must be an array');
  const byId = new Map();
  const byPath = new Map();
  for (const workflow of workflows) {
    if (!Number.isInteger(workflow?.id) || typeof workflow?.path !== 'string' || typeof workflow?.state !== 'string') {
      throw new Error('live Actions registry contains invalid id/path/state metadata');
    }
    if (byId.has(workflow.id)) throw new Error(`duplicate live workflow id ${workflow.id}`);
    byId.set(workflow.id, workflow);
    const rows = byPath.get(workflow.path) ?? [];
    rows.push(workflow);
    byPath.set(workflow.path, rows);
  }
  return { byId, byPath };
}

function validateLiveBeforeOrPartial(registry, remediation, workflows) {
  const errors = [];
  const canonicalPaths = new Set(registry.active_registrations.map((entry) => entry.path));
  const staleIds = new Set(remediation.remediations.map((entry) => entry.id));
  const stalePaths = new Set(remediation.remediations.map((entry) => entry.path));
  let indexed;
  try {
    indexed = indexLive(workflows);
  } catch (error) {
    return [error instanceof Error ? error.message : String(error)];
  }

  for (const canonicalPath of canonicalPaths) {
    const active = (indexed.byPath.get(canonicalPath) ?? []).filter((entry) => entry.state === 'active');
    if (active.length !== 1) errors.push(`canonical workflow must have exactly one active registration: ${canonicalPath}; observed ${active.length}`);
  }

  for (const row of remediation.remediations) {
    const live = indexed.byId.get(row.id);
    if (!live) {
      errors.push(`allowlisted stale workflow id is absent from live registry: ${row.id}`);
      continue;
    }
    if (live.path !== row.path) errors.push(`allowlisted stale workflow id/path mismatch: ${row.id}; expected ${row.path}; observed ${live.path}`);
    if (!['active', 'disabled_manually'].includes(live.state)) errors.push(`allowlisted stale workflow has unexpected state ${row.id}: ${live.state}`);
  }

  const unexpectedActive = workflows.filter((entry) => entry.state === 'active' && !canonicalPaths.has(entry.path) && !staleIds.has(entry.id));
  for (const workflow of unexpectedActive) errors.push(`unknown active workflow registration blocks remediation ${workflow.id}: ${workflow.path}`);

  for (const workflow of workflows.filter((entry) => entry.state === 'active' && staleIds.has(entry.id))) {
    if (!stalePaths.has(workflow.path)) errors.push(`active stale workflow id/path escaped allowlist ${workflow.id}: ${workflow.path}`);
  }

  const activeCount = workflows.filter((entry) => entry.state === 'active').length;
  const staleActiveCount = remediation.remediations.filter((row) => indexed.byId.get(row.id)?.state === 'active').length;
  if (activeCount !== registry.active_registrations.length + staleActiveCount) {
    errors.push(`live active count must equal canonical + remaining allowlisted stale registrations; observed active=${activeCount} stale_active=${staleActiveCount}`);
  }
  if (activeCount > remediation.expected_pre_remediation_active_count || activeCount < remediation.expected_canonical_active_count) {
    errors.push(`live active count escaped remediation bounds: ${activeCount}`);
  }
  return errors;
}

function validateLiveAfter(registry, remediation, workflows) {
  const errors = validateLiveBeforeOrPartial(registry, remediation, workflows);
  let indexed;
  try {
    indexed = indexLive(workflows);
  } catch (error) {
    return [...errors, error instanceof Error ? error.message : String(error)];
  }
  for (const row of remediation.remediations) {
    const live = indexed.byId.get(row.id);
    if (live?.state !== 'disabled_manually') errors.push(`stale workflow must be disabled_manually after remediation ${row.id}: observed ${live?.state ?? 'missing'}`);
  }
  const active = workflows.filter((entry) => entry.state === 'active');
  const canonicalPaths = new Set(registry.active_registrations.map((entry) => entry.path));
  if (active.length !== registry.active_registrations.length) errors.push(`post-remediation active count must be ${registry.active_registrations.length}; observed ${active.length}`);
  for (const workflow of active) if (!canonicalPaths.has(workflow.path)) errors.push(`post-remediation unexpected active workflow ${workflow.id}: ${workflow.path}`);
  return errors;
}

async function githubRequest(apiPath, token, method = 'GET') {
  const response = await fetch(`https://api.github.com${apiPath}`, {
    method,
    headers: {
      Accept: 'application/vnd.github+json',
      Authorization: `Bearer ${token}`,
      'User-Agent': 'part-crm-actions-registry-remediation',
      'X-GitHub-Api-Version': '2022-11-28',
    },
  });
  if (!response.ok) {
    const body = (await response.text()).slice(0, 1000);
    throw new Error(`GitHub API ${method} ${apiPath} failed closed: HTTP ${response.status}: ${body}`);
  }
  if (response.status === 204) return null;
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

async function readToken() {
  process.stdin.setEncoding('utf8');
  let value = '';
  for await (const chunk of process.stdin) value += chunk;
  const token = value.trim();
  if (!token) throw new Error('ephemeral github.token must be supplied on stdin');
  return token;
}

function report(errors) {
  for (const error of errors) console.error(error);
  return errors.length === 0;
}

function syntheticRows(registry, remediation, staleState = 'active') {
  const canonical = registry.active_registrations.map((entry, index) => ({ id: 1000 + index, path: entry.path, state: 'active' }));
  const stale = remediation.remediations.map((entry) => ({ ...entry, state: staleState }));
  return [...canonical, ...stale];
}

function selfTest(registry, remediation) {
  if (!report(validateContract(registry, remediation))) return false;
  const before = syntheticRows(registry, remediation, 'active');
  if (validateLiveBeforeOrPartial(registry, remediation, before).length !== 0) {
    console.error('synthetic pre-remediation state must be valid');
    return false;
  }
  const after = syntheticRows(registry, remediation, 'disabled_manually');
  if (validateLiveAfter(registry, remediation, after).length !== 0) {
    console.error('synthetic post-remediation state must be valid');
    return false;
  }
  const unknown = structuredClone(before);
  unknown.push({ id: 999999991, path: '.github/workflows/unknown.yml', state: 'active' });
  if (!validateLiveBeforeOrPartial(registry, remediation, unknown).some((error) => error.includes('unknown active workflow'))) {
    console.error('unknown active workflow negative fixture was not rejected');
    return false;
  }
  const mismatched = structuredClone(before);
  const staleId = remediation.remediations[0].id;
  mismatched.find((entry) => entry.id === staleId).path = '.github/workflows/ar10-wrong.yml';
  if (!validateLiveBeforeOrPartial(registry, remediation, mismatched).some((error) => error.includes('id/path mismatch'))) {
    console.error('stale id/path mismatch negative fixture was not rejected');
    return false;
  }
  const unsafeContract = structuredClone(remediation);
  unsafeContract.remediations[0].path = registry.active_registrations[0].path;
  if (!validateContract(registry, unsafeContract).some((error) => error.includes('may not disable canonical workflow'))) {
    console.error('canonical workflow disable negative fixture was not rejected');
    return false;
  }
  console.log('Stale Actions registration remediation contract and negative fixtures passed.');
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
  const registry = await loadJson(root, REGISTRY_RELATIVE);
  const remediation = await loadJson(root, REMEDIATION_RELATIVE);
  const contractErrors = validateContract(registry, remediation);
  if (!report(contractErrors)) return 1;
  if (command === 'contract') {
    console.log('Stale Actions registration remediation is bounded to the 17 historical registrations observed by hosted governance.');
    return 0;
  }
  if (command === 'self-test') return selfTest(registry, remediation) ? 0 : 1;
  if (command !== 'apply') {
    console.error(`unknown command: ${command}; expected contract, self-test, or apply`);
    return 2;
  }

  if (process.env.GITHUB_EVENT_NAME !== 'push' || process.env.GITHUB_REF !== 'refs/heads/main') {
    console.error('Actions registry mutation is allowed only on an accepted push to refs/heads/main');
    return 1;
  }
  const repository = process.env.GITHUB_REPOSITORY || remediation.repository;
  if (repository !== EXPECTED_REPOSITORY) {
    console.error(`GITHUB_REPOSITORY must be ${EXPECTED_REPOSITORY}; observed ${repository}`);
    return 1;
  }
  const token = await readToken();
  const before = await listLiveWorkflows(repository, token);
  const beforeErrors = validateLiveBeforeOrPartial(registry, remediation, before);
  if (!report(beforeErrors)) return 1;
  const byId = indexLive(before).byId;
  const activeTargets = remediation.remediations.filter((row) => byId.get(row.id)?.state === 'active');
  for (const row of activeTargets) {
    await githubRequest(`/repos/${repository}/actions/workflows/${row.id}/disable`, token, 'PUT');
    console.log(`Disabled stale Actions registration ${row.id}: ${row.path}`);
  }
  const after = await listLiveWorkflows(repository, token);
  const afterErrors = validateLiveAfter(registry, remediation, after);
  if (!report(afterErrors)) return 1;
  console.log(`GitHub Actions registry converged to canonical ${registry.active_registrations.length} active registrations; disabled ${activeTargets.length} stale registrations in this run.`);
  return 0;
}

main().then((code) => { process.exitCode = code; }).catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
