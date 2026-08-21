#!/usr/bin/env node

import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const EXECUTOR = '.github/workflows/d1-migration-executor.yml';
const PROMOTION = '.github/workflows/release-set-promotion.yml';
const WORKFLOWS = '.github/workflows';
const PINNED_WRANGLER = 'wrangler@4.94.0';
const SHARED_MUTATION_GROUP = 'release-set-promotion-staging';
const OBSERVE_REF = 'CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_OBSERVE_API_TOKEN }}';
const DEPLOY_REF = 'CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}';

function fail(message) {
  throw new Error(message);
}

function normalizedShell(text) {
  return text.replace(/\\\s*\n\s*/g, ' ');
}

function replaceFixture(label, text, from, to) {
  const mutated = text.replace(from, to);
  if (mutated === text) fail(`negative protected-executor fixture did not mutate source: ${label}`);
  return mutated;
}

async function workflowPaths(root) {
  const entries = await readdir(path.join(root, WORKFLOWS), { withFileTypes: true });
  return entries
    .filter((entry) => entry.isFile() && /\.ya?ml$/i.test(entry.name))
    .map((entry) => `${WORKFLOWS}/${entry.name}`)
    .sort();
}

function validateSharedMutationGroup(executorText, promotionText) {
  const marker = `group: ${SHARED_MUTATION_GROUP}`;
  if (!executorText.includes(marker)) {
    fail(`protected D1 executor lost shared staging provider mutation group: ${SHARED_MUTATION_GROUP}`);
  }
  if (!promotionText.includes(marker)) {
    fail(`Release Set Promotion lost shared staging provider mutation group: ${SHARED_MUTATION_GROUP}`);
  }
  if (!executorText.includes('cancel-in-progress: false') || !promotionText.includes('cancel-in-progress: false')) {
    fail('shared staging provider mutation authority must serialize without cancellation');
  }
}

async function validateExecutor(text, root = ROOT) {
  const promotionText = await readFile(path.join(root, PROMOTION), 'utf8');
  validateSharedMutationGroup(text, promotionText);

  const requiredMarkers = [
    'workflow_call:',
    'workflow_dispatch:',
    'environment: staging',
    `group: ${SHARED_MUTATION_GROUP}`,
    'cancel-in-progress: false',
    'authorize:',
    'needs: authorize',
    'test "$TARGET_ENVIRONMENT" = "staging"',
    'test "$GITHUB_REF" = "refs/heads/main"',
    'test "$GITHUB_SHA" = "$SOURCE_SHA"',
    'test "$MUTATION_AUTHORIZED" = "true"',
    'test "$CONFIRMATION" = "$SOURCE_SHA:$TARGET_ENVIRONMENT:$COMPONENT:$DATABASE_ID"',
    'd1 info',
    'SELECT id, name FROM d1_migrations ORDER BY id',
    'd1 status',
    'd1 plan',
    'd1 compatibility',
    'd1 time-travel info',
    'ledger-fence.json',
    'status-fence.json',
    'cmp --silent artifacts/d1-migration/status-before.json artifacts/d1-migration/status-fence.json',
    'd1 migrations apply',
    'd1 verify',
    'PRAGMA foreign_key_check',
    'PRAGMA integrity_check',
    "provider_mutation_executed': bool(plan.get('planned_migrations'))",
    "automatic_restore_executed': False",
    "secret_material_recorded': False",
    'actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a',
    '--experimental-provision=false',
    '--experimental-auto-create=false',
    'env -u CLOUDFLARE_API_TOKEN -u CLOUDFLARE_ACCOUNT_ID cargo run',
    OBSERVE_REF,
    DEPLOY_REF,
  ];
  for (const marker of requiredMarkers) {
    if (!text.includes(marker)) fail(`protected D1 executor lost required contract marker: ${marker}`);
  }

  if (text.includes('environment: ${{ inputs.environment }}')) {
    fail('AR-11 protected D1 executor must use the literal staging Environment, never dynamic production-capable binding');
  }
  if (text.includes('          - production')) {
    fail('AR-11 protected D1 executor must not expose production as a dispatch target');
  }

  const authorizeIndex = text.indexOf('\n  authorize:');
  const migrateIndex = text.indexOf('\n  migrate:');
  if (authorizeIndex < 0 || migrateIndex < 0 || authorizeIndex >= migrateIndex) {
    fail('input authorization must be a separate job before the protected mutation job');
  }
  const authorizeBody = text.slice(authorizeIndex, migrateIndex);
  if (authorizeBody.includes('environment:')) {
    fail('preflight authorization job must not bind any GitHub Environment');
  }
  if (authorizeBody.includes('CLOUDFLARE_API_TOKEN')) {
    fail('preflight authorization job must not receive any provider API token');
  }
  for (const marker of [
    'test "$TARGET_ENVIRONMENT" = "staging"',
    'test "$COMPONENT" = "catalog" || test "$COMPONENT" = "resolver"',
    'test "$MUTATION_AUTHORIZED" = "true"',
    'test "$CONFIRMATION" = "$SOURCE_SHA:$TARGET_ENVIRONMENT:$COMPONENT:$DATABASE_ID"',
  ]) {
    if (!authorizeBody.includes(marker)) fail(`preflight authorization lost fail-closed marker: ${marker}`);
  }

  const migrateStepsIndex = text.indexOf('\n    steps:', migrateIndex);
  if (migrateStepsIndex < 0) fail('protected mutation job steps are missing');
  const migrateHeader = text.slice(migrateIndex, migrateStepsIndex);
  if (!migrateHeader.includes('needs: authorize')) {
    fail('protected mutation job must depend on successful preflight authorization');
  }
  if (!migrateHeader.includes('environment: staging')) {
    fail('protected mutation job lost literal staging Environment binding');
  }
  if (!migrateHeader.includes('    env:\n')) {
    fail('protected D1 executor job-level metadata environment is missing');
  }
  if (migrateHeader.includes('CLOUDFLARE_API_TOKEN')) {
    fail('Cloudflare API tokens must not exist in the protected mutation job-level environment');
  }

  const forbiddenMarkers = [
    'd1 time-travel restore',
    'time-travel restore',
    'd1 create',
    'database create',
    'experimental-provision=true',
    'experimental-auto-create=true',
    'cancel-in-progress: true',
  ];
  for (const marker of forbiddenMarkers) {
    if (text.includes(marker)) fail(`protected D1 executor contains forbidden mutation/recovery marker: ${marker}`);
  }
  if (/create\s+table\s+[^\n;]*(?:d1|migration)[^\n;]*lock/is.test(text)) {
    fail('protected D1 executor must not invent a database-resident migration lock');
  }

  const normalized = normalizedShell(text);
  const applyPattern = new RegExp(`npx --yes ${PINNED_WRANGLER.replaceAll('.', '\\.')} d1 migrations apply\\b`, 'g');
  const applySites = normalized.match(applyPattern) ?? [];
  if (applySites.length !== 1) {
    fail(`protected D1 executor must contain exactly one pinned Wrangler apply site; observed=${applySites.length}`);
  }
  const remoteApplyPattern = new RegExp(`npx --yes ${PINNED_WRANGLER.replaceAll('.', '\\.')} d1 migrations apply\\b[^\\n]*?--remote\\b`, 'g');
  if ((normalized.match(remoteApplyPattern) ?? []).length !== 1) {
    fail('the sole protected D1 migration apply site must explicitly target --remote');
  }

  const providerPattern = new RegExp(`npx --yes ${PINNED_WRANGLER.replaceAll('.', '\\.')} d1 (?:info|execute|time-travel info|migrations apply)\\b`, 'g');
  const providerSites = normalized.match(providerPattern) ?? [];
  if (providerSites.length === 0) fail('protected D1 executor contains no pinned Wrangler provider operations');
  if ((normalized.match(/--experimental-provision=false/g) ?? []).length < providerSites.length) {
    fail('every protected provider operation must explicitly disable experimental provisioning');
  }
  if ((normalized.match(/--experimental-auto-create=false/g) ?? []).length < providerSites.length) {
    fail('every protected provider operation must explicitly disable experimental auto-create');
  }

  const observeSteps = (text.match(/CLOUDFLARE_API_TOKEN: \$\{\{ secrets\.CLOUDFLARE_OBSERVE_API_TOKEN \}\}/g) ?? []).length;
  if (observeSteps < 5) {
    fail(`read-only provider observations must use the dedicated observe credential; observed=${observeSteps}`);
  }
  const deploySteps = (text.match(/CLOUDFLARE_API_TOKEN: \$\{\{ secrets\.CLOUDFLARE_API_TOKEN \}\}/g) ?? []).length;
  if (deploySteps !== 1) {
    fail(`deploy-capable credential must appear in exactly one post-policy mutation step; observed=${deploySteps}`);
  }
  const policyIndex = text.indexOf('Run credential-free native plan, compatibility, and rollback-blocker gates');
  const deployIndex = text.indexOf(DEPLOY_REF);
  if (policyIndex < 0 || deployIndex < policyIndex) {
    fail('deploy-capable credential is materialized before native plan/compatibility');
  }
  const deployStepStart = text.lastIndexOf('\n      - name:', deployIndex);
  const deployStepEndCandidate = text.indexOf('\n      - name:', deployIndex);
  const deployStepEnd = deployStepEndCandidate < 0 ? text.length : deployStepEndCandidate;
  const deployStep = text.slice(deployStepStart, deployStepEnd);
  const fenceRead = deployStep.indexOf('ledger-fence.json');
  const fenceStatus = deployStep.indexOf('status-fence.json');
  const fenceCompare = deployStep.indexOf('cmp --silent artifacts/d1-migration/status-before.json artifacts/d1-migration/status-fence.json');
  const apply = deployStep.indexOf('d1 migrations apply');
  if (!(fenceRead >= 0 && fenceStatus > fenceRead && fenceCompare > fenceStatus && apply > fenceCompare)) {
    fail('deploy step must re-observe and compare the exact ledger fence before migrations apply');
  }

  const remoteMutationPaths = [];
  for (const workflowPath of await workflowPaths(root)) {
    const candidate = normalizedShell(await readFile(path.join(root, workflowPath), 'utf8'));
    if (/d1 migrations apply\b[^\n]*?--remote\b/.test(candidate)) remoteMutationPaths.push(workflowPath);
  }
  if (remoteMutationPaths.length !== 1 || remoteMutationPaths[0] !== EXECUTOR) {
    fail(`exactly one workflow may own remote D1 migrations apply; observed=${JSON.stringify(remoteMutationPaths)}`);
  }
}

async function expectRejected(label, text) {
  try {
    await validateExecutor(text, ROOT);
  } catch {
    return;
  }
  fail(`negative protected-executor fixture unexpectedly passed: ${label}`);
}

async function selfTest(text) {
  await validateExecutor(text, ROOT);
  const promotionText = await readFile(path.join(ROOT, PROMOTION), 'utf8');
  let sharedMutexRejected = false;
  try {
    validateSharedMutationGroup(
      text,
      replaceFixture(
        'promotion mutex drift',
        promotionText,
        `group: ${SHARED_MUTATION_GROUP}`,
        'group: unsafe-independent-promotion',
      ),
    );
  } catch {
    sharedMutexRejected = true;
  }
  if (!sharedMutexRejected) {
    fail('independent D1/promotion mutation-group fixture unexpectedly passed');
  }

  await expectRejected(
    'automatic restore',
    replaceFixture('automatic restore', text, 'd1 time-travel info', 'd1 time-travel restore'),
  );
  await expectRejected(
    'concurrency cancellation',
    replaceFixture('concurrency cancellation', text, 'cancel-in-progress: false', 'cancel-in-progress: true'),
  );
  await expectRejected(
    'provider auto-create',
    replaceFixture('provider auto-create', text, '--experimental-auto-create=false', '--experimental-auto-create=true'),
  );
  await expectRejected(
    'missing preflight dependency',
    replaceFixture('missing preflight dependency', text, '    needs: authorize\n', ''),
  );
  await expectRejected(
    'preflight environment binding',
    replaceFixture('preflight environment binding', text, '  authorize:\n', '  authorize:\n    environment: untrusted-input\n'),
  );
  await expectRejected(
    'preflight provider credential',
    replaceFixture(
      'preflight provider credential',
      text,
      '    env:\n      SOURCE_SHA:',
      '    env:\n      CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}\n      SOURCE_SHA:',
    ),
  );
  await expectRejected(
    'job-level provider credential',
    replaceFixture(
      'job-level provider credential',
      text,
      '    env:\n      CLOUDFLARE_ACCOUNT_ID:',
      '    env:\n      CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}\n      CLOUDFLARE_ACCOUNT_ID:',
    ),
  );
  await expectRejected(
    'dynamic protected environment',
    replaceFixture('dynamic protected environment', text, '    environment: staging\n', '    environment: ${{ inputs.environment }}\n'),
  );
  await expectRejected(
    'production dispatch target',
    replaceFixture('production dispatch target', text, '          - staging\n', '          - staging\n          - production\n'),
  );
  await expectRejected(
    'deploy token used for observation',
    replaceFixture('deploy token used for observation', text, OBSERVE_REF, DEPLOY_REF),
  );
  await expectRejected(
    'missing ledger fence compare',
    replaceFixture(
      'missing ledger fence compare',
      text,
      '          cmp --silent artifacts/d1-migration/status-before.json artifacts/d1-migration/status-fence.json\n',
      '',
    ),
  );
  await expectRejected(
    'second remote apply',
    `${text}\n# npx --yes ${PINNED_WRANGLER} d1 migrations apply X --remote --experimental-provision=false --experimental-auto-create=false\n`,
  );
  console.log('Protected D1 executor observe-before-mutate, shared mutation mutex and fail-closed negative fixtures rejected as expected.');
}

async function main() {
  const text = await readFile(path.join(ROOT, EXECUTOR), 'utf8');
  if (process.argv.includes('--self-test')) {
    await selfTest(text);
    return;
  }
  if (process.argv.length > 2) fail(`unknown arguments: ${process.argv.slice(2).join(' ')}`);
  await validateExecutor(text, ROOT);
  console.log('Protected D1 executor contract passed: staging-only, shared Release Set/D1 mutation mutex, read-only observe credential before native policy, one deploy credential after exact ledger fence, one remote mutation authority, pinned Wrangler, credential-free opsctl, no auto-provision/auto-create and no automatic restore.');
}

main().catch((error) => {
  console.error(`D1 executor gate error: ${error instanceof Error ? error.message : error}`);
  process.exitCode = 1;
});
