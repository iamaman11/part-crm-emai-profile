#!/usr/bin/env node

import { readFileSync } from 'node:fs';
import process from 'node:process';

const PATHS = {
  authority: 'architecture/credential-authority.json',
  lifecycle: 'architecture/credential-lifecycle.json',
  profile: 'architecture/profile-security.json',
  operator: 'architecture/operator-contract.json',
  historicalRegistry: 'architecture/credential-authority-ar8b.json',
  opsctl: 'tools/opsctl/src/lib.rs',
  governance: '.github/workflows/github-governance-gate.yml',
};

const EXPECTED_LIFECYCLE = new Set([
  'resolver.encryption-keyring',
  'resolver.handle-hmac',
  'mailbox-resolver.caller-auth',
  'control-plane.client-contact-protection',
  'profile-generation.r2-access',
  'resolver.google-oauth-application',
  'resolver.microsoft-oauth-application',
]);

const EXPECTED_NEGATIVE_CASES = new Set([
  'missing_binding',
  'malformed_keyring',
  'unknown_version',
  'revoked_version',
  'stale_rotation',
  'concurrent_rotation',
  'interrupted_reconciliation',
  'secret_leakage',
  'competing_mutator',
  'routine_deploy_secret_transport',
  'environment_cross_binding',
  'revoke_before_verify',
]);

const FORBIDDEN_VALUE_KEYS = new Set([
  'value', 'secret_value', 'plaintext', 'plaintext_value', 'private_key', 'password',
  'token_value', 'credential_value', 'key_material', 'raw_secret', 'raw_token',
]);

function load(path) {
  const parsed = JSON.parse(readFileSync(path, 'utf8'));
  if (parsed === null || Array.isArray(parsed) || typeof parsed !== 'object') {
    throw new Error(`${path}: root must be one JSON object`);
  }
  return parsed;
}

function sameSet(values, expected) {
  return Array.isArray(values)
    && values.length === expected.size
    && values.every((value) => expected.has(value));
}

function scanForbidden(value, path, errors) {
  if (Array.isArray(value)) {
    value.forEach((entry, index) => scanForbidden(entry, `${path}[${index}]`, errors));
    return;
  }
  if (value === null || typeof value !== 'object') return;
  for (const [key, nested] of Object.entries(value)) {
    if (FORBIDDEN_VALUE_KEYS.has(key.toLowerCase())) {
      errors.push(`${path}.${key}: value-bearing secret field is forbidden`);
    }
    scanForbidden(nested, `${path}.${key}`, errors);
  }
}

function validate(subjects, sources) {
  const errors = [];
  const { authority, lifecycle, profile, operator, historicalRegistry } = subjects;

  if (authority.kind !== 'CURRENT_CREDENTIAL_AUTHORITY' || authority.status !== 'current') {
    errors.push('credential-authority.json must be the current composition root');
  }
  if (authority.registry_source !== PATHS.historicalRegistry
      || authority.registry_source_role !== 'IMMUTABLE_ACCEPTED_PROVENANCE_DATASET'
      || authority.credential_lifecycle_source !== PATHS.lifecycle
      || authority.profile_security_source !== PATHS.profile
      || authority.operator_contract_source !== PATHS.operator) {
    errors.push('credential authority composition references drifted');
  }
  if (authority.historical_provenance?.accepted_ar8b_must_not_be_rewritten !== true
      || historicalRegistry.status !== 'ACCEPTED_AR8B_CREDENTIAL_METADATA_AUTHORITY') {
    errors.push('accepted AR-8B registry must remain immutable provenance');
  }
  if (authority.invariants?.canonical_composition_roots !== 1
      || authority.invariants?.competing_mutable_authority !== 'FORBIDDEN'
      || authority.invariants?.routine_application_release_rotates_credentials !== false
      || authority.invariants?.application_deployment_and_credential_rotation_separated !== true
      || authority.invariants?.dynamic_mailbox_user_oauth_state_authority !== 'AR-8A'
      || authority.invariants?.production_mutation_from_architecture_tooling !== false
      || authority.invariants?.operator_secret_readback !== false) {
    errors.push('credential authority invariants drifted');
  }

  if (lifecycle.kind !== 'CREDENTIAL_LIFECYCLE_AUTHORITY' || lifecycle.status !== 'current'
      || lifecycle.credential_authority !== PATHS.authority
      || lifecycle.production_mutation !== false
      || lifecycle.routine_release_rotates_runtime_secrets !== false
      || lifecycle.routine_release_secret_transport !== false) {
    errors.push('credential lifecycle root/invariants drifted');
  }
  const lifecycleIds = lifecycle.concerns?.map((entry) => entry.id);
  if (!sameSet(lifecycleIds, EXPECTED_LIFECYCLE)) {
    errors.push('credential lifecycle concern set drifted');
  }
  for (const concern of lifecycle.concerns ?? []) {
    if (concern.retire_previous_requires_verified_replacement !== true
        || typeof concern.allowed_mutator !== 'string'
        || !concern.allowed_mutator.includes('rotation')) {
      errors.push(`${concern.id}: lifecycle must use explicit rotation and verify-before-retire`);
    }
    if (typeof concern.recovery !== 'string' || concern.recovery.length === 0) {
      errors.push(`${concern.id}: recovery guidance is required`);
    }
  }
  const google = lifecycle.concerns?.find((entry) => entry.id === 'resolver.google-oauth-application');
  const microsoft = lifecycle.concerns?.find((entry) => entry.id === 'resolver.microsoft-oauth-application');
  for (const oauth of [google, microsoft]) {
    if (!oauth || oauth.user_token_state_authority !== 'AR-8A'
        || oauth.application_credential_failure_state !== 'ConfigurationUnavailable'
        || oauth.user_refresh_rejection_state !== 'ReauthRequired'
        || oauth.environment_ownership !== 'EXPLICIT_ENVIRONMENT_SCOPED_REGISTRATION_AND_BINDING') {
      errors.push('provider application credential state must remain distinct from AR-8A user OAuth state');
    }
  }
  const r2 = lifecycle.concerns?.find((entry) => entry.id === 'profile-generation.r2-access');
  if (!r2 || r2.credential_pair_atomic !== true || r2.routine_release_rotation !== false) {
    errors.push('R2 credential pair must remain atomic and outside routine release rotation');
  }

  if (profile.kind !== 'PROFILE_SECURITY_AUTHORITY' || profile.status !== 'current'
      || profile.credential_authority !== PATHS.authority) {
    errors.push('profile security authority root drifted');
  }
  if (operator.kind !== 'OPERATOR_CONTRACT_AUTHORITY' || operator.status !== 'current'
      || operator.mode !== 'READ_ONLY_METADATA_ONLY'
      || operator.credential_authority !== PATHS.authority
      || operator.credential_lifecycle !== PATHS.lifecycle
      || operator.profile_security !== PATHS.profile) {
    errors.push('operator contract root drifted');
  }
  if (!sameSet(operator.required_negative_cases, EXPECTED_NEGATIVE_CASES)) {
    errors.push('operator negative matrix must contain the complete permanent fail-closed set');
  }
  if (operator.ceremony_policy?.retirement_rule !== 'VERIFY_REPLACEMENT_BEFORE_RETIRE_PREVIOUS'
      || operator.ceremony_policy?.recovery_required !== true) {
    errors.push('operator recovery/retirement policy drifted');
  }
  for (const [name, value] of Object.entries(operator.invariants ?? {})) {
    if (value !== false && name !== 'mode') {
      errors.push(`operator invariant ${name} must remain false`);
    }
  }

  for (const [name, subject] of Object.entries({ authority, lifecycle, profile, operator })) {
    scanForbidden(subject, name, errors);
  }

  for (const forbidden of [
    'architecture/ar8-completion-lifecycle.json',
    'architecture/ar8-operator-rehearsal.json',
  ]) {
    if (sources.opsctl.includes(forbidden)) {
      errors.push(`opsctl still depends on AR-specific mutable architecture path: ${forbidden}`);
    }
  }
  for (const forbidden of [
    '.github/scripts/ar8-completion-lifecycle.mjs',
    '.github/scripts/ar8-profile-security.mjs',
    '.github/scripts/ar8-f-operator-rehearsal.mjs',
  ]) {
    if (sources.governance.includes(`run: node ${forbidden}`)) {
      errors.push(`permanent governance still executes AR-specific candidate validator: ${forbidden}`);
    }
  }
  for (const required of [
    'run: node .github/scripts/architecture-authority-check.mjs',
    'run: node .github/scripts/profile-security-authority-check.mjs',
  ]) {
    if (!sources.governance.includes(required)) {
      errors.push(`permanent governance lost subject-domain validator: ${required}`);
    }
  }
  return errors;
}

function readSubjects() {
  return {
    authority: load(PATHS.authority),
    lifecycle: load(PATHS.lifecycle),
    profile: load(PATHS.profile),
    operator: load(PATHS.operator),
    historicalRegistry: load(PATHS.historicalRegistry),
  };
}

function readSources() {
  return {
    opsctl: readFileSync(PATHS.opsctl, 'utf8'),
    governance: readFileSync(PATHS.governance, 'utf8'),
  };
}

function assertRejected(label, subjects, sources) {
  if (validate(subjects, sources).length === 0) {
    throw new Error(`${label}: negative fixture unexpectedly passed`);
  }
}

function main() {
  const subjects = readSubjects();
  const sources = readSources();
  const errors = validate(subjects, sources);
  if (errors.length > 0) throw new Error(errors.join('\n'));

  if (process.argv.includes('--self-test')) {
    const duplicate = structuredClone(subjects);
    duplicate.authority.invariants.canonical_composition_roots = 2;
    assertRejected('competing authority', duplicate, sources);

    const revokeFirst = structuredClone(subjects);
    revokeFirst.lifecycle.concerns[0].retire_previous_requires_verified_replacement = false;
    assertRejected('revoke-before-verify', revokeFirst, sources);

    const mutableOps = structuredClone(subjects);
    mutableOps.operator.invariants.provider_mutation = true;
    assertRejected('operator mutation', mutableOps, sources);

    console.log('Subject architecture authority negative fixtures rejected as expected.');
    return;
  }
  console.log('Subject credential lifecycle, profile security and operator authorities are canonical and non-competing.');
}

try {
  main();
} catch (error) {
  console.error(`architecture authority check failed: ${error.message}`);
  process.exit(1);
}
