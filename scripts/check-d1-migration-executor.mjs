#!/usr/bin/env node

import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const EXECUTOR = '.github/workflows/d1-migration-executor.yml';
const WORKFLOWS = '.github/workflows';
const PINNED_WRANGLER = 'wrangler@4.94.0';

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

async function validateExecutor(text, root = ROOT) {
  const requiredMarkers = [
    'workflow_call:',
    'workflow_dispatch:',
    'environment: ${{ inputs.environment }}',
    'group: d1-migration-${{ inputs.environment }}-${{ inputs.component }}-${{ inputs.database_id }}',
    'cancel-in-progress: false',
    'authorize:',
    'needs: authorize',
    'test "$TARGET_ENVIRONMENT" = "staging" || test "$TARGET_ENVIRONMENT" = "production"',
    'test "$GITHUB_REF" = "refs/heads/main"',
    'test "$GITHUB_SHA" = "$SOURCE_SHA"',
    'test "$MUTATION_AUTHORIZED" = "true"',
    'test "$CONFIRMATION" = "$SOURCE_SHA:$TARGET_ENVIRONMENT:$COMPONENT:$DATABASE_ID"',
    'docs/status.json',
    'production_authorized',
    'production D1 mutation remains blocked before Production Core authorization',
    'd1 info',
    'SELECT id, name FROM d1_migrations ORDER BY id',
    'd1 status',
    'd1 plan',
    'd1 compatibility',
    'd1 time-travel info',
    'd1 migrations apply',
    'd1 verify',
    'PRAGMA foreign_key_check',
    'PRAGMA integrity_check',
    "automatic_restore_executed': False",
    "secret_material_recorded': False",
    'actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a',
    '--experimental-provision=false',
    '--experimental-auto-create=false',
    'env -u CLOUDFLARE_API_TOKEN -u CLOUDFLARE_ACCOUNT_ID cargo run',
  ];
  for (const marker of requiredMarkers) {
    if (!text.includes(marker)) fail(`protected D1 executor lost required contract marker: ${marker}`);
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
    fail('preflight authorization job must not receive the provider API token');
  }
  for (const marker of [
    'test "$TARGET_ENVIRONMENT" = "staging" || test "$TARGET_ENVIRONMENT" = "production"',
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
  if (!migrateHeader.includes('environment: ${{ inputs.environment }}')) {
    fail('protected mutation job lost exact environment binding');
  }
  if (!migrateHeader.includes('    env:\n')) {
    fail('protected D1 executor job-level metadata environment is missing');
  }
  if (migrateHeader.includes('CLOUDFLARE_API_TOKEN')) {
    fail('Cloudflare API token must not exist in the protected mutation job-level environment');
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

  const tokenSteps = (text.match(/CLOUDFLARE_API_TOKEN: \$\{\{ secrets\.CLOUDFLARE_API_TOKEN \}\}/g) ?? []).length;
  if (tokenSteps < 5) fail('provider credential must be scoped to explicit Wrangler steps, not omitted or broadened');

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
    'second remote apply',
    `${text}\n# npx --yes ${PINNED_WRANGLER} d1 migrations apply X --remote --experimental-provision=false --experimental-auto-create=false\n`,
  );
  console.log('Protected D1 executor negative fixtures rejected as expected.');
}

async function main() {
  const text = await readFile(path.join(ROOT, EXECUTOR), 'utf8');
  if (process.argv.includes('--self-test')) {
    await selfTest(text);
    return;
  }
  if (process.argv.length > 2) fail(`unknown arguments: ${process.argv.slice(2).join(' ')}`);
  await validateExecutor(text, ROOT);
  console.log('Protected D1 executor contract passed: preflight authorization before protected Environment, one remote mutation authority, pinned Wrangler only, credential-free opsctl, no auto-provision/auto-create and no automatic restore.');
}

main().catch((error) => {
  console.error(`D1 executor gate error: ${error instanceof Error ? error.message : error}`);
  process.exitCode = 1;
});
