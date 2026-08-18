#!/usr/bin/env node

import { readFileSync } from 'node:fs';
import process from 'node:process';

const OVERLAY_PATH = 'architecture/ar8-completion-lifecycle.json';
const CANONICAL_PATH = 'architecture/credential-authority-ar8b.json';
const ROUTINE_PROMOTION_PATH = '.github/workflows/mailbox-secret-resolver-promotion.yml';
const EXPECTED_STAGE_ORDER = [
  'issue_or_import',
  'validate',
  'bind_replacement',
  'switch_active',
  'verify',
  'retire_previous',
];
const EXPECTED = new Map([
  ['resolver.encryption-keyring', { slice: 'AR-8D.2', cutover: 'AR-8D', external: false }],
  ['resolver.handle-hmac', { slice: 'AR-8D.3', cutover: 'AR-8D', external: false }],
  ['mailbox-resolver.caller-auth', { slice: 'AR-8D.4', cutover: 'AR-8D', external: false }],
  ['control-plane.client-contact-protection', { slice: 'AR-8D.5', cutover: 'AR-8D', external: false }],
  ['profile-generation.r2-access', { slice: 'AR-8D.6', cutover: 'AR-8C', external: true }],
  ['resolver.google-oauth-application', { slice: 'AR-8E', cutover: 'AR-8E', external: true }],
  ['resolver.microsoft-oauth-application', { slice: 'AR-8E', cutover: 'AR-8E', external: true }],
]);
const R2_PAIR_BINDINGS = [
  'R2_GENERATION_ACCESS_KEY_ID',
  'R2_GENERATION_SECRET_ACCESS_KEY',
];
const OAUTH_APP_SECRET_BINDINGS = [
  'GOOGLE_OAUTH_CLIENT_SECRET',
  'MICROSOFT_OAUTH_CLIENT_SECRET',
];
const R2_RETIREMENT_PRECONDITIONS = [
  'replacement token is scoped to the intended account and bucket',
  'replacement Access Key ID and Secret Access Key are bound as one pair',
  'bounded staging R2 access succeeds with replacement',
  'Worker binding metadata verifies replacement before old token revocation',
];
const GOOGLE_RETIREMENT_PRECONDITIONS = [
  'target OAuth client id and authorized redirect URI metadata match the intended environment',
  'replacement client secret is added and enabled while the previous secret remains enabled',
  'replacement secret is write-only bound to GOOGLE_OAUTH_CLIENT_SECRET for the intended resolver environment',
  'bounded staging authorization-code and refresh-token exchanges succeed with the replacement',
  'metadata-only rollout evidence confirms the previous secret is no longer used before disable',
  'previous secret is disabled only after replacement verification and deleted only after a bounded rollback observation window',
];
const MICROSOFT_RETIREMENT_PRECONDITIONS = [
  'target application client id and registered Web redirect URI metadata match the intended environment',
  'replacement passwordCredential is added before removal of the previous credential',
  'replacement secretText is captured only at issuance and write-only bound to MICROSOFT_OAUTH_CLIENT_SECRET for the intended resolver environment',
  'bounded staging authorization-code and refresh-token exchanges succeed with the replacement',
  'replacement passwordCredential keyId and endDateTime metadata are recorded without secret value readback',
  'previous passwordCredential is removed by keyId only after replacement verification',
];
const FORBIDDEN_VALUE_KEYS = /(?:plaintext_value|secret_value|token_value|credential_value|key_hex|private_key)/i;

function load(path) {
  const value = JSON.parse(readFileSync(path, 'utf8'));
  if (value === null || Array.isArray(value) || typeof value !== 'object') {
    throw new Error(`${path}: root must be one JSON object`);
  }
  return value;
}

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

function same(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

function scanForbiddenValueFields(value, path, errors) {
  if (Array.isArray(value)) {
    value.forEach((nested, index) => scanForbiddenValueFields(nested, `${path}[${index}]`, errors));
    return;
  }
  if (value === null || typeof value !== 'object') {
    return;
  }
  for (const [key, nested] of Object.entries(value)) {
    if (FORBIDDEN_VALUE_KEYS.test(key)) {
      errors.push(`${path}.${key}: secret/plaintext value fields are forbidden`);
    }
    scanForbiddenValueFields(nested, `${path}.${key}`, errors);
  }
}

function concernIndex(canonical, errors) {
  const credentials = canonical.credentials;
  if (!Array.isArray(credentials)) {
    errors.push(`${CANONICAL_PATH}: credentials must be an array`);
    return new Map();
  }
  const result = new Map();
  for (const entry of credentials) {
    if (entry === null || Array.isArray(entry) || typeof entry !== 'object') {
      errors.push(`${CANONICAL_PATH}: credential entry must be an object`);
      continue;
    }
    const id = entry.id;
    if (typeof id !== 'string' || id.length === 0 || result.has(id)) {
      errors.push(`${CANONICAL_PATH}: credential ids must be unique non-empty strings`);
      continue;
    }
    result.set(id, entry);
  }
  return result;
}

function requireStringList(value, label, errors) {
  if (
    !Array.isArray(value) ||
    value.length === 0 ||
    value.some((item) => typeof item !== 'string' || item.length === 0)
  ) {
    errors.push(`${label}: expected a non-empty string array`);
  }
}

function requireString(value, label, errors) {
  if (typeof value !== 'string' || value.length === 0) {
    errors.push(`${label}: expected a non-empty string`);
  }
}

function validateOAuthApplicationCommon(concern, errors) {
  if (
    concern.application_profile !== 'CONFIDENTIAL_WEB_OAUTH_CLIENT' ||
    concern.environment_ownership !== 'EXPLICIT_ENVIRONMENT_SCOPED_REGISTRATION_AND_BINDING' ||
    concern.user_token_state_authority !== 'AR-8A' ||
    concern.application_credential_failure_state !== 'ConfigurationUnavailable' ||
    concern.user_refresh_rejection_state !== 'ReauthRequired' ||
    concern.secret_plaintext_evidence !== 'FORBIDDEN'
  ) {
    errors.push(
      `${concern.id}: OAuth application credential lifecycle must remain environment-scoped, metadata-only and subordinate to the AR-8A user-token state authority`,
    );
  }
  for (const field of [
    'client_id_authority',
    'client_secret_authority',
    'provider_application_registration_authority',
    'redirect_configuration_authority',
  ]) {
    requireString(concern[field], `${concern.id}.${field}`, errors);
  }
  requireStringList(concern.provider_metadata_evidence, `${concern.id}.provider_metadata_evidence`, errors);
}

function validateSpecific(concern, errors) {
  switch (concern.id) {
    case 'resolver.encryption-keyring':
      if (concern.overlap_model !== 'ACTIVE_PLUS_RETAINED' || concern.read_old_write_new !== true) {
        errors.push(`${concern.id}: encryption lifecycle must be active+retained and read-old/write-new`);
      }
      break;
    case 'resolver.handle-hmac':
      if (
        concern.overlap_model !== 'ACTIVE_PLUS_BOUNDED_RETAINED_VERSIONS' ||
        concern.lookup_resolution !== 'BOUNDED_RETAINED_VERSION_CANDIDATES'
      ) {
        errors.push(`${concern.id}: lookup-HMAC resolution must be bounded by retained versions`);
      }
      requireStringList(concern.stored_dependency_metadata, `${concern.id}.stored_dependency_metadata`, errors);
      break;
    case 'mailbox-resolver.caller-auth':
      if (
        concern.key_selection !== 'EXPLICIT_AUTHENTICATED_KEY_ID' ||
        concern.single_protocol_policy_required !== true ||
        concern.legacy_v1_compatibility !== 'TEMPORARY_MIGRATION_ONLY'
      ) {
        errors.push(`${concern.id}: service auth must use one explicit-key-id protocol policy with temporary v1 compatibility`);
      }
      break;
    case 'control-plane.client-contact-protection':
      if (
        concern.active_selection !== 'EXPLICIT_VERSION_FIELDS' ||
        concern.active_encryption_field !== 'activeEncryptionVersion' ||
        concern.active_lookup_field !== 'activeLookupVersion'
      ) {
        errors.push(`${concern.id}: active contact keys must be selected by explicit version fields`);
      }
      break;
    case 'profile-generation.r2-access':
      if (
        concern.credential_pair_atomic !== true ||
        concern.provider_identity !== 'R2 API token -> Access Key ID + Secret Access Key' ||
        concern.overlap_model !== 'SEPARATE_PROVIDER_TOKENS' ||
        concern.supersedes_lifecycle_projection !==
          'credential-authority-ar8b.json::ar8c_operational_lifecycle/profile-generation.r2-access' ||
        !same(concern.retirement_preconditions, R2_RETIREMENT_PRECONDITIONS)
      ) {
        errors.push(`${concern.id}: R2 replacement must preserve the exact pair-atomic issue/scope/bind/verify/retire contract and explicitly supersede the AR-8C routine-promotion lifecycle projection`);
      }
      if (!String(concern.allowed_mutator ?? '').includes('explicit-governed-worker-secret-rotation')) {
        errors.push(`${concern.id}: R2 mutation must be an explicit rotation lifecycle, not routine promotion`);
      }
      break;
    case 'resolver.google-oauth-application':
      validateOAuthApplicationCommon(concern, errors);
      if (
        concern.provider_overlap_guarantee !== 'TWO_ENABLED_CLIENT_SECRETS_MAXIMUM' ||
        concern.provider_secret_limit !== 2 ||
        concern.previous_secret_disable_reversible_before_delete !== true ||
        concern.previous_secret_delete_reversible !== false ||
        !same(concern.retirement_preconditions, GOOGLE_RETIREMENT_PRECONDITIONS) ||
        !String(concern.allowed_mutator ?? '').includes('google-oauth-client-secret-issuance')
      ) {
        errors.push(`${concern.id}: Google confidential-web client rotation must use the bounded two-secret add/bind/verify/disable/observe/delete lifecycle`);
      }
      break;
    case 'resolver.microsoft-oauth-application':
      validateOAuthApplicationCommon(concern, errors);
      if (
        concern.provider_overlap_guarantee !== 'MULTIPLE_PASSWORD_CREDENTIALS_SUPPORTED' ||
        concern.password_credential_collection !== 'application.passwordCredentials' ||
        concern.replacement_operation !== 'application:addPassword' ||
        concern.retirement_operation !== 'application:removePassword(keyId)' ||
        concern.expiry_metadata_authority !== 'passwordCredential.endDateTime' ||
        concern.secret_retrieval_semantics !== 'ISSUANCE_RESPONSE_ONLY' ||
        !same(concern.retirement_preconditions, MICROSOFT_RETIREMENT_PRECONDITIONS) ||
        !String(concern.allowed_mutator ?? '').includes('microsoft-entra-application-addpassword')
      ) {
        errors.push(`${concern.id}: Microsoft application-secret rotation must use passwordCredentials addPassword/bind/verify/removePassword(keyId) semantics`);
      }
      break;
    default:
      errors.push(`unexpected AR-8 completion lifecycle concern: ${String(concern.id)}`);
  }
}

function validate(overlay, canonical, routinePromotion) {
  const errors = [];
  if (
    overlay.schema_version !== 1 ||
    overlay.kind !== 'AR8_COMPLETION_LIFECYCLE_OVERLAY' ||
    overlay.status !== 'candidate' ||
    overlay.tracking_issue !== 361 ||
    overlay.parent_issue !== 308 ||
    overlay.canonical_credential_authority !== CANONICAL_PATH ||
    overlay.canonical_inventory !== 'architecture/inventory.json'
  ) {
    errors.push('AR-8 completion lifecycle provenance or canonical authority drifted');
  }
  if (
    overlay.production_mutation !== false ||
    overlay.secret_plaintext_in_git !== false ||
    overlay.opsctl_mutation !== false ||
    overlay.routine_release_rotates_runtime_secrets !== false
  ) {
    errors.push('AR-8 completion lifecycle must keep production, plaintext, opsctl mutation and routine-release rotation fail-closed');
  }
  if (!same(overlay.stage_order, EXPECTED_STAGE_ORDER)) {
    errors.push('AR-8 completion lifecycle stage order drifted');
  }
  scanForbiddenValueFields(overlay, 'overlay', errors);
  for (const binding of [...R2_PAIR_BINDINGS, ...OAUTH_APP_SECRET_BINDINGS]) {
    if (routinePromotion.includes(binding)) {
      errors.push(`${ROUTINE_PROMOTION_PATH}: routine promotion must not transport or rebind ${binding}`);
    }
  }

  const canonicalById = concernIndex(canonical, errors);
  const concerns = overlay.concerns;
  if (!Array.isArray(concerns)) {
    errors.push('AR-8 completion lifecycle concerns must be an array');
    return errors;
  }
  const seen = new Set();
  for (const concern of concerns) {
    if (concern === null || Array.isArray(concern) || typeof concern !== 'object') {
      errors.push('AR-8 completion lifecycle concern must be an object');
      continue;
    }
    const id = concern.id;
    if (typeof id !== 'string' || seen.has(id)) {
      errors.push(`duplicate or invalid lifecycle concern id: ${String(id)}`);
      continue;
    }
    seen.add(id);
    const expected = EXPECTED.get(id);
    if (expected === undefined) {
      errors.push(`unexpected lifecycle concern id: ${id}`);
      continue;
    }
    const canonicalEntry = canonicalById.get(id);
    if (canonicalEntry === undefined) {
      errors.push(`${id}: missing from canonical AR-8B credential authority`);
      continue;
    }
    if (
      concern.logical_slice !== expected.slice ||
      concern.canonical_future_cutover !== expected.cutover ||
      concern.externally_issued !== expected.external ||
      canonicalEntry.future_cutover !== expected.cutover ||
      canonicalEntry.externally_issued !== expected.external
    ) {
      errors.push(`${id}: lifecycle ownership/cutover disagrees with canonical AR-8B metadata`);
    }
    if (
      concern.routine_release_rotation !== false ||
      concern.retire_previous_requires_verified_replacement !== true ||
      typeof concern.allowed_mutator !== 'string' ||
      concern.allowed_mutator.length === 0 ||
      typeof concern.recovery !== 'string' ||
      concern.recovery.length === 0
    ) {
      errors.push(`${id}: lifecycle must separate routine release, verify before retirement and define one mutator/recovery path`);
    }
    requireStringList(concern.retirement_preconditions, `${id}.retirement_preconditions`, errors);
    validateSpecific(concern, errors);
  }
  for (const id of EXPECTED.keys()) {
    if (!seen.has(id)) {
      errors.push(`missing required AR-8 completion lifecycle concern: ${id}`);
    }
  }
  if (seen.size !== EXPECTED.size || concerns.length !== EXPECTED.size) {
    errors.push('AR-8 completion lifecycle must contain exactly the governed D2-D6/E concern set');
  }
  return errors;
}

function assertRejected(label, overlay, canonical, routinePromotion) {
  const errors = validate(overlay, canonical, routinePromotion);
  if (errors.length === 0) {
    throw new Error(`negative fixture unexpectedly passed: ${label}`);
  }
}

function selfTest(overlay, canonical, routinePromotion) {
  const production = clone(overlay);
  production.production_mutation = true;
  assertRejected('production mutation', production, canonical, routinePromotion);

  const routineRotation = clone(overlay);
  routineRotation.concerns.find((item) => item.id === 'profile-generation.r2-access').routine_release_rotation = true;
  assertRejected('R2 routine-release rotation', routineRotation, canonical, routinePromotion);

  const splitR2Pair = clone(overlay);
  splitR2Pair.concerns.find((item) => item.id === 'profile-generation.r2-access').credential_pair_atomic = false;
  assertRejected('R2 split credential pair', splitR2Pair, canonical, routinePromotion);

  const r2RevokeBeforeVerify = clone(overlay);
  r2RevokeBeforeVerify.concerns.find((item) => item.id === 'profile-generation.r2-access').retire_previous_requires_verified_replacement = false;
  assertRejected('R2 revoke-before-verify', r2RevokeBeforeVerify, canonical, routinePromotion);

  const r2MissingScopeProof = clone(overlay);
  r2MissingScopeProof.concerns.find((item) => item.id === 'profile-generation.r2-access').retirement_preconditions.pop();
  assertRejected('R2 missing binding metadata verification', r2MissingScopeProof, canonical, routinePromotion);

  assertRejected(
    'R2 pair transported by routine promotion',
    overlay,
    canonical,
    `${routinePromotion}\n${R2_PAIR_BINDINGS.join('\n')}`,
  );

  const implicitContact = clone(overlay);
  implicitContact.concerns.find((item) => item.id === 'control-plane.client-contact-protection').active_selection = 'JSON_ARRAY_ORDER';
  assertRejected('implicit contact active key', implicitContact, canonical, routinePromotion);

  const googleUnlimited = clone(overlay);
  googleUnlimited.concerns.find((item) => item.id === 'resolver.google-oauth-application').provider_secret_limit = 3;
  assertRejected('Google secret limit drift', googleUnlimited, canonical, routinePromotion);

  const googleNoRollback = clone(overlay);
  googleNoRollback.concerns.find((item) => item.id === 'resolver.google-oauth-application').previous_secret_disable_reversible_before_delete = false;
  assertRejected('Google disable rollback removed', googleNoRollback, canonical, routinePromotion);

  const googleDeleteRollback = clone(overlay);
  googleDeleteRollback.concerns.find((item) => item.id === 'resolver.google-oauth-application').previous_secret_delete_reversible = true;
  assertRejected('Google delete treated as reversible', googleDeleteRollback, canonical, routinePromotion);

  const secondOAuthStateAuthority = clone(overlay);
  secondOAuthStateAuthority.concerns.find((item) => item.id === 'resolver.google-oauth-application').user_token_state_authority = 'AR-8E';
  assertRejected('second OAuth user-token state authority', secondOAuthStateAuthority, canonical, routinePromotion);

  const crossEnvironmentOAuth = clone(overlay);
  crossEnvironmentOAuth.concerns.find((item) => item.id === 'resolver.microsoft-oauth-application').environment_ownership = 'SHARED_ACROSS_ENVIRONMENTS';
  assertRejected('cross-environment OAuth application ownership', crossEnvironmentOAuth, canonical, routinePromotion);

  const microsoftWrongIssueOperation = clone(overlay);
  microsoftWrongIssueOperation.concerns.find((item) => item.id === 'resolver.microsoft-oauth-application').replacement_operation = 'application:updatePassword';
  assertRejected('Microsoft replacement operation drift', microsoftWrongIssueOperation, canonical, routinePromotion);

  const microsoftSecretReadback = clone(overlay);
  microsoftSecretReadback.concerns.find((item) => item.id === 'resolver.microsoft-oauth-application').secret_retrieval_semantics = 'PERSISTENT_READBACK';
  assertRejected('Microsoft secret readback authority', microsoftSecretReadback, canonical, routinePromotion);

  const microsoftRevokeBeforeVerify = clone(overlay);
  microsoftRevokeBeforeVerify.concerns.find((item) => item.id === 'resolver.microsoft-oauth-application').retire_previous_requires_verified_replacement = false;
  assertRejected('Microsoft revoke-before-verify', microsoftRevokeBeforeVerify, canonical, routinePromotion);

  assertRejected(
    'OAuth application secret transported by routine promotion',
    overlay,
    canonical,
    `${routinePromotion}\n${OAUTH_APP_SECRET_BINDINGS.join('\n')}`,
  );

  const duplicate = clone(overlay);
  duplicate.concerns.push(clone(duplicate.concerns[0]));
  assertRejected('duplicate concern authority', duplicate, canonical, routinePromotion);

  const plaintext = clone(overlay);
  plaintext.concerns[0].secret_value = 'forbidden-fixture';
  assertRejected('secret value field', plaintext, canonical, routinePromotion);
}

const overlay = load(OVERLAY_PATH);
const canonical = load(CANONICAL_PATH);
const routinePromotion = readFileSync(ROUTINE_PROMOTION_PATH, 'utf8');
const errors = validate(overlay, canonical, routinePromotion);
if (errors.length > 0) {
  for (const error of errors) {
    console.error(`ERROR: ${error}`);
  }
  process.exit(1);
}
if (process.argv.includes('--self-test')) {
  selfTest(overlay, canonical, routinePromotion);
  console.log('AR-8 completion lifecycle authority and negative fixtures passed.');
} else {
  console.log('AR-8 completion lifecycle authority passed.');
}
