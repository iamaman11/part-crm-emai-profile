#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';

const ROOT = process.cwd();
const AUTHORITY = 'architecture/ar8-d-secret-transport-successor.json';
const D3_AUTHORITY = 'architecture/pre2j-d3-resolver-bootstrap-authority.json';
const D3_MARKER = 'architecture/pre2j-d3-resolver-bootstrap-implementation.json';
const AR11_AUTHORITY = 'architecture/release-architecture-ar11.json';
const AR11_CHECKER = '.github/scripts/release-operational-ar11.mjs';
const AR11_PROMOTION = '.github/workflows/release-set-promotion.yml';
const WRANGLER_SOURCE = 'deploy/cloudflare/wrangler.jsonc';
const CORE_OVERLAY = 'scripts/release-core-overlay-ar11.py';
const BINDING_HELPER = '.github/scripts/worker-secret-bindings.mjs';
const LEGACY_WORKFLOW = '.github/workflows/mailbox-secret-resolver-promotion.yml';
const LEGACY_WRAPPER = 'scripts/mailbox-secret-resolver-promotion.py';
const LEGACY_CORE = 'scripts/_mailbox_secret_resolver_promotion_core.py';
const HISTORICAL_CHECKER = 'scripts/check-pre2j-d3-resolver-bootstrap-implementation-historical';
const CURRENT_D3_CHECKER = 'scripts/check-pre2j-d3-resolver-bootstrap-implementation.py';
const QUALITY_GATE = '.github/workflows/quality-gate.yml';
const FAST_VERIFY = 'scripts/verify-fast.py';
const EXPECTED_TRANSITION_BASE = '9635ef21aafa0e2ff04551ef4cecf9497cbc87d5';
const EXPECTED_WORKFLOW_BLOB = '85fd78557c97c96c179ff5d45f338bf12e639305';

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

function transitionIdentityErrors(authority) {
  if (!authority || typeof authority !== 'object' || Array.isArray(authority)) return ['D3 transition record is missing or malformed'];
  const errors = [];
  if (authority.schema_version !== 1 || authority.kind !== 'POLICY_TRANSITION' || authority.tracking_issue !== 361) errors.push('D3 transition record identity/version drifted');
  const predecessor = authority.predecessor ?? {};
  if (predecessor.d3_authority !== D3_AUTHORITY || predecessor.d3_implementation_marker !== D3_MARKER) errors.push('D3 predecessor authority/marker identity drifted');
  if (!/^[0-9a-f]{40}$/.test(String(predecessor.transition_base_main ?? ''))) errors.push('D3 predecessor transition SHA is missing or malformed');
  else if (predecessor.transition_base_main !== EXPECTED_TRANSITION_BASE) errors.push('D3 predecessor transition SHA changed from the governed transition');
  if (!/^[0-9a-f]{40}$/.test(String(predecessor.promotion_workflow_git_blob_sha ?? ''))) errors.push('D3 historical promotion workflow blob identity is missing or malformed');
  else if (predecessor.promotion_workflow_git_blob_sha !== EXPECTED_WORKFLOW_BLOB) errors.push('D3 historical promotion workflow blob identity drifted');
  if (predecessor.promotion_workflow !== LEGACY_WORKFLOW) errors.push('D3 historical promotion workflow path drifted');
  const successor = authority.successor ?? {};
  if (
    successor.policy !== 'AR-11 Release Set promotion with steady-state Worker secret binding verification' ||
    successor.promotion_workflow !== AR11_PROMOTION ||
    successor.release_architecture_authority !== AR11_AUTHORITY ||
    successor.operational_checker !== AR11_CHECKER ||
    successor.binding_metadata_helper !== BINDING_HELPER ||
    successor.worker_secret_value_authority !== 'Cloudflare Worker secret store' ||
    successor.routine_deploy_secret_value_transport !== false ||
    successor.routine_deploy_secret_mutation !== false ||
    successor.rotation_lifecycle !== 'separate_explicit_rotation_authority'
  ) errors.push('current D3 secret-transport successor policy is missing or drifted');
  return errors;
}

function historicalProvenanceErrors(authority) {
  const errors = [];
  const predecessor = authority.predecessor ?? {};
  const ref = String(predecessor.transition_base_main ?? '');
  if (!/^[0-9a-f]{40}$/.test(ref)) return ['D3 predecessor transition SHA is not an exact commit'];
  ensureCommit(ref);
  const workflowBlob = run('git', ['rev-parse', `${ref}:${LEGACY_WORKFLOW}`], { check: false });
  if (workflowBlob.status !== 0 || workflowBlob.stdout.trim() !== predecessor.promotion_workflow_git_blob_sha) errors.push('historical D3 promotion workflow blob does not match the governed transition base');
  for (const relative of [D3_AUTHORITY, D3_MARKER]) {
    const historical = run('git', ['show', `${ref}:${relative}`], { check: false });
    if (historical.status !== 0 || historical.stdout.replace(/\r\n?/g, '\n') !== read(relative)) errors.push(`historical D3 provenance bytes drifted after transition: ${relative}`);
  }
  return errors;
}

function retiredAuthorityErrors(exists = existsSync) {
  const errors = [];
  for (const relative of [LEGACY_WORKFLOW, LEGACY_WRAPPER, LEGACY_CORE, HISTORICAL_CHECKER]) {
    if (exists(path.join(ROOT, relative))) errors.push(`retired D3 executable authority must be absent from current tree: ${relative}`);
  }
  return errors;
}

function logicalShellLines(source) {
  return source.replace(/\\\n\s*/g, ' ').split('\n').map((line) => line.trim()).filter(Boolean);
}

function secretObservationErrors(promotion) {
  const commands = logicalShellLines(promotion).filter((line) => line.includes('wrangler@4.94.0 secret list'));
  const errors = [];
  if (commands.length !== 2) errors.push(`current Release Set promotion must contain exactly two secret-name observations; observed=${commands.length}`);
  for (const command of commands) {
    for (const required of ['--format json', '--config "$WRANGLER_CONFIG"', '--env staging']) {
      if (!command.includes(required)) errors.push(`Worker secret observation lacks ${required}`);
    }
    if (/\s--name(?:\s|=)/.test(command)) errors.push('Worker secret observation redundantly overrides the rendered env-owned Worker name');
    if (/\s(?:put|bulk|delete)\b/.test(command)) errors.push('Worker secret observation contains mutation semantics');
  }
  return errors;
}

function renderedWorkerNameErrors(wranglerSource, overlaySource) {
  const errors = [];
  let config;
  try {
    config = JSON.parse(wranglerSource);
  } catch (error) {
    return [`canonical Wrangler source is not parseable JSON: ${error instanceof Error ? error.message : String(error)}`];
  }
  const stagingName = config?.env?.staging?.name;
  if (stagingName !== '${STAGING_WORKER_NAME}') {
    errors.push(`canonical Wrangler staging name must be exactly the one deploy-manifest placeholder; observed=${JSON.stringify(stagingName)}`);
  }
  if (String(stagingName ?? '').includes('staging-staging')) {
    errors.push('canonical Wrangler staging name contains a duplicated environment suffix');
  }
  const projectionLines = overlaySource
    .split('\n')
    .filter((line) => /^\s*"\$\{STAGING_WORKER_NAME\}"\s*:/.test(line));
  if (projectionLines.length !== 1) {
    errors.push(`AR-11 overlay must contain exactly one STAGING_WORKER_NAME projection; observed=${projectionLines.length}`);
  } else if (!/^\s*"\$\{STAGING_WORKER_NAME\}"\s*:\s*manifest\["worker_name"\]\s*,?\s*$/.test(projectionLines[0])) {
    errors.push('AR-11 overlay must project deploy-manifest worker_name as the exact staging Worker name without suffixes or expressions');
  }
  if (!overlaySource.includes('return replacements.get(value, value)')) {
    errors.push('AR-11 overlay no longer performs exact placeholder substitution semantics');
  }
  return errors;
}

function promotionPolicyErrors(promotion) {
  const errors = [];
  const forbidden = [
    'CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON',
    'CLOUDFLARE_RESOLVER_SECRETS_JSON',
    ' secret put ', ' secret bulk ', ' secret delete ', ' secrets put ', ' secrets bulk ', ' secrets delete ',
    'worker-build --release', 'cargo build', 'run: npm run build', 'environment: production',
    LEGACY_WORKFLOW, LEGACY_WRAPPER, LEGACY_CORE, HISTORICAL_CHECKER,
  ];
  const lowered = ` ${promotion.toLowerCase()} `;
  for (const marker of forbidden) if (lowered.includes(marker.toLowerCase())) errors.push(`current Release Set promotion contains forbidden legacy/mutation authority: ${marker}`);
  errors.push(...secretObservationErrors(promotion));
  for (const marker of [
    'workflow_run:',
    '- Release Set Build',
    'AR11_READY_TO_MUTATE',
    'WORKER_PROMOTION_AUTHORIZATION_V2',
    'promotion preflight',
    'promotion verify',
    'release-set-promotion-staging',
  ]) {
    if (!promotion.includes(marker)) errors.push(`current Release Set promotion is missing semantic invariant ${JSON.stringify(marker)}`);
  }

  const preflightJob = promotion.indexOf('\n  preflight-ready:\n');
  const ready = promotion.indexOf('kind:"AR11_READY_TO_MUTATE"', preflightJob);
  const resolveVerifyJob = promotion.indexOf('\n  resolve-verify:\n');
  const authorization = promotion.indexOf('Resolve exact one-shot authorization only after READY exists', resolveVerifyJob);
  const mutateJob = promotion.indexOf('\n  mutate:\n');
  const deployCredential = promotion.indexOf('DEPLOY_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}', mutateJob);
  const deploy = promotion.indexOf('Deploy exact Release Set v3 bits after all fences', mutateJob);
  if (!(preflightJob >= 0 && ready > preflightJob && resolveVerifyJob > ready && authorization > resolveVerifyJob && mutateJob > authorization && deployCredential > mutateJob && deploy > deployCredential)) {
    errors.push('release successor must order read-only READY -> authorization binding -> deploy credential -> mutation');
  }
  return errors;
}

function replayDependencyErrors(label, source) {
  const errors = [];
  for (const marker of [HISTORICAL_CHECKER, LEGACY_WRAPPER, LEGACY_CORE, 'git checkout']) {
    if (source.includes(marker)) errors.push(`${label} still depends on retired D3 executable machinery: ${marker}`);
  }
  if (label === CURRENT_D3_CHECKER) {
    for (const marker of ['git show', 'write_bytes(predecessor_', 'run_historical(']) if (source.includes(marker)) errors.push(`${label} still replays historical D3 implementation: ${marker}`);
  }
  return errors;
}

function currentSuccessorErrors() {
  const errors = [];
  for (const relative of [AR11_AUTHORITY, AR11_CHECKER, AR11_PROMOTION, WRANGLER_SOURCE, CORE_OVERLAY, BINDING_HELPER, CURRENT_D3_CHECKER]) {
    if (!existsSync(path.join(ROOT, relative))) errors.push(`missing current D3/release successor artifact: ${relative}`);
  }
  if (errors.length > 0) return errors;
  errors.push(...promotionPolicyErrors(read(AR11_PROMOTION)));
  errors.push(...renderedWorkerNameErrors(read(WRANGLER_SOURCE), read(CORE_OVERLAY)));
  for (const relative of [CURRENT_D3_CHECKER, QUALITY_GATE, FAST_VERIFY]) {
    if (!existsSync(path.join(ROOT, relative))) { errors.push(`missing current D3 caller: ${relative}`); continue; }
    errors.push(...replayDependencyErrors(relative, read(relative)));
  }
  const checker = run('node', [AR11_CHECKER], { check: false });
  if (checker.status !== 0) errors.push(`current Release Set operational authority failed: ${(checker.stderr || checker.stdout).trim()}`);
  return errors;
}

function validate() {
  if (!existsSync(path.join(ROOT, AUTHORITY))) return ['D3 transition record is missing'];
  const authority = JSON.parse(read(AUTHORITY));
  const errors = [];
  errors.push(...transitionIdentityErrors(authority));
  if (errors.length === 0) errors.push(...historicalProvenanceErrors(authority));
  errors.push(...retiredAuthorityErrors());
  errors.push(...currentSuccessorErrors());
  return errors;
}

function selfTest() {
  const authority = JSON.parse(read(AUTHORITY));
  if (transitionIdentityErrors(null).length === 0) throw new Error('missing transition record negative fixture passed');
  const malformedSha = structuredClone(authority); malformedSha.predecessor.transition_base_main = 'main';
  if (transitionIdentityErrors(malformedSha).length === 0) throw new Error('malformed predecessor SHA negative fixture passed');
  const missingBlob = structuredClone(authority); delete missingBlob.predecessor.promotion_workflow_git_blob_sha;
  if (transitionIdentityErrors(missingBlob).length === 0) throw new Error('missing historical blob identity negative fixture passed');
  const missingSuccessor = structuredClone(authority); missingSuccessor.successor = {};
  if (transitionIdentityErrors(missingSuccessor).length === 0) throw new Error('missing successor policy negative fixture passed');
  for (const retired of [LEGACY_WORKFLOW, LEGACY_WRAPPER, LEGACY_CORE, HISTORICAL_CHECKER]) {
    const errors = retiredAuthorityErrors((candidate) => candidate.endsWith(retired));
    if (errors.length === 0) throw new Error(`retired executable restoration negative fixture passed: ${retired}`);
  }
  if (promotionPolicyErrors(read(AR11_PROMOTION) + '\nCLOUDFLARE_RESOLVER_SECRETS_JSON\n').length === 0) throw new Error('superseded secret-bundle input negative fixture passed');
  const redundantNamePromotion = read(AR11_PROMOTION).replace('secret list --format json', 'secret list --name "$worker_name" --format json');
  if (promotionPolicyErrors(redundantNamePromotion).length === 0) throw new Error('redundant env-owned Worker name override negative fixture passed');
  const wranglerSource = read(WRANGLER_SOURCE);
  const duplicatedEnvName = wranglerSource.replace('"name": "${STAGING_WORKER_NAME}"', '"name": "${STAGING_WORKER_NAME}-staging"');
  if (duplicatedEnvName === wranglerSource) throw new Error('duplicated staging Worker suffix fixture setup failed');
  if (renderedWorkerNameErrors(duplicatedEnvName, read(CORE_OVERLAY)).length === 0) throw new Error('duplicated staging Worker suffix fixture passed');
  const overlaySource = read(CORE_OVERLAY);
  const suffixedOverlay = overlaySource.replace('"${STAGING_WORKER_NAME}": manifest["worker_name"]', '"${STAGING_WORKER_NAME}": manifest["worker_name"] + "-staging"');
  if (suffixedOverlay === overlaySource) throw new Error('suffixed rendered Worker-name projection fixture setup failed');
  if (renderedWorkerNameErrors(wranglerSource, suffixedOverlay).length === 0) throw new Error('suffixed rendered Worker-name projection fixture passed');
  if (promotionPolicyErrors(read(AR11_PROMOTION) + `\n${HISTORICAL_CHECKER}\n`).length === 0) throw new Error('historical implementation promotion dependency negative fixture passed');
  if (replayDependencyErrors(CURRENT_D3_CHECKER, `git show x\n${HISTORICAL_CHECKER}\n`).length === 0) throw new Error('historical executable replay negative fixture passed');

  const promotion = read(AR11_PROMOTION);
  const cosmeticLabel = 'Observe provider and publish lossless read-only terminal outcome';
  if (promotion.includes(cosmeticLabel)) {
    const renamedPromotion = promotion.replace(cosmeticLabel, 'Cosmetic AR11 preflight label');
    if (promotionPolicyErrors(renamedPromotion).length !== 0) throw new Error('cosmetic AR11 label unexpectedly defines successor ordering');
  }
  const deployCredential = 'DEPLOY_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}';
  const authorization = 'Resolve exact one-shot authorization only after READY exists';
  const withoutDeployCredential = promotion.replace(deployCredential, 'DEPLOY_TOKEN_REMOVED_FOR_ORDER_FIXTURE');
  const reorderedPromotion = withoutDeployCredential.replace(authorization, `${deployCredential}\n      - name: ${authorization}`);
  if (!promotionPolicyErrors(reorderedPromotion).some((error) => error.includes('must order read-only READY'))) {
    throw new Error('deploy-credential-before-authorization negative fixture passed');
  }

  const operationalSelfTest = run('node', [AR11_CHECKER, '--self-test'], { check: false });
  if (operationalSelfTest.status !== 0) throw new Error(`Release Set operational negative matrix failed: ${(operationalSelfTest.stderr || operationalSelfTest.stdout).trim()}`);
  console.log('Static D3 transition provenance, exact rendered Worker-name, and semantic secret-transport negative matrix passed.');
}

if (process.argv.includes('--self-test')) {
  try { selfTest(); process.exit(0); }
  catch (error) { console.error(`Static D3 transition negative self-test failed: ${error instanceof Error ? error.message : String(error)}`); process.exit(1); }
}

try {
  const errors = validate();
  if (errors.length > 0) { console.error('Static D3 transition/current successor gate failed:\n' + errors.map((error) => `- ${error}`).join('\n')); process.exit(1); }
  console.log('Static D3 transition provenance and current Release Set secret-transport successor passed.');
} catch (error) {
  console.error(`Static D3 transition/current successor gate failed: ${error instanceof Error ? error.message : String(error)}`);
  process.exit(1);
}
