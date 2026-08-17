import { EXPECTED, cfRequest, invariant, secret } from './ar8c-provider-bootstrap-common.mjs';
import { acceptedPrerequisites, discoverIdentity } from './ar8c-provider-bootstrap-provider.mjs';

const RECOVERY_SQL = Object.freeze({
  resolverState: `
    SELECT
      (SELECT COUNT(*) FROM resolver_encrypted_records) AS encrypted_records,
      (SELECT COUNT(*) FROM resolver_idempotency_records) AS idempotency_records,
      (SELECT COUNT(*) FROM resolver_key_rotation_runs) AS key_rotation_runs,
      (SELECT COUNT(*) FROM resolver_request_nonces) AS request_nonces
  `,
  catalogState: `
    SELECT
      (SELECT COUNT(*) FROM client_contact_points) AS protected_contact_points,
      (SELECT COUNT(*) FROM mailbox_bindings) AS mailbox_bindings,
      (SELECT COUNT(*) FROM mailbox_binding_create_commands) AS mailbox_binding_create_commands,
      (SELECT COUNT(*) FROM mailbox_onboarding_state WHERE credential_handle IS NOT NULL) AS onboarding_state_credential_handles,
      (SELECT COUNT(*) FROM mailbox_onboarding_history WHERE previous_credential_handle IS NOT NULL OR next_credential_handle IS NOT NULL) AS onboarding_history_credential_handles,
      (SELECT COUNT(*) FROM mailbox_onboarding_commands WHERE previous_credential_handle IS NOT NULL OR next_credential_handle IS NOT NULL) AS onboarding_command_credential_handles
  `,
});

const RESOLVER_PRESERVATION_FIELDS = Object.freeze([
  'encrypted_records',
  'idempotency_records',
  'key_rotation_runs',
]);

const CATALOG_RESOLVER_REFERENCE_FIELDS = Object.freeze([
  'mailbox_bindings',
  'mailbox_binding_create_commands',
  'onboarding_state_credential_handles',
  'onboarding_history_credential_handles',
  'onboarding_command_credential_handles',
]);

function isReadOnlyAggregateSql(sql) {
  const normalized = String(sql).trim();
  if (!/^SELECT\b/i.test(normalized)) return false;
  if (/\b(INSERT|UPDATE|DELETE|ALTER|CREATE|DROP|REPLACE|VACUUM|ATTACH|DETACH|REINDEX|PRAGMA)\b/i.test(normalized)) return false;
  if (/\bSELECT\s+\*/i.test(normalized)) return false;
  return /COUNT\s*\(\s*\*\s*\)/i.test(normalized);
}

function countValue(row, field) {
  const value = Number(row?.[field]);
  invariant(Number.isSafeInteger(value) && value >= 0, `recovery assessment count ${field} is invalid`);
  return value;
}

function normalizeCounts(row, fields) {
  return Object.fromEntries(fields.map((field) => [field, countValue(row, field)]));
}

async function queryAggregateRow(token, accountId, databaseId, sql) {
  invariant(isReadOnlyAggregateSql(sql), 'credential recovery assessment refused non-aggregate or mutating D1 SQL');
  const payload = await cfRequest(
    token,
    `/accounts/${accountId}/d1/database/${databaseId}/query`,
    { method: 'POST', body: { sql } },
  );
  const statements = Array.isArray(payload?.result) ? payload.result : [];
  invariant(statements.length === 1 && statements[0]?.success !== false, 'credential recovery D1 aggregate query did not return one successful statement');
  const rows = Array.isArray(statements[0]?.results) ? statements[0].results : [];
  invariant(rows.length === 1, 'credential recovery D1 aggregate query did not return exactly one row');
  return rows[0];
}

function classifyRecoveryCounts(resolverRow, catalogRow) {
  const resolver = normalizeCounts(resolverRow, [
    ...RESOLVER_PRESERVATION_FIELDS,
    'request_nonces',
  ]);
  const catalog = normalizeCounts(catalogRow, [
    'protected_contact_points',
    ...CATALOG_RESOLVER_REFERENCE_FIELDS,
  ]);

  const resolverPreservationRows = RESOLVER_PRESERVATION_FIELDS
    .reduce((total, field) => total + resolver[field], 0);
  const catalogResolverReferences = CATALOG_RESOLVER_REFERENCE_FIELDS
    .reduce((total, field) => total + catalog[field], 0);
  const contactPreservationRows = catalog.protected_contact_points;
  const keyPreservationRequired = resolverPreservationRows > 0
    || catalogResolverReferences > 0
    || contactPreservationRows > 0;

  return {
    schema_version: 1,
    environment: EXPECTED.environment,
    classification: keyPreservationRequired
      ? 'KEY_PRESERVATION_REQUIRED'
      : 'FRESH_PROJECT_SECRET_ISSUANCE_SAFE',
    external_oauth_secret_authority: 'PROVIDER_OWNED_INPUT_REQUIRED',
    resolver_key_material_dependency_count: resolverPreservationRows,
    catalog_resolver_reference_count: catalogResolverReferences,
    contact_key_material_dependency_count: contactPreservationRows,
    resolver,
    catalog,
  };
}

async function assess() {
  invariant(process.env.GITHUB_REPOSITORY === EXPECTED.repository, `must run in ${EXPECTED.repository}`);
  invariant(process.env.GITHUB_REF_NAME === EXPECTED.refName, 'credential recovery assessment must run from main');
  invariant(process.env.GITHUB_EVENT_NAME === EXPECTED.eventName, 'credential recovery assessment must be workflow_dispatch only');
  invariant(process.env.AR8C_TARGET_ENVIRONMENT === EXPECTED.environment, 'AR8C_TARGET_ENVIRONMENT must be staging');

  const token = secret('CLOUDFLARE_BOOTSTRAP_TOKEN');
  const { accountId } = await discoverIdentity(token);
  const prerequisites = await acceptedPrerequisites(token, accountId);

  const [resolverRow, catalogRow] = await Promise.all([
    queryAggregateRow(token, accountId, prerequisites.resolverId, RECOVERY_SQL.resolverState),
    queryAggregateRow(token, accountId, prerequisites.catalogId, RECOVERY_SQL.catalogState),
  ]);
  const evidence = classifyRecoveryCounts(resolverRow, catalogRow);
  console.log(`AR8C_CREDENTIAL_RECOVERY_ASSESSMENT_JSON=${JSON.stringify(evidence)}`);
  console.log(`AR-8C credential recovery assessment: PASS (${evidence.classification})`);
}

function selfTest() {
  invariant(Object.values(RECOVERY_SQL).every(isReadOnlyAggregateSql), 'one or more recovery assessment statements are not aggregate read-only SELECT statements');
  invariant(!isReadOnlyAggregateSql('DELETE FROM resolver_encrypted_records'), 'DELETE fixture must fail closed');
  invariant(!isReadOnlyAggregateSql('SELECT * FROM resolver_encrypted_records'), 'row-content SELECT fixture must fail closed');
  invariant(!isReadOnlyAggregateSql('PRAGMA table_info(resolver_encrypted_records)'), 'PRAGMA fixture must fail closed');

  const safe = classifyRecoveryCounts(
    { encrypted_records: 0, idempotency_records: 0, key_rotation_runs: 0, request_nonces: 2 },
    {
      protected_contact_points: 0,
      mailbox_bindings: 0,
      mailbox_binding_create_commands: 0,
      onboarding_state_credential_handles: 0,
      onboarding_history_credential_handles: 0,
      onboarding_command_credential_handles: 0,
    },
  );
  invariant(safe.classification === 'FRESH_PROJECT_SECRET_ISSUANCE_SAFE', 'zero durable key dependencies must permit fresh project-secret issuance');
  invariant(safe.external_oauth_secret_authority === 'PROVIDER_OWNED_INPUT_REQUIRED', 'OAuth client secrets must remain provider-owned external inputs');

  const resolverBlocked = classifyRecoveryCounts(
    { encrypted_records: 1, idempotency_records: 0, key_rotation_runs: 0, request_nonces: 0 },
    {
      protected_contact_points: 0,
      mailbox_bindings: 0,
      mailbox_binding_create_commands: 0,
      onboarding_state_credential_handles: 0,
      onboarding_history_credential_handles: 0,
      onboarding_command_credential_handles: 0,
    },
  );
  invariant(resolverBlocked.classification === 'KEY_PRESERVATION_REQUIRED', 'resolver ciphertext must require key preservation');

  const catalogBlocked = classifyRecoveryCounts(
    { encrypted_records: 0, idempotency_records: 0, key_rotation_runs: 0, request_nonces: 0 },
    {
      protected_contact_points: 0,
      mailbox_bindings: 0,
      mailbox_binding_create_commands: 0,
      onboarding_state_credential_handles: 1,
      onboarding_history_credential_handles: 0,
      onboarding_command_credential_handles: 0,
    },
  );
  invariant(catalogBlocked.classification === 'KEY_PRESERVATION_REQUIRED', 'catalog resolver-handle references must require key preservation');

  const contactBlocked = classifyRecoveryCounts(
    { encrypted_records: 0, idempotency_records: 0, key_rotation_runs: 0, request_nonces: 0 },
    {
      protected_contact_points: 1,
      mailbox_bindings: 0,
      mailbox_binding_create_commands: 0,
      onboarding_state_credential_handles: 0,
      onboarding_history_credential_handles: 0,
      onboarding_command_credential_handles: 0,
    },
  );
  invariant(contactBlocked.classification === 'KEY_PRESERVATION_REQUIRED', 'protected contact ciphertext/tokens must require key preservation');
  console.log('AR-8C credential recovery assessment self-test: PASS');
}

const command = process.argv[2] ?? 'recovery-assess';
if (command === 'self-test') {
  selfTest();
} else if (command === 'recovery-assess') {
  await assess();
} else {
  throw new Error(`Unknown command: ${command}`);
}
