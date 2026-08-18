#!/usr/bin/env node
import { readFileSync, lstatSync } from 'node:fs';
import { resolve } from 'node:path';

const ROOT = resolve(import.meta.dirname, '../..');
const AUTHORITY_PATH = resolve(ROOT, 'architecture/ar8-operator-rehearsal.json');
const LIFECYCLE_PATH = resolve(ROOT, 'architecture/ar8-completion-lifecycle.json');
const OPSCTL_PATH = resolve(ROOT, 'tools/opsctl/src/lib.rs');
const ROUTINE_PROMOTION_PATH = resolve(ROOT, '.github/workflows/mailbox-secret-resolver-promotion.yml');

const EXPECTED_NEGATIVES = new Set([
  'missing_binding',
  'malformed_keyring',
  'unknown_or_revoked_version',
  'stale_rotation',
  'concurrent_rotation',
  'interrupted_reconciliation',
  'secret_value_leakage',
  'competing_mutator',
  'routine_deploy_secret_transport',
  'cross_environment_mixup',
  'revoke_before_verify',
]);

const EXPECTED_SURFACES = new Map([
  ['status', ['opsctl status', 'docs/status.json']],
  ['inventory', ['opsctl inventory', 'architecture/inventory.json']],
  ['doctor', ['opsctl doctor', 'canonical validators plus AR-8 candidate authorities']],
  ['credential_lifecycle', ['opsctl credential-lifecycle', 'architecture/ar8-completion-lifecycle.json']],
  ['rotation_plan', ['opsctl rotation-plan', 'architecture/ar8-operator-rehearsal.json']],
]);

const FORBIDDEN_VALUE_KEYS = new Set([
  'value',
  'secret_value',
  'plaintext',
  'plaintext_value',
  'private_key',
  'password',
  'token_value',
  'credential_value',
  'key_material',
  'raw_secret',
  'raw_token',
]);

const ROUTINE_SECRET_TRANSPORT_MARKERS = [
  'CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON',
  'CLOUDFLARE_RESOLVER_SECRETS_JSON',
  'R2_GENERATION_ACCESS_KEY_ID',
  'R2_GENERATION_SECRET_ACCESS_KEY',
  'GOOGLE_OAUTH_CLIENT_SECRET',
  'MICROSOFT_OAUTH_CLIENT_SECRET',
];

function clone(value) {
  return structuredClone(value);
}

function load(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

function requireBoolean(value, expected, label, errors) {
  if (value !== expected) {
    errors.push(`${label} must be ${expected}`);
  }
}

function requireNonEmptyString(value, label, errors) {
  if (typeof value !== 'string' || value.length === 0) {
    errors.push(`${label} must be a non-empty string`);
  }
}

function findForbiddenValueKeys(value, path = '$', errors = []) {
  if (Array.isArray(value)) {
    value.forEach((item, index) => findForbiddenValueKeys(item, `${path}[${index}]`, errors));
    return errors;
  }
  if (value === null || typeof value !== 'object') {
    return errors;
  }
  for (const [key, child] of Object.entries(value)) {
    if (FORBIDDEN_VALUE_KEYS.has(key)) {
      errors.push(`${path}.${key}: secret-value field is forbidden`);
    }
    findForbiddenValueKeys(child, `${path}.${key}`, errors);
  }
  return errors;
}

function evidenceExistsAndContains(entry, errors) {
  if (entry === null || typeof entry !== 'object') {
    errors.push('negative-matrix evidence entry must be an object');
    return;
  }
  requireNonEmptyString(entry.path, 'negative-matrix evidence.path', errors);
  requireNonEmptyString(entry.marker, 'negative-matrix evidence.marker', errors);
  if (typeof entry.path !== 'string' || typeof entry.marker !== 'string') {
    return;
  }
  const path = resolve(ROOT, entry.path);
  let stat;
  try {
    stat = lstatSync(path);
  } catch {
    errors.push(`negative-matrix evidence path is missing: ${entry.path}`);
    return;
  }
  if (!stat.isFile() || stat.isSymbolicLink()) {
    errors.push(`negative-matrix evidence path must be a regular non-symlink file: ${entry.path}`);
    return;
  }
  const source = readFileSync(path, 'utf8');
  if (!source.includes(entry.marker)) {
    errors.push(`negative-matrix evidence marker missing: ${entry.path} :: ${entry.marker}`);
  }
}

function validate(authority, lifecycle, opsctlSource, routinePromotion, verifyEvidence = true) {
  const errors = [];
  if (authority.schema_version !== 1 || authority.kind !== 'AR8F_OPERATOR_REHEARSAL_AUTHORITY') {
    errors.push('AR-8F authority identity/schema is invalid');
  }
  if (
    authority.status !== 'candidate' ||
    authority.tracking_issue !== 361 ||
    authority.parent_issue !== 308 ||
    authority.completion_pr !== 362 ||
    authority.canonical_lifecycle !== 'architecture/ar8-completion-lifecycle.json'
  ) {
    errors.push('AR-8F candidate lineage drifted');
  }
  if (authority.accepted_projection_update !== 'DEFERRED_UNTIL_FINAL_FROZEN_SHA') {
    errors.push('accepted inventory/status/docs projection must remain deferred until the final frozen SHA');
  }

  const policy = authority.environment_policy ?? {};
  if (policy.rehearsal_environment !== 'staging') {
    errors.push('AR-8F rehearsal environment must be staging');
  }
  for (const key of [
    'production_mutation',
    'secret_readback',
    'opsctl_mutation',
    'routine_release_secret_transport',
    'provider_mutation_from_opsctl',
  ]) {
    requireBoolean(policy[key], false, `environment_policy.${key}`, errors);
  }

  const surfaces = authority.operator_surfaces ?? {};
  for (const [name, [command, source]] of EXPECTED_SURFACES) {
    const surface = surfaces[name];
    if (surface?.command !== command || surface?.source !== source) {
      errors.push(`operator surface ${name} must remain canonical: ${command} -> ${source}`);
    }
    if (!['metadata-only', 'read-only-validation'].includes(surface?.mode)) {
      errors.push(`operator surface ${name} must remain read-only`);
    }
  }
  if (Object.keys(surfaces).length !== EXPECTED_SURFACES.size) {
    errors.push('operator surface set drifted');
  }
  for (const marker of [
    'rotation-plan',
    'architecture/ar8-operator-rehearsal.json',
    'AR-8F keeps this interface read-only',
  ]) {
    if (!opsctlSource.includes(marker)) {
      errors.push(`opsctl lost AR-8F read-only marker: ${marker}`);
    }
  }
  for (const mutable of ['"rotate"', '"deploy"', '"promote"', '"provision"']) {
    if (opsctlSource.includes(`${mutable} => Ok(`)) {
      errors.push(`opsctl exposed forbidden mutation command: ${mutable}`);
    }
  }

  const audit = authority.audit_lineage ?? {};
  if (audit.mode !== 'metadata-only') {
    errors.push('audit lineage must be metadata-only');
  }
  if (!Array.isArray(audit.allowed_fields) || !Array.isArray(audit.forbidden_fields)) {
    errors.push('audit lineage fields must be explicit lists');
  } else {
    const allowed = new Set(audit.allowed_fields);
    const forbidden = new Set(audit.forbidden_fields);
    for (const key of FORBIDDEN_VALUE_KEYS) {
      if (!forbidden.has(key)) {
        errors.push(`audit lineage must forbid value field ${key}`);
      }
      if (allowed.has(key)) {
        errors.push(`audit lineage cannot allow secret-value field ${key}`);
      }
    }
    for (const required of ['concern_id', 'environment', 'credential_or_key_id', 'version', 'state', 'source_sha', 'evidence_ref']) {
      if (!allowed.has(required)) {
        errors.push(`audit lineage missing metadata field ${required}`);
      }
    }
  }

  const ceremonyPolicy = authority.ceremony_policy ?? {};
  const expectedStages = ['precheck', 'issue_or_import', 'bind_replacement', 'switch_active', 'verify', 'retire_previous'];
  if (JSON.stringify(ceremonyPolicy.stage_order) !== JSON.stringify(expectedStages)) {
    errors.push('AR-8F ceremony stage order drifted');
  }
  if (
    ceremonyPolicy.retirement_rule !== 'VERIFY_REPLACEMENT_BEFORE_RETIRE_PREVIOUS' ||
    ceremonyPolicy.rollback_rule !== 'KEEP_PREVIOUS_VALID_OR_RETAINED_UNTIL_REPLACEMENT_IS_VERIFIED' ||
    ceremonyPolicy.cross_environment_rule !== 'TARGET_ENVIRONMENT_IDENTIFIERS_MUST_MATCH_STAGING_BEFORE_ANY_STAGING_CEREMONY' ||
    ceremonyPolicy.recovery_required !== true
  ) {
    errors.push('AR-8F verify-before-retire / rollback / cross-environment policy drifted');
  }

  const lifecycleConcerns = Array.isArray(lifecycle.concerns) ? lifecycle.concerns : [];
  const expectedConcerns = new Map(lifecycleConcerns.map((item) => [item.id, item.logical_slice]));
  const plans = Array.isArray(authority.concern_plans) ? authority.concern_plans : [];
  const seen = new Set();
  for (const plan of plans) {
    if (seen.has(plan.id)) {
      errors.push(`duplicate AR-8F concern plan: ${plan.id}`);
      continue;
    }
    seen.add(plan.id);
    if (expectedConcerns.get(plan.id) !== plan.logical_slice) {
      errors.push(`${plan.id}: logical slice disagrees with lifecycle authority`);
    }
    if (plan.environment !== 'staging' || plan.mutation_executed !== false || plan.secret_readback !== false) {
      errors.push(`${plan.id}: rehearsal must be staging-scoped, non-mutating evidence with no secret readback`);
    }
    requireNonEmptyString(plan.rehearsal_mode, `${plan.id}.rehearsal_mode`, errors);
    requireNonEmptyString(plan.recovery, `${plan.id}.recovery`, errors);
    if (!Array.isArray(plan.ceremony) || plan.ceremony.length !== expectedStages.length) {
      errors.push(`${plan.id}: ceremony must contain exactly ${expectedStages.length} stages`);
    } else {
      const stages = plan.ceremony.map((step) => typeof step === 'string' ? step.split(':', 1)[0] : '');
      if (JSON.stringify(stages) !== JSON.stringify(expectedStages)) {
        errors.push(`${plan.id}: ceremony order must be ${expectedStages.join(' -> ')}`);
      }
    }
  }
  for (const id of expectedConcerns.keys()) {
    if (!seen.has(id)) {
      errors.push(`missing AR-8F concern plan: ${id}`);
    }
  }
  if (seen.size !== expectedConcerns.size || plans.length !== expectedConcerns.size) {
    errors.push('AR-8F must cover exactly the lifecycle concern set');
  }

  const matrix = Array.isArray(authority.negative_matrix) ? authority.negative_matrix : [];
  const matrixIds = new Set();
  for (const item of matrix) {
    if (matrixIds.has(item.id)) {
      errors.push(`duplicate AR-8F negative matrix id: ${item.id}`);
      continue;
    }
    matrixIds.add(item.id);
    if (!EXPECTED_NEGATIVES.has(item.id)) {
      errors.push(`unexpected AR-8F negative matrix id: ${item.id}`);
    }
    if (typeof item.expected !== 'string' || !item.expected.startsWith('FAIL_CLOSED')) {
      errors.push(`${item.id}: expected outcome must be fail-closed`);
    }
    if (!Array.isArray(item.evidence) || item.evidence.length === 0) {
      errors.push(`${item.id}: evidence linkage is required`);
    } else if (verifyEvidence) {
      item.evidence.forEach((entry) => evidenceExistsAndContains(entry, errors));
    }
  }
  for (const id of EXPECTED_NEGATIVES) {
    if (!matrixIds.has(id)) {
      errors.push(`missing AR-8F negative matrix case: ${id}`);
    }
  }
  if (matrixIds.size !== EXPECTED_NEGATIVES.size || matrix.length !== EXPECTED_NEGATIVES.size) {
    errors.push('AR-8F negative matrix must be exact and complete');
  }

  for (const marker of ROUTINE_SECRET_TRANSPORT_MARKERS) {
    if (routinePromotion.includes(marker)) {
      errors.push(`routine promotion transports/references forbidden runtime secret value marker: ${marker}`);
    }
  }

  const finalGate = authority.final_projection_gate ?? {};
  if (
    finalGate.inventory_status_docs_advance !== 'ONLY_ON_ONE_EXACT_FINAL_FROZEN_SHA' ||
    finalGate.full_ar8_accepted !== false ||
    finalGate.ar9_blocked !== true ||
    finalGate.architecture_complete !== false ||
    finalGate.production_core_gate !== 'BLOCKED' ||
    finalGate.production_ready !== false
  ) {
    errors.push('AR-8F final projection gate was advanced prematurely');
  }

  findForbiddenValueKeys(authority, '$', errors);
  return errors;
}

function assertRejected(label, authority, lifecycle, opsctlSource, routinePromotion) {
  const errors = validate(authority, lifecycle, opsctlSource, routinePromotion, false);
  if (errors.length === 0) {
    throw new Error(`negative fixture unexpectedly passed: ${label}`);
  }
}

function selfTest(authority, lifecycle, opsctlSource, routinePromotion) {
  const production = clone(authority);
  production.environment_policy.production_mutation = true;
  assertRejected('production mutation', production, lifecycle, opsctlSource, routinePromotion);

  const secretReadback = clone(authority);
  secretReadback.environment_policy.secret_readback = true;
  assertRejected('secret readback', secretReadback, lifecycle, opsctlSource, routinePromotion);

  const missingNegative = clone(authority);
  missingNegative.negative_matrix.pop();
  assertRejected('missing negative matrix case', missingNegative, lifecycle, opsctlSource, routinePromotion);

  const crossEnvironment = clone(authority);
  crossEnvironment.concern_plans[0].environment = 'production';
  assertRejected('cross-environment rehearsal', crossEnvironment, lifecycle, opsctlSource, routinePromotion);

  const retireBeforeVerify = clone(authority);
  [retireBeforeVerify.concern_plans[0].ceremony[4], retireBeforeVerify.concern_plans[0].ceremony[5]] =
    [retireBeforeVerify.concern_plans[0].ceremony[5], retireBeforeVerify.concern_plans[0].ceremony[4]];
  assertRejected('retire before verify', retireBeforeVerify, lifecycle, opsctlSource, routinePromotion);

  const duplicate = clone(authority);
  duplicate.concern_plans.push(clone(duplicate.concern_plans[0]));
  assertRejected('duplicate concern plan', duplicate, lifecycle, opsctlSource, routinePromotion);

  const plaintext = clone(authority);
  plaintext.concern_plans[0].secret_value = 'forbidden-fixture';
  assertRejected('secret value field', plaintext, lifecycle, opsctlSource, routinePromotion);

  const premature = clone(authority);
  premature.final_projection_gate.full_ar8_accepted = true;
  assertRejected('premature final projection', premature, lifecycle, opsctlSource, routinePromotion);

  assertRejected(
    'routine deploy secret transport',
    authority,
    lifecycle,
    opsctlSource,
    `${routinePromotion}\nGOOGLE_OAUTH_CLIENT_SECRET`,
  );

  const mutableOpsctl = `${opsctlSource}\n"rotate" => Ok(ReadCommand::RotationPlan),\n`;
  assertRejected('opsctl mutation surface', authority, lifecycle, mutableOpsctl, routinePromotion);
}

const authority = load(AUTHORITY_PATH);
const lifecycle = load(LIFECYCLE_PATH);
const opsctlSource = readFileSync(OPSCTL_PATH, 'utf8');
const routinePromotion = readFileSync(ROUTINE_PROMOTION_PATH, 'utf8');
const errors = validate(authority, lifecycle, opsctlSource, routinePromotion);
if (errors.length > 0) {
  errors.forEach((error) => console.error(`ERROR: ${error}`));
  process.exit(1);
}
if (process.argv.includes('--self-test')) {
  selfTest(authority, lifecycle, opsctlSource, routinePromotion);
  console.log('AR-8F operator rehearsal authority and negative fixtures passed.');
} else {
  console.log('AR-8F operator rehearsal authority passed.');
}
