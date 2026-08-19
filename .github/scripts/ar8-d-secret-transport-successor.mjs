#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';

const ROOT = process.cwd();
const AUTHORITY = 'architecture/ar8-d-secret-transport-successor.json';
const D3_AUTHORITY = 'architecture/pre2j-d3-resolver-bootstrap-authority.json';
const D3_MARKER = 'architecture/pre2j-d3-resolver-bootstrap-implementation.json';
const AR11_CHECKER = '.github/scripts/release-operational-ar11.mjs';
const AR11_PROMOTION = '.github/workflows/release-set-promotion.yml';
const BINDING_HELPER = '.github/scripts/worker-secret-bindings.mjs';
const LEGACY_WORKFLOW = '.github/workflows/mailbox-secret-resolver-promotion.yml';

function read(relative) {
  return readFileSync(path.join(ROOT, relative), 'utf8').replace(/\r\n?/g, '\n');
}
function run(command, args, { check = true } = {}) {
  const result = spawnSync(command, args, { cwd: ROOT, encoding: 'utf8', windowsHide: true });
  const status = result.status ?? -1;
  if (check && (result.error || status !== 0)) {
    throw result.error ?? new Error(`${command} ${args.join(' ')} failed (${status}): ${(result.stderr || result.stdout || '').trim()}`);
  }
  return { status, stdout: result.stdout ?? '', stderr: result.stderr ?? '' };
}
function ensureCommit(ref) {
  if (run('git', ['cat-file', '-e', `${ref}^{commit}`], { check: false }).status === 0) return;
  run('git', ['fetch', '--no-tags', '--depth=1', 'origin', ref], { check: false });
  if (run('git', ['cat-file', '-e', `${ref}^{commit}`], { check: false }).status !== 0) {
    throw new Error(`governed predecessor commit is unavailable: ${ref}`);
  }
}

function historicalErrors(authority) {
  const errors = [];
  const predecessor = authority.predecessor ?? {};
  const ref = String(predecessor.transition_base_main ?? '');
  if (!/^[0-9a-f]{40}$/.test(ref)) return ['AR-8D transition base is not an exact commit'];
  ensureCommit(ref);
  const workflowBlob = run('git', ['rev-parse', `${ref}:${LEGACY_WORKFLOW}`], { check: false });
  if (workflowBlob.status !== 0 || workflowBlob.stdout.trim() !== predecessor.promotion_workflow_git_blob_sha) {
    errors.push('historical D3 promotion workflow blob drifted from governed transition base');
  }
  for (const relative of [D3_AUTHORITY, D3_MARKER]) {
    const historical = run('git', ['show', `${ref}:${relative}`], { check: false });
    if (historical.status !== 0 || historical.stdout.replace(/\r\n?/g, '\n') !== read(relative)) {
      errors.push(`historical D3 authority changed after AR-8D transition: ${relative}`);
    }
  }
  return errors;
}

function currentSuccessorErrors() {
  const errors = [];
  for (const relative of [AR11_CHECKER, AR11_PROMOTION, BINDING_HELPER]) {
    if (!existsSync(path.join(ROOT, relative))) errors.push(`missing current successor artifact: ${relative}`);
  }
  if (errors.length > 0) return errors;
  const promotion = read(AR11_PROMOTION);
  const forbidden = [
    'CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON',
    'CLOUDFLARE_RESOLVER_SECRETS_JSON',
    ' secret put ',
    ' secret bulk ',
    ' secret delete ',
    ' secrets put ',
    ' secrets bulk ',
    ' secrets delete ',
    'worker-build --release',
    'cargo build',
    'npm run build',
    'environment: production',
  ];
  const lowered = ` ${promotion.toLowerCase()} `;
  for (const marker of forbidden) {
    if (lowered.includes(marker.toLowerCase())) errors.push(`current AR-11 promotion contains forbidden legacy/secret authority: ${marker}`);
  }
  for (const marker of ['wrangler@4.94.0 secret list', 'promotion preflight', 'promotion verify', 'release-set-promotion-staging']) {
    if (!promotion.includes(marker)) errors.push(`current AR-11 promotion is missing ${JSON.stringify(marker)}`);
  }
  const checker = run('node', [AR11_CHECKER, '--pre-cutover'], { check: false });
  if (checker.status !== 0) errors.push(`AR-11 successor checker failed: ${(checker.stderr || checker.stdout).trim()}`);
  return errors;
}

function validate() {
  const authority = JSON.parse(read(AUTHORITY));
  const errors = [];
  if (authority.schema_version !== 1 || authority.kind !== 'POLICY_TRANSITION' || authority.tracking_issue !== 361) {
    errors.push('AR-8D transition authority identity/version drifted');
  }
  const predecessor = authority.predecessor ?? {};
  if (
    predecessor.d3_authority !== D3_AUTHORITY ||
    predecessor.d3_implementation_marker !== D3_MARKER ||
    predecessor.promotion_workflow !== LEGACY_WORKFLOW ||
    predecessor.promotion_workflow_git_blob_sha !== '85fd78557c97c96c179ff5d45f338bf12e639305'
  ) {
    errors.push('AR-8D predecessor identity drifted');
  }
  const successor = authority.successor ?? {};
  if (
    successor.worker_secret_value_authority !== 'Cloudflare Worker secret store' ||
    successor.routine_deploy_secret_value_transport !== false ||
    successor.routine_deploy_secret_mutation !== false ||
    successor.rotation_lifecycle !== 'separate_explicit_rotation_authority'
  ) {
    errors.push('AR-8D secret transport successor invariants drifted');
  }
  errors.push(...historicalErrors(authority));
  errors.push(...currentSuccessorErrors());
  return errors;
}

if (process.argv.includes('--self-test')) {
  const promotion = read(AR11_PROMOTION) + '\n secret put X\n';
  if (!` ${promotion.toLowerCase()} `.includes(' secret put ')) throw new Error('negative fixture unexpectedly passed');
  console.log('AR-8D historical-to-AR-11 successor negative self-test passed.');
  process.exit(0);
}

try {
  const errors = validate();
  if (errors.length > 0) {
    console.error('AR-8D historical/successor gate failed:\n' + errors.map((error) => `- ${error}`).join('\n'));
    process.exit(1);
  }
  console.log('AR-8D historical D3 evidence and current AR-11 secret-transport successor passed.');
} catch (error) {
  console.error(`AR-8D historical/successor gate failed: ${error instanceof Error ? error.message : String(error)}`);
  process.exit(1);
}
