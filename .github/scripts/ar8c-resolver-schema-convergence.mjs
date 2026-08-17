import { appendFile, readFile, readdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

const CF_API = 'https://api.cloudflare.com/client/v4';
const AUTHORITY_PATH = 'architecture/ar8-staging-provider-bootstrap-contract.json';
const MIGRATION_DIR = 'migrations/resolver-d1';
const TARGET = Object.freeze({
  repository: 'iamaman11/part-crm-emai-profile',
  ref: 'main',
  event: 'workflow_dispatch',
  environment: 'staging',
  accountName: "Pvisakp@gmail.com's Account",
  zoneName: 'alegria.by',
  databaseName: 'mailbox-secret-resolver-gate-b-bootstrap-20260815-033400',
  expectedOnlyMissingMigration: '0002_oauth_refresh_fencing.sql',
});
const REQUIRED_BASE_TABLES = Object.freeze([
  'resolver_encrypted_records',
  'resolver_idempotency_records',
  'resolver_key_rotation_runs',
  'resolver_request_nonces',
]);
const REQUIRED_FENCING_COLUMNS = Object.freeze([
  'mutation_generation',
  'credential_state',
  'refresh_owner_digest',
  'refresh_started_at_ms',
  'refresh_expires_at_ms',
]);
const REQUIRED_FENCING_INDEXES = Object.freeze([
  'resolver_encrypted_records_credential_state',
  'resolver_encrypted_records_refresh_lease',
]);
const FORBIDDEN_SQL = /\b(DROP|DELETE|UPDATE|INSERT|REPLACE|VACUUM|ATTACH|DETACH)\b/i;

function invariant(condition, message) {
  if (!condition) throw new Error(message);
}

function secret(name) {
  const value = process.env[name];
  invariant(typeof value === 'string' && value.trim().length >= 20, `${name} is missing from protected staging`);
  return value.trim();
}

async function cf(token, apiPath, { method = 'GET', body = null } = {}) {
  const headers = { Authorization: `Bearer ${token}`, Accept: 'application/json' };
  if (body !== null) headers['Content-Type'] = 'application/json';
  const response = await fetch(`${CF_API}${apiPath}`, {
    method,
    headers,
    body: body === null ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(30_000),
  });
  let payload;
  try {
    payload = await response.json();
  } catch {
    throw new Error(`Cloudflare ${apiPath} returned non-JSON HTTP ${response.status}`);
  }
  if (!response.ok || payload?.success !== true) {
    const errors = Array.isArray(payload?.errors)
      ? payload.errors.slice(0, 5).map((entry) => ({ code: entry?.code ?? null, message: String(entry?.message ?? 'unknown').slice(0, 300) }))
      : [];
    throw new Error(`Cloudflare ${apiPath} failed with HTTP ${response.status}: ${JSON.stringify(errors)}`);
  }
  return payload;
}

async function discover(token) {
  const zones = await cf(token, `/zones?${new URLSearchParams({ name: TARGET.zoneName, per_page: '50' })}`);
  const matches = (Array.isArray(zones?.result) ? zones.result : []).filter(
    (zone) => zone?.name === TARGET.zoneName && zone?.account?.name === TARGET.accountName && zone?.status === 'active',
  );
  invariant(matches.length === 1, `expected exactly one active ${TARGET.zoneName} zone in ${TARGET.accountName}; got ${matches.length}`);
  const accountId = matches[0]?.account?.id;
  invariant(/^[0-9a-f]{32}$/.test(accountId ?? ''), 'Cloudflare account id has invalid shape');

  const dbPayload = await cf(token, `/accounts/${accountId}/d1/database?per_page=10000`);
  const databases = Array.isArray(dbPayload?.result) ? dbPayload.result : [];
  invariant(databases.some((db) => db?.name === 'part-crm-catalog-production'), 'expected production sentinel was not visible; refusing mutation under uncertain account inventory');
  const targetMatches = databases.filter((db) => db?.name === TARGET.databaseName);
  invariant(targetMatches.length === 1, `expected exactly one dedicated resolver D1 named ${TARGET.databaseName}; got ${targetMatches.length}`);
  invariant(targetMatches[0]?.name !== 'part-crm-catalog-production', 'production D1 can never be the resolver convergence target');
  const databaseId = targetMatches[0]?.uuid;
  invariant(/^[0-9a-f-]{36}$/.test(databaseId ?? ''), 'resolver D1 id has invalid shape');
  return { accountId, databaseId };
}

function rows(payload) {
  const statements = Array.isArray(payload?.result) ? payload.result : [];
  const output = [];
  for (const statement of statements) {
    invariant(statement?.success !== false, 'D1 query statement failed');
    if (Array.isArray(statement?.results)) output.push(...statement.results);
  }
  return output;
}

async function query(token, accountId, databaseId, sql) {
  invariant(/^SELECT\b/i.test(sql.trim()), 'convergence discovery queries must be SELECT-only');
  invariant(!FORBIDDEN_SQL.test(sql), 'convergence discovery query contains mutation SQL');
  return rows(await cf(token, `/accounts/${accountId}/d1/database/${databaseId}/query`, { method: 'POST', body: { sql } }));
}

async function localMigrationNames() {
  const names = (await readdir(MIGRATION_DIR))
    .filter((name) => /^\d{4}_[a-z0-9_]+\.sql$/.test(name))
    .sort((a, b) => a.localeCompare(b, 'en'));
  invariant(names.length === 2, `resolver convergence authority currently expects exactly two migrations; got ${names.length}`);
  invariant(names[0] === '0001_resolver_security_boundary.sql', 'resolver migration 0001 drifted');
  invariant(names[1] === TARGET.expectedOnlyMissingMigration, 'resolver migration 0002 drifted');
  return names;
}

function isExactPrefix(remote, local) {
  return remote.length <= local.length && remote.every((name, index) => name === local[index]);
}

function stripSqlComments(source) {
  return source.replace(/^\s*--.*$/gm, '').trim();
}

async function validateMigrationSource() {
  const migrationPath = path.join(MIGRATION_DIR, TARGET.expectedOnlyMissingMigration);
  const source = await readFile(migrationPath, 'utf8');
  invariant(!FORBIDDEN_SQL.test(source), 'AR-8A resolver migration contains a forbidden destructive/data-mutation SQL token');
  const normalized = stripSqlComments(source);
  invariant(/^PRAGMA\s+foreign_keys\s*=\s*ON\s*;/i.test(normalized), 'resolver migration must begin with PRAGMA foreign_keys = ON');
  const alterStatements = normalized.match(/ALTER TABLE\s+resolver_encrypted_records[\s\S]*?;/gi) ?? [];
  const indexStatements = normalized.match(/CREATE INDEX\s+[a-z0-9_]+[\s\S]*?;/gi) ?? [];
  invariant(alterStatements.length === 5, `resolver migration must contain exactly five ALTER TABLE statements; got ${alterStatements.length}`);
  invariant(alterStatements.every((statement) => /\bADD COLUMN\b/i.test(statement)), 'resolver migration ALTER TABLE statements must be ADD COLUMN only');
  invariant(indexStatements.length === 2, `resolver migration must contain exactly two CREATE INDEX statements; got ${indexStatements.length}`);
  for (const column of REQUIRED_FENCING_COLUMNS) {
    invariant(new RegExp(`\\bADD COLUMN\\s+${column}\\b`, 'i').test(normalized), `resolver migration is missing approved column ${column}`);
  }
  for (const index of REQUIRED_FENCING_INDEXES) {
    invariant(new RegExp(`\\bCREATE INDEX\\s+${index}\\b`, 'i').test(normalized), `resolver migration is missing approved index ${index}`);
  }
}

async function inspectState(token, accountId, databaseId) {
  const [ledgerRows, tableRows, columnRows, indexRows] = await Promise.all([
    query(token, accountId, databaseId, 'SELECT id, name FROM d1_migrations ORDER BY id'),
    query(token, accountId, databaseId, "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name"),
    query(token, accountId, databaseId, "SELECT name FROM pragma_table_info('resolver_encrypted_records') ORDER BY cid"),
    query(token, accountId, databaseId, "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'resolver_encrypted_records' ORDER BY name"),
  ]);
  return {
    ledger: ledgerRows.map((row) => String(row?.name ?? '')),
    tables: new Set(tableRows.map((row) => String(row?.name ?? ''))),
    columns: new Set(columnRows.map((row) => String(row?.name ?? ''))),
    indexes: new Set(indexRows.map((row) => String(row?.name ?? ''))),
  };
}

async function validateAuthority() {
  const authority = JSON.parse(await readFile(AUTHORITY_PATH, 'utf8'));
  invariant(authority?.status === 'AR8C_STAGING_PROVIDER_EXECUTION_AUTHORITY', 'accepted AR-8C provider execution authority is missing');
  invariant(authority?.environment === 'staging', 'provider execution authority is not staging-only');
  invariant(authority?.invariants?.production_read_or_mutation === 'FORBIDDEN', 'production mutation authority drifted');
  invariant(authority?.invariants?.terraform === 'FORBIDDEN', 'Terraform prohibition drifted');
  invariant(authority?.accepted_staging_resource_roles?.resolver_d1?.accepted_name === TARGET.databaseName, 'resolver D1 authority target drifted');
  invariant(authority?.resolver_schema_convergence?.current_expected_missing_migration === TARGET.expectedOnlyMissingMigration, 'resolver convergence migration authority drifted');
}

async function writeWranglerConfig(accountId, databaseId) {
  const runnerTemp = process.env.RUNNER_TEMP;
  const githubEnv = process.env.GITHUB_ENV;
  const workspace = process.env.GITHUB_WORKSPACE;
  invariant(runnerTemp && githubEnv && workspace, 'GitHub runner paths are unavailable');
  const configPath = path.join(runnerTemp, 'ar8c-resolver-convergence.wrangler.json');
  const config = {
    $schema: 'https://raw.githubusercontent.com/cloudflare/workers-sdk/main/packages/wrangler/config-schema.json',
    name: 'ar8c-resolver-schema-convergence-staging',
    account_id: accountId,
    compatibility_date: '2026-08-15',
    d1_databases: [{
      binding: 'MAILBOX_SECRET_RESOLVER_DB',
      database_name: TARGET.databaseName,
      database_id: databaseId,
      migrations_table: 'd1_migrations',
      migrations_dir: path.join(workspace, MIGRATION_DIR),
    }],
  };
  await writeFile(configPath, `${JSON.stringify(config, null, 2)}\n`, { mode: 0o600 });
  await appendFile(githubEnv, `AR8C_WRANGLER_CONFIG=${configPath}\nAR8C_RESOLVER_DB_NAME=${TARGET.databaseName}\n`, 'utf8');
}

async function prepare() {
  invariant(process.env.GITHUB_REPOSITORY === TARGET.repository, `must run in ${TARGET.repository}`);
  invariant(process.env.GITHUB_REF_NAME === TARGET.ref, 'resolver convergence must run from accepted main');
  invariant(process.env.GITHUB_EVENT_NAME === TARGET.event, 'resolver convergence must be workflow_dispatch only');
  invariant(process.env.AR8C_TARGET_ENVIRONMENT === TARGET.environment, 'resolver convergence target environment must be staging');
  await validateAuthority();
  await validateMigrationSource();
  const token = secret('CLOUDFLARE_BOOTSTRAP_TOKEN');
  const { accountId, databaseId } = await discover(token);
  const local = await localMigrationNames();
  const state = await inspectState(token, accountId, databaseId);
  invariant(REQUIRED_BASE_TABLES.every((name) => state.tables.has(name)), 'dedicated resolver D1 base tables do not match accepted authority');
  invariant(isExactPrefix(state.ledger, local), `resolver d1_migrations is not an exact ordered prefix: ${JSON.stringify(state.ledger)}`);
  const missing = local.slice(state.ledger.length);
  invariant(missing.length === 0 || (missing.length === 1 && missing[0] === TARGET.expectedOnlyMissingMigration), `resolver missing suffix is outside accepted authority: ${JSON.stringify(missing)}`);

  const action = missing.length === 0 ? 'NOOP' : 'APPLY';
  if (action === 'APPLY') {
    invariant(REQUIRED_FENCING_COLUMNS.every((name) => !state.columns.has(name)), 'AR-8A fencing columns are partially present; refusing non-atomic convergence');
  }
  await writeWranglerConfig(accountId, databaseId);
  await appendFile(process.env.GITHUB_ENV, `AR8C_RESOLVER_ACTION=${action}\n`, 'utf8');
  console.log(`AR-8C resolver schema convergence preflight: PASS action=${action} remote_migrations=${state.ledger.length} local_migrations=${local.length}`);
}

async function verify() {
  await validateAuthority();
  const token = secret('CLOUDFLARE_BOOTSTRAP_TOKEN');
  const { accountId, databaseId } = await discover(token);
  const local = await localMigrationNames();
  const state = await inspectState(token, accountId, databaseId);
  invariant(JSON.stringify(state.ledger) === JSON.stringify(local), `resolver migration ledger did not converge: ${JSON.stringify(state.ledger)}`);
  invariant(REQUIRED_BASE_TABLES.every((name) => state.tables.has(name)), 'resolver base tables disappeared after convergence');
  invariant(REQUIRED_FENCING_COLUMNS.every((name) => state.columns.has(name)), 'one or more AR-8A fencing columns are missing after convergence');
  invariant(REQUIRED_FENCING_INDEXES.every((name) => state.indexes.has(name)), 'one or more AR-8A fencing indexes are missing after convergence');
  console.log('AR-8C resolver schema convergence: PASS (current resolver migration ledger and AR-8A schema verified)');
}

async function selfTest() {
  invariant(isExactPrefix(['0001'], ['0001', '0002']), 'prefix fixture failed');
  invariant(isExactPrefix(['0001', '0002'], ['0001', '0002']), 'exact fixture failed');
  invariant(!isExactPrefix(['0002'], ['0001', '0002']), 'non-prefix fixture was accepted');
  invariant(!isExactPrefix(['0001', '0003'], ['0001', '0002']), 'divergent fixture was accepted');
  invariant(FORBIDDEN_SQL.test('DROP TABLE x'), 'DROP fixture was not detected');
  invariant(FORBIDDEN_SQL.test('UPDATE resolver_encrypted_records SET x = 1'), 'UPDATE fixture was not detected');
  invariant(!FORBIDDEN_SQL.test('ALTER TABLE resolver_encrypted_records ADD COLUMN x TEXT; CREATE INDEX y ON resolver_encrypted_records(x);'), 'additive fixture was rejected');
  await validateMigrationSource();
  console.log('AR-8C resolver schema convergence self-test: PASS');
}

const command = process.argv[2] ?? 'self-test';
if (command === 'self-test') await selfTest();
else if (command === 'prepare') await prepare();
else if (command === 'verify') await verify();
else throw new Error(`Unknown command: ${command}`);
