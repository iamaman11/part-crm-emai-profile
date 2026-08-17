import { readFile } from 'node:fs/promises';

const AUTHORITY_PATH = 'architecture/ar8-staging-provider-bootstrap-contract.json';
const CREDENTIAL_AUTHORITY_PATH = 'architecture/credential-authority-ar8b.json';
const HISTORICAL_AUTHORITY_PATH = 'architecture/pre2j-d3-resolver-bootstrap-authority.json';
const RESOLVER_MIGRATION_PATH = 'migrations/resolver-d1/0002_oauth_refresh_fencing.sql';

const EXACT_OUTPUTS = Object.freeze([
  'CLOUDFLARE_ACCESS_CLIENT_ID',
  'CLOUDFLARE_ACCESS_CLIENT_SECRET',
  'CLOUDFLARE_API_TOKEN',
  'CLOUDFLARE_DEPLOY_MANIFEST_JSON',
]);
const EXACT_TRANSITIONAL_BUNDLES = Object.freeze([
  'CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON',
  'CLOUDFLARE_RESOLVER_SECRETS_JSON',
]);
const EXACT_CLASSIFICATIONS = Object.freeze([
  'PRESENT_AND_MATCHING',
  'MISSING',
  'PRESENT_BUT_CONFLICTING',
  'UNKNOWN_DUE_TO_AUTHORIZATION',
]);
const EXACT_ORDER = Object.freeze([
  'accepted_resolver_release_artifact',
  'resolver_d1_schema_convergence_if_exact_prefix',
  'catalog_d1_reuse_only_if_present_and_matching',
  'profile_r2_and_queues_reuse_or_create_only_if_missing',
  'r2_s3_credentials',
  'resolver_worker_with_direct_secret_store_bindings',
  'access_application_service_auth_policy_and_token',
  'steady_state_cloudflare_api_token',
  'github_staging_environment_outputs',
  'control_plane_first_deploy_with_direct_secret_store_bindings_and_custom_domain',
  'staging_smoke_and_attestation',
  'bootstrap_credential_revocation',
  'ar8c_read_only_reconciliation',
]);
const EXACT_QUEUE_NAMES = Object.freeze({
  generation_verification: 'part-crm-generation-verification-staging-d3',
  integration_events: 'part-crm-integration-events-staging-d3',
  mailbox_jobs: 'part-crm-mailbox-jobs-staging-d3',
  mailbox_jobs_dlq: 'part-crm-mailbox-jobs-dlq-staging-d3',
});
const EXACT_PROVIDER_ISSUED = Object.freeze([
  'R2_GENERATION_ACCESS_KEY_ID',
  'R2_GENERATION_SECRET_ACCESS_KEY',
  'CLOUDFLARE_API_TOKEN',
  'CLOUDFLARE_ACCESS_CLIENT_ID',
  'CLOUDFLARE_ACCESS_CLIENT_SECRET',
]);
const EXACT_PROJECT_GENERATED = Object.freeze([
  'CLIENT_CONTACT_PROTECTION_KEYRING',
  'MAILBOX_RESOLVER_CALLER_AUTH_KEY',
  'MAILBOX_RESOLVER_ENCRYPTION_KEYRING',
  'MAILBOX_RESOLVER_HANDLE_HMAC_KEY',
]);

function invariant(condition, message) {
  if (!condition) throw new Error(message);
}

function sameArray(actual, expected, label) {
  invariant(Array.isArray(actual), `${label} must be an array`);
  invariant(actual.length === expected.length, `${label} length drifted`);
  for (let index = 0; index < expected.length; index += 1) {
    invariant(actual[index] === expected[index], `${label} drifted at index ${index}: expected ${expected[index]}, got ${actual[index]}`);
  }
}

function sameSet(actual, expected, label) {
  invariant(Array.isArray(actual), `${label} must be an array`);
  const a = [...actual].sort();
  const e = [...expected].sort();
  sameArray(a, e, label);
}

async function json(path) {
  return JSON.parse(await readFile(path, 'utf8'));
}

function assertNoSecretMaterial(value, path = 'root') {
  if (Array.isArray(value)) {
    value.forEach((item, index) => assertNoSecretMaterial(item, `${path}[${index}]`));
    return;
  }
  if (value && typeof value === 'object') {
    for (const [key, item] of Object.entries(value)) {
      invariant(!/^(value|secret_value|plaintext|token_value|password|private_key|key_material|raw_secret|raw_token)$/i.test(key), `forbidden secret-bearing field in authority: ${path}.${key}`);
      assertNoSecretMaterial(item, `${path}.${key}`);
    }
    return;
  }
  if (typeof value === 'string') {
    invariant(!/\b(gh[pousr]_[A-Za-z0-9_]{20,}|github_pat_[A-Za-z0-9_]{20,}|AKIA[0-9A-Z]{16})\b/.test(value), `secret-shaped material found at ${path}`);
    invariant(!/\$\{[^}]+\}/.test(value), `placeholder interpolation found at ${path}`);
  }
}

function validateAuthority(authority, credentialAuthority, historicalAuthority, migrationSource) {
  invariant(authority.schema_version === 2, 'authority schema_version must be 2');
  invariant(authority.status === 'AR8C_STAGING_PROVIDER_EXECUTION_AUTHORITY', 'authority status drifted');
  invariant(authority.parent_issue === 308 && authority.implementation_issue === 314, 'AR-8 issue linkage drifted');
  invariant(authority.environment === 'staging', 'authority must remain staging-only');
  invariant(authority.role === 'EXECUTION_PREREQUISITE_ONLY_NOT_CREDENTIAL_REGISTRY', 'authority role drifted');
  invariant(authority.canonical_credential_authority === CREDENTIAL_AUTHORITY_PATH, 'canonical credential authority path drifted');
  invariant(authority.accepted_bootstrap_design_authority === HISTORICAL_AUTHORITY_PATH, 'historical bootstrap authority path drifted');
  invariant(credentialAuthority?.status === 'ACCEPTED_AR8B_CREDENTIAL_METADATA_AUTHORITY', 'canonical AR-8B credential authority is not accepted');
  invariant(credentialAuthority?.metadata_only === true, 'canonical credential authority must remain metadata-only');
  invariant(credentialAuthority?.invariants?.production_mutation === false, 'canonical credential authority must keep production mutation forbidden');
  invariant(credentialAuthority?.invariants?.ar9_blocked === true, 'canonical credential authority must keep AR-9 blocked');

  const invariants = authority.invariants ?? {};
  for (const key of [
    'plaintext_in_git',
    'placeholder_or_guessed_provider_state',
    'secret_value_readback',
    'production_read_or_mutation',
    'terraform',
    'scheduled_or_routine_provisioning',
    'conflicting_resource_creation',
    'resource_delete_or_rename',
    'bootstrap_credential_equals_steady_state_credential',
    'runtime_secret_readback',
    'ar8c_reconciliation_mutation',
    'ar9_before_full_ar8_acceptance',
  ]) {
    invariant(invariants[key] === 'FORBIDDEN', `${key} must remain FORBIDDEN`);
  }
  invariant(invariants.workflow_dispatch_only === true, 'workflow_dispatch_only must remain true');
  invariant(invariants.protected_staging_environment_required === true, 'protected staging environment is required');
  invariant(invariants.accepted_main_only === true, 'accepted_main_only must remain true');
  invariant(invariants.read_only_discovery_before_mutation === true, 'read-only discovery must remain first');
  invariant(invariants.create_only_missing === true, 'create_only_missing must remain true');

  const classification = authority.classification_contract ?? {};
  sameArray(classification.allowed_states, EXACT_CLASSIFICATIONS, 'classification states');
  invariant(classification.create_allowed_only_for === 'MISSING', 'creation must be limited to MISSING');
  invariant(classification.reuse_allowed_only_for === 'PRESENT_AND_MATCHING', 'reuse must be limited to PRESENT_AND_MATCHING');
  sameSet(classification.fail_closed_states, ['PRESENT_BUT_CONFLICTING', 'UNKNOWN_DUE_TO_AUTHORIZATION'], 'fail-closed states');
  invariant(/duplicate resolver D1 is forbidden/i.test(classification.exception ?? ''), 'resolver exception must explicitly forbid duplicate D1 creation');

  const roles = authority.accepted_staging_resource_roles ?? {};
  invariant(roles.resolver_d1?.accepted_name === 'mailbox-secret-resolver-gate-b-bootstrap-20260815-033400', 'resolver D1 name drifted');
  invariant(roles.resolver_d1?.duplicate_creation === 'FORBIDDEN', 'resolver duplicate creation must remain forbidden');
  invariant(roles.catalog_d1?.accepted_name === 'part-crm-catalog-staging-d3-20260815', 'catalog D1 name drifted');
  invariant(roles.catalog_d1?.required_state === 'PRESENT_AND_MATCHING', 'catalog D1 must remain match-only');
  sameSet(roles.forbidden_catalog_targets, ['part-crm-catalog-staging', 'part-crm-catalog-production', 'part-crm-d3a-bootstrap-proof'], 'forbidden catalog targets');
  invariant(roles.profile_r2?.accepted_name === 'part-crm-profile-objects-staging-d3', 'profile R2 name drifted');
  invariant(roles.profile_r2?.unclaimed_existing_bucket === 'part-crm-browser-profiles', 'unclaimed R2 bucket declaration drifted');
  invariant(JSON.stringify(roles.queues) === JSON.stringify(EXACT_QUEUE_NAMES), 'accepted queue names drifted');
  invariant(roles.resolver_worker?.accepted_name === 'mailbox-secret-resolver-staging', 'resolver Worker name drifted');
  invariant(roles.control_plane_worker?.accepted_name === 'browser-profile-control-plane-staging', 'control-plane Worker name drifted');
  invariant(roles.custom_domain === 'staging.alegria.by', 'staging custom domain drifted');
  invariant(roles.access?.application_name === 'part-crm-staging-control-plane', 'Access application name drifted');
  invariant(roles.access?.service_token_name === 'part-crm-staging-github-service-auth', 'Access service-token name drifted');
  invariant(roles.access?.service_auth_policy_name === 'part-crm-staging-service-auth', 'Access policy name drifted');

  const convergence = authority.resolver_schema_convergence ?? {};
  invariant(convergence.target_name === roles.resolver_d1.accepted_name, 'resolver convergence target must equal accepted resolver D1');
  invariant(convergence.migration_source === 'migrations/resolver-d1', 'resolver migration source drifted');
  invariant(/exact ordered prefix/i.test(convergence.precondition ?? ''), 'resolver convergence must require exact ordered migration prefix');
  invariant(/exact missing ordered suffix/i.test(convergence.allowed_operation ?? ''), 'resolver convergence must be suffix-only');
  invariant(convergence.current_expected_missing_migration === '0002_oauth_refresh_fencing.sql', 'current resolver missing migration drifted');
  sameSet(convergence.allowed_sql_classes, [
    'PRAGMA foreign_keys',
    'ALTER TABLE resolver_encrypted_records ADD COLUMN',
    'CREATE INDEX',
  ], 'allowed resolver SQL classes');
  sameSet(convergence.forbidden_sql_tokens, ['DROP', 'DELETE', 'UPDATE', 'INSERT', 'REPLACE', 'VACUUM', 'ATTACH', 'DETACH'], 'forbidden resolver SQL tokens');
  invariant(/PRESENT_AND_MATCHING/.test(convergence.postcondition ?? ''), 'resolver convergence must reclassify to PRESENT_AND_MATCHING');
  invariant(/No reverse\/destructive migration/.test(convergence.rollback_model ?? ''), 'destructive resolver rollback must remain unauthorized');

  invariant(/^-- AR-8: durable OAuth refresh/.test(migrationSource), 'expected resolver migration source drifted');
  for (const token of convergence.forbidden_sql_tokens) {
    const expression = new RegExp(`\\b${token}\\b`, 'i');
    invariant(!expression.test(migrationSource), `resolver migration contains forbidden SQL token ${token}`);
  }
  invariant((migrationSource.match(/ALTER TABLE resolver_encrypted_records/gi) ?? []).length === 5, 'resolver migration must contain exactly five accepted ADD COLUMN statements');
  invariant((migrationSource.match(/CREATE INDEX/gi) ?? []).length === 2, 'resolver migration must contain exactly two accepted indexes');

  sameArray(authority.bootstrap_execution_order, EXACT_ORDER, 'bootstrap execution order');
  sameSet(authority.project_generated_secret_classes, EXACT_PROJECT_GENERATED, 'project-generated secret classes');
  sameSet(authority.provider_issued_secret_classes, EXACT_PROVIDER_ISSUED, 'provider-issued secret classes');
  invariant(/Cloudflare Worker secret stores/.test(authority.runtime_secret_destination ?? ''), 'runtime secret destination must remain Worker secret stores');
  invariant(/readback is forbidden/i.test(authority.runtime_secret_destination ?? ''), 'runtime secret readback must remain forbidden');

  invariant(authority.staging_outputs?.github_environment === 'staging', 'staging output environment drifted');
  sameSet(authority.staging_outputs?.required_new_or_verified_bindings, EXACT_OUTPUTS, 'required staging outputs');
  sameSet(authority.staging_outputs?.transitional_existing_bundle_bindings, EXACT_TRANSITIONAL_BUNDLES, 'transitional staging bundles');
  invariant(/provider IDs are never precommitted or guessed/.test(authority.staging_outputs?.manifest_source ?? ''), 'manifest source must prohibit precommitted/guessed provider IDs');

  invariant(authority.failure_policy?.production_match === 'STOP_IMMEDIATELY', 'production match must stop immediately');
  invariant(authority.failure_policy?.resource_conflict === 'STOP_NO_DUPLICATE_CREATION', 'resource conflict must stop duplicate creation');
  invariant(authority.failure_policy?.resolver_non_prefix_schema === 'STOP_REQUIRE_NEW_REMEDIATION_AUTHORITY', 'resolver non-prefix state must require new authority');
  invariant(authority.failure_policy?.secret_readback_required === 'STOP_AND_REDESIGN', 'secret readback requirement must stop and redesign');
  invariant(authority.remaining_sequence?.at(-1) === 'AR-9 only after full AR-8 acceptance', 'AR-9 ordering drifted');

  const historicalText = JSON.stringify(historicalAuthority);
  invariant(/bootstrap/i.test(historicalText), 'historical bootstrap authority is not recognizable');
  invariant(!JSON.stringify(authority).includes('6e82cd8f-6899-4578-a4c3-068f64ac40e0'), 'production provider ID must never be committed into authority');
  assertNoSecretMaterial(authority);
}

async function loadFixture() {
  const [authority, credentialAuthority, historicalAuthority, migrationSource] = await Promise.all([
    json(AUTHORITY_PATH),
    json(CREDENTIAL_AUTHORITY_PATH),
    json(HISTORICAL_AUTHORITY_PATH),
    readFile(RESOLVER_MIGRATION_PATH, 'utf8'),
  ]);
  return { authority, credentialAuthority, historicalAuthority, migrationSource };
}

async function contract() {
  const fixture = await loadFixture();
  validateAuthority(fixture.authority, fixture.credentialAuthority, fixture.historicalAuthority, fixture.migrationSource);
  console.log('AR-8C staging provider execution authority is internally consistent.');
}

async function selfTest() {
  const fixture = await loadFixture();
  const mutations = [
    ['production mutation', (value) => { value.invariants.production_read_or_mutation = 'ALLOWED'; }],
    ['terraform', (value) => { value.invariants.terraform = 'ALLOWED'; }],
    ['create conflicting', (value) => { value.classification_contract.create_allowed_only_for = 'PRESENT_BUT_CONFLICTING'; }],
    ['duplicate resolver', (value) => { value.accepted_staging_resource_roles.resolver_d1.duplicate_creation = 'ALLOWED'; }],
    ['production catalog', (value) => { value.accepted_staging_resource_roles.catalog_d1.accepted_name = 'part-crm-catalog-production'; }],
    ['resolver non-prefix', (value) => { value.resolver_schema_convergence.precondition = 'apply whatever is missing'; }],
    ['destructive sql', (value) => { value.resolver_schema_convergence.forbidden_sql_tokens = value.resolver_schema_convergence.forbidden_sql_tokens.filter((item) => item !== 'DROP'); }],
    ['missing output', (value) => { value.staging_outputs.required_new_or_verified_bindings.pop(); }],
    ['premature ar9', (value) => { value.remaining_sequence[value.remaining_sequence.length - 1] = 'AR-9'; }],
    ['secret field', (value) => { value.secret_value = 'not-allowed'; }],
  ];

  for (const [label, mutate] of mutations) {
    const copy = structuredClone(fixture.authority);
    mutate(copy);
    let rejected = false;
    try {
      validateAuthority(copy, fixture.credentialAuthority, fixture.historicalAuthority, fixture.migrationSource);
    } catch {
      rejected = true;
    }
    invariant(rejected, `negative fixture was not rejected: ${label}`);
  }
  console.log('AR-8C staging provider execution authority negative fixtures passed.');
}

const command = process.argv[2] ?? 'contract';
if (command === 'contract') await contract();
else if (command === 'self-test') await selfTest();
else throw new Error(`Unknown command: ${command}`);
