import { readdir } from 'node:fs/promises';

const EXPECTED = Object.freeze({
  repository: 'iamaman11/part-crm-emai-profile',
  refName: 'main',
  eventName: 'workflow_dispatch',
  environment: 'staging',
  accountName: "Pvisakp@gmail.com's Account",
  zoneName: 'alegria.by',
  productionCatalogName: 'part-crm-catalog-production',
  resolverPrefix: 'mailbox-secret-resolver-gate-b-bootstrap-',
  catalogNames: Object.freeze(['part-crm-catalog-staging']),
  catalogPrefixes: Object.freeze(['part-crm-catalog-staging-d3-']),
  proofNames: Object.freeze(['part-crm-d3a-bootstrap-proof']),
});

const CLOUDFLARE_API = 'https://api.cloudflare.com/client/v4';
const SQL = Object.freeze({
  schemaObjects: "SELECT type, name, tbl_name FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
  migrationLedgerPresence: "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'd1_migrations'",
  migrationLedger: 'SELECT id, name FROM d1_migrations ORDER BY id',
  resolverEncryptedRecordColumns: "SELECT name FROM pragma_table_info('resolver_encrypted_records') ORDER BY cid",
});

const REQUIRED_RESOLVER_TABLES = Object.freeze([
  'resolver_encrypted_records',
  'resolver_idempotency_records',
  'resolver_key_rotation_runs',
  'resolver_request_nonces',
]);

const REQUIRED_RESOLVER_COLUMNS = Object.freeze([
  'tenant_id',
  'lookup_digest',
  'provider',
  'record_kind',
  'logical_id',
  'key_version',
  'nonce_hex',
  'ciphertext_hex',
  'created_at_ms',
  'updated_at_ms',
  'expires_at_ms',
  'consumed_at_ms',
  'discarded_at_ms',
  'mutation_generation',
  'credential_state',
  'refresh_owner_digest',
  'refresh_started_at_ms',
  'refresh_expires_at_ms',
]);

function invariant(condition, message) {
  if (!condition) throw new Error(message);
}

function secret(name) {
  const value = process.env[name];
  invariant(typeof value === 'string' && value.trim().length >= 20, `${name} is missing or unusable in the protected staging environment`);
  return value.trim();
}

function safeText(value) {
  if (typeof value !== 'string' || value.length === 0) return null;
  return value.replace(/[\r\n\t]/g, ' ').slice(0, 300);
}

function sanitizeApiErrors(payload) {
  const errors = Array.isArray(payload?.errors) ? payload.errors : [];
  return errors.slice(0, 5).map((entry) => ({
    code: entry?.code ?? null,
    message: safeText(entry?.message) ?? 'unknown error',
  }));
}

async function cloudflareRequest(token, path, { method = 'GET', body = null } = {}) {
  const headers = {
    Authorization: `Bearer ${token}`,
    Accept: 'application/json',
  };
  if (body !== null) headers['Content-Type'] = 'application/json';

  const response = await fetch(`${CLOUDFLARE_API}${path}`, {
    method,
    headers,
    body: body === null ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(20_000),
  });

  let payload;
  try {
    payload = await response.json();
  } catch {
    throw new Error(`Cloudflare ${path} returned non-JSON HTTP ${response.status}`);
  }

  if (!response.ok || payload?.success !== true) {
    throw new Error(`Cloudflare ${path} failed with HTTP ${response.status}: ${JSON.stringify(sanitizeApiErrors(payload))}`);
  }
  return payload;
}

async function discoverAccountId(token) {
  const params = new URLSearchParams({ name: EXPECTED.zoneName, per_page: '50' });
  const payload = await cloudflareRequest(token, `/zones?${params.toString()}`);
  const zones = Array.isArray(payload?.result) ? payload.result : [];
  const matches = zones.filter((zone) => zone?.name === EXPECTED.zoneName && zone?.account?.name === EXPECTED.accountName);
  invariant(matches.length === 1, `Expected exactly one ${EXPECTED.zoneName} zone in ${EXPECTED.accountName}; got ${matches.length}`);
  invariant(/^[0-9a-f]{32}$/.test(matches[0]?.account?.id ?? ''), 'Discovered Cloudflare account id has an invalid shape');
  return matches[0].account.id;
}

function queryRows(payload) {
  const statements = Array.isArray(payload?.result) ? payload.result : [];
  const rows = [];
  for (const statement of statements) {
    invariant(statement?.success !== false, 'Cloudflare D1 query statement reported failure');
    if (Array.isArray(statement?.results)) rows.push(...statement.results);
  }
  return rows;
}

async function queryD1(token, accountId, databaseId, sql) {
  invariant(isReadOnlySql(sql), `refusing non-read-only D1 SQL: ${sql}`);
  const payload = await cloudflareRequest(
    token,
    `/accounts/${accountId}/d1/database/${databaseId}/query`,
    { method: 'POST', body: { sql } },
  );
  return queryRows(payload);
}

function isReadOnlySql(sql) {
  const normalized = String(sql).trim();
  if (!/^SELECT\b/i.test(normalized)) return false;
  return !/\b(INSERT|UPDATE|DELETE|ALTER|CREATE|DROP|REPLACE|VACUUM|ATTACH|DETACH|REINDEX)\b/i.test(normalized);
}

function isProductionName(name) {
  return name === EXPECTED.productionCatalogName || /(^|[-_.])production($|[-_.])/i.test(name);
}

function roleForDatabase(name) {
  if (name.startsWith(EXPECTED.resolverPrefix)) return 'resolver_candidate';
  if (EXPECTED.catalogNames.includes(name) || EXPECTED.catalogPrefixes.some((prefix) => name.startsWith(prefix))) return 'catalog_candidate';
  if (EXPECTED.proofNames.includes(name)) return 'proof_only';
  return null;
}

function normalizedSchemaObjects(rows) {
  return rows
    .map((row) => ({
      type: safeText(row?.type),
      name: safeText(row?.name),
      tbl_name: safeText(row?.tbl_name),
    }))
    .filter((row) => row.type && row.name)
    .sort((a, b) => `${a.type}\u0000${a.name}`.localeCompare(`${b.type}\u0000${b.name}`, 'en'));
}

function classifyResolver(schemaObjects, columnRows) {
  const tableNames = new Set(schemaObjects.filter((item) => item.type === 'table').map((item) => item.name));
  const columns = new Set(columnRows.map((row) => safeText(row?.name)).filter(Boolean));
  const missingTables = REQUIRED_RESOLVER_TABLES.filter((name) => !tableNames.has(name));
  const missingColumns = REQUIRED_RESOLVER_COLUMNS.filter((name) => !columns.has(name));
  return {
    bootstrap_classification: missingTables.length === 0 && missingColumns.length === 0
      ? 'PRESENT_AND_MATCHING'
      : 'PRESENT_BUT_CONFLICTING',
    schema_state: missingTables.length === 0 && missingColumns.length === 0
      ? 'CURRENT_RESOLVER_SCHEMA_MATCH'
      : 'RESOLVER_SCHEMA_MISMATCH',
    missing_required_tables: missingTables,
    missing_resolver_encrypted_record_columns: missingColumns,
  };
}

function classifyCatalog(remoteMigrationNames, expectedMigrationNames, hasLedger) {
  if (!hasLedger) {
    return {
      bootstrap_classification: 'PRESENT_BUT_CONFLICTING',
      migration_state: 'NO_D1_MIGRATION_LEDGER',
    };
  }

  const exact = remoteMigrationNames.length === expectedMigrationNames.length
    && remoteMigrationNames.every((name, index) => name === expectedMigrationNames[index]);
  if (exact) {
    return {
      bootstrap_classification: 'PRESENT_AND_MATCHING',
      migration_state: 'CURRENT_CATALOG_MIGRATION_MATCH',
    };
  }

  const prefix = remoteMigrationNames.length < expectedMigrationNames.length
    && remoteMigrationNames.every((name, index) => name === expectedMigrationNames[index]);
  if (prefix) {
    return {
      bootstrap_classification: 'PRESENT_BUT_CONFLICTING',
      migration_state: 'CATALOG_BEHIND_CURRENT',
    };
  }

  return {
    bootstrap_classification: 'PRESENT_BUT_CONFLICTING',
    migration_state: 'CATALOG_MIGRATION_CONFLICT',
  };
}

async function expectedCatalogMigrations() {
  const names = (await readdir('migrations/d1'))
    .filter((name) => /^\d{4}_[a-z0-9_]+\.sql$/.test(name))
    .sort((a, b) => a.localeCompare(b, 'en'));
  invariant(names.length > 0, 'current catalog migration inventory is empty');
  for (let index = 0; index < names.length; index += 1) {
    const expectedPrefix = String(index + 1).padStart(4, '0');
    invariant(names[index].startsWith(`${expectedPrefix}_`), `catalog migration inventory is not contiguous at ${names[index]}`);
  }
  return names;
}

async function classifyRemoteD1(token) {
  const accountId = await discoverAccountId(token);
  const databasesPayload = await cloudflareRequest(token, `/accounts/${accountId}/d1/database?per_page=10000`);
  const databases = Array.isArray(databasesPayload?.result) ? databasesPayload.result : [];
  const expectedMigrations = await expectedCatalogMigrations();

  const candidates = databases
    .map((database) => ({
      name: safeText(database?.name),
      id: safeText(database?.uuid),
    }))
    .filter((database) => database.name && database.id && roleForDatabase(database.name))
    .sort((a, b) => a.name.localeCompare(b.name, 'en'));

  invariant(candidates.length > 0, 'no recognized staging/proof D1 candidates were discovered');
  invariant(candidates.every((database) => !isProductionName(database.name)), 'production D1 entered the read-only staging candidate set');

  const evidence = [];
  for (const database of candidates) {
    invariant(!isProductionName(database.name), `production D1 read is forbidden: ${database.name}`);
    const role = roleForDatabase(database.name);
    const schemaRows = await queryD1(token, accountId, database.id, SQL.schemaObjects);
    const schemaObjects = normalizedSchemaObjects(schemaRows);

    if (role === 'resolver_candidate') {
      const columns = await queryD1(token, accountId, database.id, SQL.resolverEncryptedRecordColumns);
      evidence.push({
        name: database.name,
        role,
        ...classifyResolver(schemaObjects, columns),
        schema_objects: schemaObjects,
      });
      continue;
    }

    if (role === 'catalog_candidate') {
      const ledgerPresence = await queryD1(token, accountId, database.id, SQL.migrationLedgerPresence);
      const hasLedger = ledgerPresence.some((row) => row?.name === 'd1_migrations');
      const ledgerRows = hasLedger ? await queryD1(token, accountId, database.id, SQL.migrationLedger) : [];
      const remoteMigrationNames = ledgerRows.map((row) => safeText(row?.name)).filter(Boolean);
      evidence.push({
        name: database.name,
        role,
        ...classifyCatalog(remoteMigrationNames, expectedMigrations, hasLedger),
        remote_migration_count: remoteMigrationNames.length,
        expected_migration_count: expectedMigrations.length,
        remote_migrations: remoteMigrationNames,
        schema_object_count: schemaObjects.length,
      });
      continue;
    }

    evidence.push({
      name: database.name,
      role: 'proof_only',
      runtime_eligibility: 'FORBIDDEN_AS_RUNTIME_TARGET',
      schema_object_count: schemaObjects.length,
    });
  }

  invariant(!JSON.stringify(evidence).toLowerCase().includes('part-crm-catalog-production'), 'production D1 leaked into classification evidence');
  return {
    schema_version: 1,
    environment: EXPECTED.environment,
    expected_catalog_migration_count: expectedMigrations.length,
    databases: evidence,
  };
}

function selfTest() {
  invariant(Object.values(SQL).every(isReadOnlySql), 'one or more D1 classification statements are not read-only SELECT statements');
  invariant(isProductionName('part-crm-catalog-production'), 'exact production D1 must be rejected');
  invariant(isProductionName('something-production-shadow'), 'production-shaped D1 must be rejected');
  invariant(!isProductionName('part-crm-catalog-staging'), 'staging D1 must not be rejected as production');
  invariant(roleForDatabase('mailbox-secret-resolver-gate-b-bootstrap-20260815-033400') === 'resolver_candidate', 'resolver candidate classification drifted');
  invariant(roleForDatabase('part-crm-catalog-staging-d3-20260815') === 'catalog_candidate', 'catalog D3 candidate classification drifted');
  invariant(roleForDatabase('part-crm-d3a-bootstrap-proof') === 'proof_only', 'proof D1 classification drifted');

  const expected = ['0001_a.sql', '0002_b.sql', '0003_c.sql'];
  invariant(classifyCatalog(expected, expected, true).migration_state === 'CURRENT_CATALOG_MIGRATION_MATCH', 'exact catalog match fixture failed');
  invariant(classifyCatalog(expected.slice(0, 2), expected, true).migration_state === 'CATALOG_BEHIND_CURRENT', 'behind catalog fixture failed');
  invariant(classifyCatalog(['0001_a.sql', '0003_c.sql'], expected, true).migration_state === 'CATALOG_MIGRATION_CONFLICT', 'catalog conflict fixture failed');
  invariant(classifyCatalog([], expected, false).migration_state === 'NO_D1_MIGRATION_LEDGER', 'missing ledger fixture failed');

  const resolverSchema = REQUIRED_RESOLVER_TABLES.map((name) => ({ type: 'table', name, tbl_name: name }));
  const resolverColumns = REQUIRED_RESOLVER_COLUMNS.map((name) => ({ name }));
  invariant(classifyResolver(resolverSchema, resolverColumns).bootstrap_classification === 'PRESENT_AND_MATCHING', 'resolver current fixture failed');
  invariant(classifyResolver(resolverSchema.slice(1), resolverColumns).bootstrap_classification === 'PRESENT_BUT_CONFLICTING', 'resolver missing-table fixture failed');
  console.log('AR-8C staging D1 read-only classifier self-test: PASS');
}

async function classify() {
  invariant(process.env.GITHUB_REPOSITORY === EXPECTED.repository, `must run in ${EXPECTED.repository}`);
  invariant(process.env.GITHUB_REF_NAME === EXPECTED.refName, 'D1 classification must run from main');
  invariant(process.env.GITHUB_EVENT_NAME === EXPECTED.eventName, 'D1 classification must be workflow_dispatch only');
  invariant(process.env.AR8C_TARGET_ENVIRONMENT === EXPECTED.environment, 'AR8C_TARGET_ENVIRONMENT must be staging');
  const token = secret('CLOUDFLARE_BOOTSTRAP_TOKEN');
  const evidence = await classifyRemoteD1(token);
  console.log(`AR8C_D1_CLASSIFICATION_JSON=${JSON.stringify(evidence)}`);
  console.log('AR-8C staging D1 classification: PASS (read-only SELECT statements only; production D1 excluded)');
}

const command = process.argv[2] ?? 'classify';
if (command === 'self-test') {
  selfTest();
} else if (command === 'classify') {
  await classify();
} else {
  throw new Error(`Unknown command: ${command}`);
}
