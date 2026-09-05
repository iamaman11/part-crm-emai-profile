#!/usr/bin/env node

import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const EXECUTOR = '.github/workflows/d1-migration-executor.yml';
const ADAPTER = 'scripts/d1-executor-plan.py';
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
  if (!executorText.includes(marker) || !promotionText.includes(marker)) {
    fail('Release Set promotion and protected D1 executor must share the exact staging mutation mutex');
  }
  if (!executorText.includes('cancel-in-progress: false') || !promotionText.includes('cancel-in-progress: false')) {
    fail('shared staging provider mutation authority must serialize without cancellation');
  }
}

async function validateExecutor(text, root = ROOT) {
  const promotionText = await readFile(path.join(root, PROMOTION), 'utf8');
  const adapterText = await readFile(path.join(root, ADAPTER), 'utf8');
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
    'test "$CONFIRMATION" = "$SOURCE_SHA:$TARGET_ENVIRONMENT:$COMPONENT:$DATABASE_ID:contract:$EXPECTED_RELEASE_SET_ID"',
    'transition_mode:',
    'expected_release_set_id:',
    "'migrations_dir': 'migrations-bounded'",
    'd1 repository',
    'd1 info',
    'SELECT id, name FROM d1_migrations ORDER BY id',
    'd1 status',
    'd1 plan',
    '--example d1-contract-transition',
    'FAIL_FORWARD_ONLY',
    'd1 compatibility',
    'd1-executor-plan.py materialize',
    'expected-pending.json',
    'ledger-before-names.json',
    'd1 migrations list',
    'd1-executor-plan.py verify-pending',
    'd1 time-travel info',
    'ledger-fence.json',
    'status-fence.json',
    'cmp --silent artifacts/d1-migration/status-before.json artifacts/d1-migration/status-fence.json',
    'ledger-fence-names.json',
    'cmp --silent artifacts/d1-migration/ledger-before-names.json artifacts/d1-migration/ledger-fence-names.json',
    'wrangler-pending-fence.txt',
    'd1 migrations apply',
    'expected-after.json',
    'ledger-after-names.json',
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

  const adapterMarkers = [
    'native D1 plan',
    'typed repository projection',
    'remote ledger is not an exact prefix',
    'ordinary d1 plan must never authorize the separate fail-forward CONTRACT',
    'contract-transition must authorize exactly the sole Catalog 0032 CONTRACT',
    'Wrangler pending list differs from native planned_migrations',
  ];
  for (const marker of adapterMarkers) {
    if (!adapterText.includes(marker)) fail(`exact-plan adapter lost fail-closed marker: ${marker}`);
  }

  for (const forbidden of [
    "'migrations_dir': '../../migrations/d1'",
    '"migrations_dir": "../../migrations/d1"',
    'migrations_dir = ../../migrations/d1',
    'd1 time-travel restore',
    'time-travel restore',
    'd1 create',
    'database create',
    'experimental-provision=true',
    'experimental-auto-create=true',
    'cancel-in-progress: true',
    '          - production',
    'environment: ${{ inputs.environment }}',
  ]) {
    if (text.includes(forbidden)) fail(`protected D1 executor contains forbidden marker: ${forbidden}`);
  }
  if (/create\s+table\s+[^\n;]*(?:d1|migration)[^\n;]*lock/is.test(text)) {
    fail('protected D1 executor must not invent a database-resident migration lock');
  }

  const authorizeIndex = text.indexOf('\n  authorize:');
  const migrateIndex = text.indexOf('\n  migrate:');
  if (authorizeIndex < 0 || migrateIndex < 0 || authorizeIndex >= migrateIndex) {
    fail('input authorization must be a separate job before the protected mutation job');
  }
  const authorizeBody = text.slice(authorizeIndex, migrateIndex);
  if (authorizeBody.includes('environment:')) fail('preflight authorization job must not bind any GitHub Environment');
  if (authorizeBody.includes('CLOUDFLARE_API_TOKEN')) fail('preflight authorization job must not receive provider credentials');
  for (const marker of [
    'test "$TARGET_ENVIRONMENT" = "staging"',
    'test "$COMPONENT" = "catalog" || test "$COMPONENT" = "resolver"',
    'test "$MUTATION_AUTHORIZED" = "true"',
    'test "$COMPONENT" = "catalog"',
    'test -n "$EXPECTED_RELEASE_SET_ID"',
  ]) {
    if (!authorizeBody.includes(marker)) fail(`preflight authorization lost fail-closed marker: ${marker}`);
  }

  const migrateStepsIndex = text.indexOf('\n    steps:', migrateIndex);
  if (migrateStepsIndex < 0) fail('protected mutation job steps are missing');
  const migrateHeader = text.slice(migrateIndex, migrateStepsIndex);
  if (!migrateHeader.includes('needs: authorize')) fail('protected mutation job must depend on preflight authorization');
  if (!migrateHeader.includes('environment: staging')) fail('protected mutation job lost literal staging Environment');
  if (!migrateHeader.includes('    env:\n')) fail('protected D1 executor job-level metadata environment is missing');
  if (migrateHeader.includes('CLOUDFLARE_API_TOKEN')) fail('provider API tokens must remain step-scoped');

  const normalized = normalizedShell(text);
  const escapedWrangler = PINNED_WRANGLER.replaceAll('.', '\\.');
  const applyPattern = new RegExp(`npx --yes ${escapedWrangler} d1 migrations apply\\b`, 'g');
  if ((normalized.match(applyPattern) ?? []).length !== 1) {
    fail('protected D1 executor must contain exactly one pinned Wrangler migrations apply site');
  }
  const remoteApplyPattern = new RegExp(`npx --yes ${escapedWrangler} d1 migrations apply\\b[^\\n]*?--remote\\b`, 'g');
  if ((normalized.match(remoteApplyPattern) ?? []).length !== 1) {
    fail('the sole protected D1 migration apply site must explicitly target --remote');
  }
  const providerPattern = new RegExp(
    `npx --yes ${escapedWrangler} d1 (?:info|execute|time-travel info|migrations list|migrations apply)\\b`,
    'g',
  );
  const providerSites = normalized.match(providerPattern) ?? [];
  if (providerSites.length === 0) fail('protected D1 executor contains no pinned Wrangler provider operations');
  if ((normalized.match(/--experimental-provision=false/g) ?? []).length < providerSites.length) {
    fail('every protected provider operation must explicitly disable experimental provisioning');
  }
  if ((normalized.match(/--experimental-auto-create=false/g) ?? []).length < providerSites.length) {
    fail('every protected provider operation must explicitly disable experimental auto-create');
  }

  const observeSteps = (text.match(/CLOUDFLARE_API_TOKEN: \$\{\{ secrets\.CLOUDFLARE_OBSERVE_API_TOKEN \}\}/g) ?? []).length;
  if (observeSteps < 6) {
    fail(`provider observations must use the dedicated observe credential; observed=${observeSteps}`);
  }
  const deploySteps = (text.match(/CLOUDFLARE_API_TOKEN: \$\{\{ secrets\.CLOUDFLARE_API_TOKEN \}\}/g) ?? []).length;
  if (deploySteps !== 1) fail(`deploy-capable credential must appear exactly once; observed=${deploySteps}`);

  const policyIndex = text.indexOf('Run credential-free native plan, compatibility, and rollback-blocker gates');
  const materializeIndex = text.indexOf('Materialize exactly native planned_migrations from typed successor projection');
  const firstPendingIndex = text.indexOf('Compare Wrangler pending list to exact native plan with observe credential');
  const deployIndex = text.indexOf(DEPLOY_REF);
  if (!(policyIndex >= 0 && materializeIndex > policyIndex && firstPendingIndex > materializeIndex && deployIndex > firstPendingIndex)) {
    fail('exact native policy, bounded materialization and read-only pending proof must all precede deploy credentials');
  }

  const deployStepStart = text.lastIndexOf('\n      - name:', deployIndex);
  const deployStepEndCandidate = text.indexOf('\n      - name:', deployIndex);
  const deployStepEnd = deployStepEndCandidate < 0 ? text.length : deployStepEndCandidate;
  const deployStep = text.slice(deployStepStart, deployStepEnd);
  const fenceRead = deployStep.indexOf('ledger-fence.json');
  const fenceStatus = deployStep.indexOf('status-fence.json');
  const statusCompare = deployStep.indexOf('cmp --silent artifacts/d1-migration/status-before.json artifacts/d1-migration/status-fence.json');
  const ledgerCompare = deployStep.indexOf('cmp --silent artifacts/d1-migration/ledger-before-names.json artifacts/d1-migration/ledger-fence-names.json');
  const pendingList = deployStep.indexOf('wrangler-pending-fence.txt');
  const pendingVerify = deployStep.indexOf('d1-executor-plan.py verify-pending');
  const apply = deployStep.indexOf('d1 migrations apply');
  if (!(fenceRead >= 0 && fenceStatus > fenceRead && statusCompare > fenceStatus && ledgerCompare > statusCompare && pendingList > ledgerCompare && pendingVerify > pendingList && apply > pendingVerify)) {
    fail('deploy step must re-fence provider identity/ledger and Wrangler pending list before apply');
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
  let mutexRejected = false;
  try {
    validateSharedMutationGroup(
      text,
      replaceFixture('promotion mutex drift', promotionText, `group: ${SHARED_MUTATION_GROUP}`, 'group: unsafe-independent-promotion'),
    );
  } catch {
    mutexRejected = true;
  }
  if (!mutexRejected) fail('independent D1/promotion mutation-group fixture unexpectedly passed');

  for (const [label, from, to] of [
    ['automatic restore', 'd1 time-travel info', 'd1 time-travel restore'],
    ['concurrency cancellation', 'cancel-in-progress: false', 'cancel-in-progress: true'],
    ['provider auto-create', '--experimental-auto-create=false', '--experimental-auto-create=true'],
    ['raw legacy Catalog execution root', "'migrations_dir': 'migrations-bounded'", "'migrations_dir': '../../migrations/d1'"],
    ['missing normalized ledger fence', 'cmp --silent artifacts/d1-migration/ledger-before-names.json artifacts/d1-migration/ledger-fence-names.json', '# removed-ledger-fence'],
    ['missing Wrangler pending fence', 'd1-executor-plan.py verify-pending', 'd1-executor-plan.py removed-pending-check'],
    ['missing contract confirmation binding', 'test "$CONFIRMATION" = "$SOURCE_SHA:$TARGET_ENVIRONMENT:$COMPONENT:$DATABASE_ID:contract:$EXPECTED_RELEASE_SET_ID"', 'test -n "$CONFIRMATION"'],
  ]) {
    await expectRejected(label, replaceFixture(label, text, from, to));
  }
  await expectRejected(
    'deploy token used for observation',
    replaceFixture('deploy token used for observation', text, OBSERVE_REF, DEPLOY_REF),
  );
  await expectRejected(
    'second remote apply',
    `${text}\n# npx --yes ${PINNED_WRANGLER} d1 migrations apply X --remote --experimental-provision=false --experimental-auto-create=false\n`,
  );
  console.log('Protected D1 executor exact-plan, bounded-lineage, pending-list and double-ledger-fence negative fixtures passed.');
}

async function main() {
  const text = await readFile(path.join(ROOT, EXECUTOR), 'utf8');
  if (process.argv.includes('--self-test')) {
    await selfTest(text);
    return;
  }
  if (process.argv.length > 2) fail(`unknown arguments: ${process.argv.slice(2).join(' ')}`);
  await validateExecutor(text, ROOT);
  console.log('Protected D1 executor contract passed: staging-only, exact native plan materialization, bounded successor lineage, Wrangler pending equality, provider/ledger re-fence, one remote apply owner, no automatic restore or provisioning.');
}

main().catch((error) => {
  console.error(`D1 executor gate error: ${error instanceof Error ? error.message : error}`);
  process.exitCode = 1;
});
