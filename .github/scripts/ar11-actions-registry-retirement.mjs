#!/usr/bin/env node

import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = path.resolve(SCRIPT_DIR, '..', '..');
const MANIFEST_RELATIVE = 'docs/evidence/2026-08-21-ar11-hosted-actions-registry-retirements.json';
const REGISTRY_RELATIVE = 'architecture/github-actions-registry.json';
const BASE_CREDENTIAL_RELATIVE = 'architecture/credential-authority-ar8b.json';
const AR11_CREDENTIAL_RELATIVE = 'architecture/credential-authority-ar11-extension.json';
const GOVERNANCE_WORKFLOW_RELATIVE = '.github/workflows/github-governance-gate.yml';
const WORKFLOWS_RELATIVE = '.github/workflows';
const REPOSITORY = 'iamaman11/part-crm-emai-profile';
const EXPECTED_DISCOVERY_SHA = '7fc11fad573f0077f1f61fd527350864df59c571';
const EXPECTED_DISCOVERY_RUN = 32509498633;
const EXPECTED_RETIREMENT_COUNT = 9;
const ALLOWED_CLASSIFICATIONS = new Set(['SCRATCH_DEBUG', 'SUPERSEDED', 'HISTORICAL_ONE_SHOT']);

function fail(message) {
  throw new Error(`AR-11 Actions registry retirement rejected: ${message}`);
}

async function readJson(filePath) {
  let value;
  try {
    value = JSON.parse(await readFile(filePath, 'utf8'));
  } catch (error) {
    fail(`${filePath} is unavailable or invalid JSON: ${error instanceof Error ? error.message : error}`);
  }
  if (!value || typeof value !== 'object' || Array.isArray(value)) fail(`${filePath} must contain one JSON object`);
  return value;
}

async function trackedWorkflowPaths(root) {
  const entries = await readdir(path.join(root, WORKFLOWS_RELATIVE), { withFileTypes: true });
  return entries
    .filter((entry) => entry.isFile() && /\.ya?ml$/i.test(entry.name))
    .map((entry) => `${WORKFLOWS_RELATIVE}/${entry.name}`)
    .sort();
}

function validateManifest(manifest, trackedPaths, registry) {
  const errors = [];
  const expect = (condition, message) => { if (!condition) errors.push(message); };
  const exactKeys = [
    'schema_version',
    'kind',
    'repository',
    'tracker_issue',
    'implementation_issue',
    'discovered_on_accepted_main_sha',
    'discovery_workflow_run_id',
    'historical_runs_preserved',
    'delete_run_history',
    'provider_mutation',
    'production_mutation',
    'required_checks_weakened',
    'retirements',
  ];
  expect(JSON.stringify(Object.keys(manifest)) === JSON.stringify(exactKeys), 'retirement manifest keys/order drifted');
  expect(manifest.schema_version === 1, 'schema_version must be 1');
  expect(manifest.kind === 'AR11_HOSTED_ACTIONS_REGISTRY_RETIREMENTS', 'manifest kind drifted');
  expect(manifest.repository === REPOSITORY, `repository must remain ${REPOSITORY}`);
  expect(manifest.tracker_issue === 399, 'tracker_issue must remain #399');
  expect(manifest.implementation_issue === 421, 'implementation_issue must remain #421');
  expect(manifest.discovered_on_accepted_main_sha === EXPECTED_DISCOVERY_SHA, 'discovery accepted-main SHA drifted');
  expect(manifest.discovery_workflow_run_id === EXPECTED_DISCOVERY_RUN, 'discovery workflow run id drifted');
  expect(manifest.historical_runs_preserved === true, 'historical run provenance must be preserved');
  expect(manifest.delete_run_history === false, 'workflow run history deletion must remain forbidden');
  expect(manifest.provider_mutation === false, 'provider mutation must remain false');
  expect(manifest.production_mutation === false, 'production mutation must remain false');
  expect(manifest.required_checks_weakened === false, 'required checks may not be weakened');

  const retirements = Array.isArray(manifest.retirements) ? manifest.retirements : [];
  expect(retirements.length === EXPECTED_RETIREMENT_COUNT, `manifest must contain exactly ${EXPECTED_RETIREMENT_COUNT} proven retirements`);
  const ids = retirements.map((row) => row?.registration_id);
  const paths = retirements.map((row) => row?.path);
  expect(ids.every(Number.isInteger), 'every retirement registration_id must be an integer');
  expect(new Set(ids).size === ids.length, 'retirement registration ids must be unique');
  expect(paths.every((value) => typeof value === 'string' && value.startsWith(`${WORKFLOWS_RELATIVE}/`) && /\.ya?ml$/i.test(value)), 'every retirement path must be a workflow YAML path');
  expect(new Set(paths).size === paths.length, 'retirement workflow paths must be unique');
  expect(retirements.every((row) => ALLOWED_CLASSIFICATIONS.has(row?.classification)), 'retirement classification is unsupported');
  expect(retirements.every((row) => typeof row?.provenance === 'string' && row.provenance.length >= 10), 'every retirement needs bounded provenance');
  expect(retirements.every((row) => row?.accepted_main_source_present === false), 'every retired registration must explicitly prove accepted-main source absence');
  expect(retirements.every((row) => row?.decision === 'DISABLE_REGISTRATION'), 'retirement decision must be DISABLE_REGISTRATION');

  const tracked = new Set(trackedPaths);
  for (const workflowPath of paths) expect(!tracked.has(workflowPath), `retired workflow path is still present on accepted source: ${workflowPath}`);

  const active = Array.isArray(registry?.active_registrations) ? registry.active_registrations : [];
  const canonicalActive = new Set(active.map((row) => row?.path));
  for (const workflowPath of paths) expect(!canonicalActive.has(workflowPath), `retired workflow path is still canonical active authority: ${workflowPath}`);
  expect(registry?.repository === REPOSITORY, 'canonical registry repository drifted');
  expect(registry?.unexpected_active_registrations_forbidden === true, 'canonical registry must keep unexpected active registrations forbidden');
  expect(registry?.historical_inactive_registrations_allowed === true, 'canonical registry must preserve historical inactive registrations');
  return errors;
}

function validateEphemeralAuthority(baseAuthority, extension, workflowText) {
  const errors = [];
  const expect = (condition, message) => { if (!condition) errors.push(message); };
  const baseCredential = (baseAuthority?.credentials ?? []).find((row) => row?.id === 'github.actions-runtime-api');
  expect(Boolean(baseCredential), 'accepted base authority must retain github.actions-runtime-api');
  expect(baseCredential?.class === 'EPHEMERAL_WORKFLOW_CREDENTIAL', 'github.actions-runtime-api class drifted');
  const aliases = (baseCredential?.bindings ?? [])
    .filter((row) => row?.surface === 'process_environment_credential_alias')
    .map((row) => row?.name)
    .sort();
  expect(JSON.stringify(aliases) === JSON.stringify(['GH_TOKEN', 'GITHUB_TOKEN']), 'github.actions-runtime-api aliases must remain GH_TOKEN/GITHUB_TOKEN');

  const rows = extension?.ephemeral_runtime_credential_extensions;
  expect(Array.isArray(rows) && rows.length === 1, 'AR-11 ephemeral runtime credential extensions must contain exactly one bounded row');
  const row = Array.isArray(rows) ? rows[0] : null;
  expect(row?.credential_id === 'github.actions-runtime-api', 'AR-11 ephemeral runtime extension must target github.actions-runtime-api');
  expect(JSON.stringify(row?.add_consumers) === JSON.stringify([
    '.github/scripts/ar11-actions-registry-retirement.mjs',
    GOVERNANCE_WORKFLOW_RELATIVE,
  ]), 'AR-11 ephemeral runtime consumers drifted');
  const boundary = row?.allowed_permission_boundary;
  expect(boundary?.workflow === GOVERNANCE_WORKFLOW_RELATIVE, 'ephemeral runtime workflow boundary drifted');
  expect(boundary?.job === 'hosted-state', 'ephemeral runtime job boundary must remain hosted-state');
  expect(boundary?.accepted_main_only === true, 'ephemeral runtime authority must remain accepted-main only');
  expect(boundary?.pull_request_exposure === false, 'pull-request Actions write exposure is forbidden');
  expect(boundary?.actions === 'write' && boundary?.contents === 'read', 'ephemeral runtime permission boundary must remain actions:write + contents:read');
  expect(boundary?.provider_credentials === false, 'ephemeral runtime retirement authority must not include provider credentials');
  expect(boundary?.production_mutation === false, 'ephemeral runtime retirement authority must not mutate production');
  expect(boundary?.historical_run_deletion === false, 'ephemeral runtime retirement authority must preserve historical runs');
  expect(typeof row?.rationale === 'string' && row.rationale.length >= 40, 'ephemeral runtime authority requires a reviewed rationale');

  const hostedMatch = workflowText.match(/\n  hosted-state:\n([\s\S]*?)\n  operational-credential-state:\n/);
  expect(Boolean(hostedMatch), 'governance workflow hosted-state job boundary is unavailable');
  const hosted = hostedMatch?.[1] ?? '';
  expect(workflowText.includes('permissions:\n  actions: read\n  contents: read'), 'governance workflow top-level Actions permission must remain read-only');
  expect(hosted.includes("if: github.event_name != 'pull_request'"), 'hosted-state must remain excluded from pull_request');
  expect(hosted.includes('permissions:\n      actions: write\n      contents: read'), 'hosted-state must declare the exact bounded Actions write permission');
  expect(!hosted.includes('environment: staging') && !hosted.includes('environment: production'), 'Actions registry retirement must not enter provider environments');
  expect(!hosted.includes('CLOUDFLARE_'), 'Actions registry retirement job must not consume Cloudflare credentials');
  expect(hosted.includes('GITHUB_ACTIONS_WRITE_AUTH: ${{ github.token }}'), 'retirement must use only ephemeral github.token');
  expect(hosted.includes("printf '%s' \"$GITHUB_ACTIONS_WRITE_AUTH\" | node .github/scripts/ar11-actions-registry-retirement.mjs reconcile"), 'hosted-state must invoke only the canonical exact-id retirement reconciler');
  expect(hosted.indexOf('ar11-actions-registry-retirement.mjs reconcile') < hosted.indexOf('github-actions-registry.mjs live'), 'canonical live registry audit must run after retirement convergence');
  return errors;
}

function validateLiveRowsForRetirement(manifest, workflows) {
  if (!Array.isArray(workflows)) return ['live workflow registry must be an array'];
  const errors = [];
  for (const retirement of manifest.retirements) {
    const matches = workflows.filter((workflow) => workflow?.id === retirement.registration_id);
    if (matches.length !== 1) {
      errors.push(`retirement registration ${retirement.registration_id} must resolve exactly once; observed ${matches.length}`);
      continue;
    }
    const workflow = matches[0];
    if (workflow.path !== retirement.path) errors.push(`retirement registration ${retirement.registration_id} path mismatch: expected ${retirement.path}, observed ${String(workflow.path)}`);
    if (!['active', 'disabled_manually'].includes(workflow.state)) errors.push(`retirement registration ${retirement.registration_id} has unsupported state ${String(workflow.state)}`);
  }
  return errors;
}

async function githubJson(apiPath, token, options = {}) {
  const response = await fetch(`https://api.github.com${apiPath}`, {
    ...options,
    headers: {
      Accept: 'application/vnd.github+json',
      Authorization: `Bearer ${token}`,
      'User-Agent': 'part-crm-ar11-actions-retirement',
      'X-GitHub-Api-Version': '2022-11-28',
      ...(options.headers ?? {}),
    },
  });
  if (!response.ok && response.status !== 204) {
    const body = (await response.text()).slice(0, 1000);
    fail(`GitHub API ${apiPath} failed closed: HTTP ${response.status}: ${body}`);
  }
  if (response.status === 204) return null;
  return response.json();
}

async function listLiveWorkflows(token) {
  const workflows = [];
  let totalCount = null;
  for (let page = 1; page <= 100; page += 1) {
    const payload = await githubJson(`/repos/${REPOSITORY}/actions/workflows?per_page=100&page=${page}`, token);
    if (!Array.isArray(payload?.workflows) || !Number.isInteger(payload?.total_count)) fail('GitHub workflow list payload is malformed');
    if (totalCount === null) totalCount = payload.total_count;
    if (payload.total_count !== totalCount) fail('GitHub workflow registry changed during pagination');
    workflows.push(...payload.workflows);
    if (payload.workflows.length < 100) break;
    if (page === 100) fail('GitHub workflow registry pagination exceeded bound');
  }
  if (workflows.length !== totalCount) fail(`workflow registry pagination mismatch: expected ${totalCount}, observed ${workflows.length}`);
  return workflows;
}

async function tokenFromStdin() {
  process.stdin.setEncoding('utf8');
  let value = '';
  for await (const chunk of process.stdin) value += chunk;
  const token = value.trim();
  if (!token) fail('ephemeral GitHub Actions token must be provided on stdin');
  return token;
}

function report(errors) {
  for (const error of errors) console.error(error);
  return errors.length === 0;
}

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

function selfTest(manifest, trackedPaths, registry, baseAuthority, extension, workflowText) {
  const contractErrors = [
    ...validateManifest(manifest, trackedPaths, registry),
    ...validateEphemeralAuthority(baseAuthority, extension, workflowText),
  ];
  if (contractErrors.length !== 0) fail(`self-test requires valid contract: ${JSON.stringify(contractErrors)}`);
  const baseline = manifest.retirements.map((row) => ({ id: row.registration_id, path: row.path, state: 'active' }));
  if (validateLiveRowsForRetirement(manifest, baseline).length !== 0) fail('positive live retirement fixture failed');

  const expectReject = (mutate, marker) => {
    const rows = clone(baseline);
    mutate(rows);
    const errors = validateLiveRowsForRetirement(manifest, rows);
    if (!errors.some((error) => error.includes(marker))) fail(`negative fixture did not reject ${marker}: ${JSON.stringify(errors)}`);
  };
  expectReject((rows) => { rows[0].path = '.github/workflows/wrong.yml'; }, 'path mismatch');
  expectReject((rows) => { rows[0].state = 'deleted'; }, 'unsupported state');
  expectReject((rows) => { rows.pop(); }, 'resolve exactly once');

  const sourcePresent = [...trackedPaths, manifest.retirements[0].path].sort();
  const sourceErrors = validateManifest(manifest, sourcePresent, registry);
  if (!sourceErrors.some((error) => error.includes('still present on accepted source'))) fail('source-present negative fixture was not rejected');

  const authorityFixture = clone(registry);
  authorityFixture.active_registrations.push({ path: manifest.retirements[0].path, category: 'PERMANENT_REQUIRED' });
  const authorityErrors = validateManifest(manifest, trackedPaths, authorityFixture);
  if (!authorityErrors.some((error) => error.includes('canonical active authority'))) fail('active-authority negative fixture was not rejected');

  const permissionFixture = clone(extension);
  permissionFixture.ephemeral_runtime_credential_extensions[0].allowed_permission_boundary.pull_request_exposure = true;
  const permissionErrors = validateEphemeralAuthority(baseAuthority, permissionFixture, workflowText);
  if (!permissionErrors.some((error) => error.includes('pull-request Actions write exposure'))) fail('pull-request authority negative fixture was not rejected');
  const providerWorkflowFixture = workflowText.replace('GITHUB_ACTIONS_WRITE_AUTH: ${{ github.token }}', 'CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}\n          GITHUB_ACTIONS_WRITE_AUTH: ${{ github.token }}');
  const providerErrors = validateEphemeralAuthority(baseAuthority, extension, providerWorkflowFixture);
  if (!providerErrors.some((error) => error.includes('Cloudflare credentials'))) fail('provider credential exposure negative fixture was not rejected');
  console.log('AR-11 exact-id Actions retirement positive and fail-closed negative fixtures passed.');
}

async function main() {
  const command = process.argv[2] ?? 'contract';
  const root = DEFAULT_ROOT;
  const manifest = await readJson(path.join(root, MANIFEST_RELATIVE));
  const registry = await readJson(path.join(root, REGISTRY_RELATIVE));
  const baseAuthority = await readJson(path.join(root, BASE_CREDENTIAL_RELATIVE));
  const extension = await readJson(path.join(root, AR11_CREDENTIAL_RELATIVE));
  const workflowText = await readFile(path.join(root, GOVERNANCE_WORKFLOW_RELATIVE), 'utf8');
  const trackedPaths = await trackedWorkflowPaths(root);
  const contractErrors = [
    ...validateManifest(manifest, trackedPaths, registry),
    ...validateEphemeralAuthority(baseAuthority, extension, workflowText),
  ];
  if (!report(contractErrors)) return 1;

  if (command === 'contract') {
    console.log(`AR-11 Actions retirement manifest classifies ${manifest.retirements.length} exact obsolete registrations; source authority remains absent and Actions write is accepted-main-only.`);
    return 0;
  }
  if (command === 'self-test') {
    selfTest(manifest, trackedPaths, registry, baseAuthority, extension, workflowText);
    return 0;
  }
  if (command !== 'reconcile') {
    console.error(`unknown command: ${command}; expected contract, self-test, or reconcile`);
    return 2;
  }

  const repository = process.env.GITHUB_REPOSITORY ?? REPOSITORY;
  if (repository !== REPOSITORY) fail(`GITHUB_REPOSITORY must be ${REPOSITORY}; observed ${repository}`);
  const token = await tokenFromStdin();
  const before = await listLiveWorkflows(token);
  const liveErrors = validateLiveRowsForRetirement(manifest, before);
  if (!report(liveErrors)) return 1;

  let changed = 0;
  let alreadyDisabled = 0;
  for (const retirement of manifest.retirements) {
    const workflow = before.find((row) => row.id === retirement.registration_id);
    if (workflow.state === 'disabled_manually') {
      alreadyDisabled += 1;
      continue;
    }
    await githubJson(`/repos/${REPOSITORY}/actions/workflows/${retirement.registration_id}/disable`, token, { method: 'PUT' });
    changed += 1;
  }

  const after = await listLiveWorkflows(token);
  for (const retirement of manifest.retirements) {
    const matches = after.filter((workflow) => workflow?.id === retirement.registration_id);
    if (matches.length !== 1) fail(`post-reconcile registration ${retirement.registration_id} must remain historically addressable exactly once`);
    const workflow = matches[0];
    if (workflow.path !== retirement.path) fail(`post-reconcile path mismatch for ${retirement.registration_id}`);
    if (workflow.state !== 'disabled_manually') fail(`post-reconcile registration ${retirement.registration_id} must be disabled_manually; observed ${String(workflow.state)}`);
  }
  console.log(`AR11_ACTIONS_RETIREMENT_RECONCILED changed=${changed} already_disabled=${alreadyDisabled} historical_runs_preserved=true production_mutation=false provider_mutation=false`);
  return 0;
}

main().then((code) => { process.exitCode = code; }).catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});
