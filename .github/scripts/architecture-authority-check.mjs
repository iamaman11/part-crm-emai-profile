#!/usr/bin/env node

import { readFileSync } from 'node:fs';
import process from 'node:process';

const PATHS = {
  authority: 'architecture/credential-authority.json',
  lifecycle: 'architecture/credential-lifecycle.json',
  profile: 'architecture/profile-security.json',
  historicalRegistry: 'architecture/credential-authority-ar8b.json',
  opsctl: 'tools/opsctl/src/lib.rs',
  governance: '.github/workflows/github-governance-gate.yml',
};
const RETIRED_OPERATOR_PATH = ['architecture', 'operator-contract.json'].join('/');
const EXPECTED_LIFECYCLE = new Set([
  'resolver.encryption-keyring',
  'resolver.handle-hmac',
  'mailbox-resolver.caller-auth',
  'control-plane.client-contact-protection',
  'profile-generation.r2-access',
  'profile-bridge.client-certificate-pki',
  'resolver.google-oauth-application',
  'resolver.microsoft-oauth-application',
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
  const { authority, lifecycle, profile, historicalRegistry } = subjects;

  if (authority.kind !== 'CURRENT_CREDENTIAL_AUTHORITY' || authority.status !== 'current') {
    errors.push('credential-authority.json must be the current credential composition root');
  }
  if (authority.registry_source !== PATHS.historicalRegistry
      || authority.registry_source_role !== 'IMMUTABLE_ACCEPTED_PROVENANCE_DATASET'
      || authority.credential_lifecycle_source !== PATHS.lifecycle
      || authority.profile_security_source !== PATHS.profile) {
    errors.push('credential authority composition references drifted');
  }
  if (Object.hasOwn(authority, 'operator_contract_source')) {
    errors.push('retired operator predecessor may not remain a credential composition source');
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
      || lifecycle.routine_release_secret_transport !== false
      || lifecycle.global_invariants?.exportable_static_credential_max_lifetime !== 'P6M'
      || lifecycle.global_invariants?.non_expiring_exportable_static_credentials !== 'FORBIDDEN'
      || lifecycle.global_invariants?.short_lived_or_federated_identity_preferred !== true
      || lifecycle.global_invariants?.retained_decryption_only_material_may_outlive_active_use_until_dependency_zero !== true) {
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
  const bridgePki = lifecycle.concerns?.find((entry) => entry.id === 'profile-bridge.client-certificate-pki');
  if (!bridgePki
      || bridgePki.externally_issued !== true
      || bridgePki.environment_ownership !== 'EXPLICIT_ENVIRONMENT_SCOPED_PKI'
      || bridgePki.v2_environment !== 'staging'
      || bridgePki.production_enabled !== false
      || bridgePki.routine_release_rotation !== false
      || bridgePki.trust_model !== 'DEDICATED_ENVIRONMENT_SCOPED_CLIENT_CA_CHAIN'
      || bridgePki.issuer !== 'EXTERNAL_PROTECTED_CERTIFICATE_AUTHORITY'
      || bridgePki.ca_signing_material_policy !== 'NO_GIT_NO_ISSUE_NO_ARTIFACT_NO_PROVIDER_PAYLOAD_NO_READBACK'
      || bridgePki.ca_public_chain_classification !== 'NON_SECRET_EXTERNAL_FACT'
      || bridgePki.ca_public_chain_digest !== 'SHA256_REQUIRED'
      || bridgePki.provider_generated_certificate_ids !== 'OBSERVED_NOT_SOURCE_AUTHORITY'
      || bridgePki.ca_or_common_name_grants_device_authorization !== false
      || bridgePki.device_authorization_owner !== 'EXISTING_D1_DEVICE_PRINCIPAL_FINGERPRINT_BINDING'
      || bridgePki.client_certificate_scope !== 'UNIQUE_PER_DEVICE'
      || bridgePki.client_auth_eku_oid !== '1.3.6.1.5.5.7.3.2'
      || bridgePki.maximum_lifetime !== lifecycle.global_invariants?.exportable_static_credential_max_lifetime
      || bridgePki.primary_enrollment !== 'AUTHENTICATED_ONE_SHOT_LOCAL_KEY_CSR'
      || bridgePki.private_key_origin !== 'TARGET_WINDOWS_HOST'
      || bridgePki.private_key_exportable !== false
      || bridgePki.private_key_transport !== 'FORBIDDEN'
      || bridgePki.enrollment_authority !== 'AUTHENTICATED_USER_SESSION_PLUS_ONE_SHOT_ENROLLMENT_CLAIM'
      || bridgePki.certificate_delivery !== 'PUBLIC_CERTIFICATE_CHAIN_ONLY_TO_EXISTING_LOCAL_KEY'
      || bridgePki.operator_pfx_primary_path !== false
      || bridgePki.pfx_admin_recovery_path !== 'OPTIONAL_NOT_B7_PRIMARY_PATH'
      || bridgePki.host_handoff !== 'PASSWORD_PROTECTED_PFX_TO_BRIDGE_HOST_OPS'
      || bridgePki.host_handoff_role !== 'OPTIONAL_ADMIN_RECOVERY_ONLY_NOT_PRIMARY_ENROLLMENT'
      || bridgePki.windows_import_private_key_policy !== 'NON_EXPORTABLE'
      || bridgePki.overlap_model !== 'REPLACEMENT_BOUND_AND_VERIFIED_BEFORE_PREVIOUS_CERTIFICATE_RETIREMENT') {
    errors.push('Bridge certificate lifecycle must use automatic local-key enrollment; PFX is recovery-only and verify-before-retire remains mandatory');
  }
  if (profile.kind !== 'PROFILE_SECURITY_AUTHORITY' || profile.status !== 'current'
      || profile.credential_authority !== PATHS.authority) {
    errors.push('profile security authority root drifted');
  }
  const bridgeAdmission = profile.bridge_machine_mtls_admission;
  if (!bridgeAdmission
      || bridgeAdmission.client_certificate?.primary_enrollment !== 'AUTHENTICATED_ONE_SHOT_LOCAL_KEY_CSR'
      || bridgeAdmission.client_certificate?.private_key_origin !== 'TARGET_WINDOWS_HOST'
      || bridgeAdmission.client_certificate?.private_key_exportable !== false
      || bridgeAdmission.client_certificate?.private_key_transport !== 'FORBIDDEN'
      || bridgeAdmission.client_certificate?.enrollment_authority !== 'AUTHENTICATED_USER_SESSION_PLUS_ONE_SHOT_ENROLLMENT_CLAIM'
      || bridgeAdmission.client_certificate?.certificate_delivery !== 'PUBLIC_CERTIFICATE_CHAIN_ONLY_TO_EXISTING_LOCAL_KEY'
      || bridgeAdmission.client_certificate?.operator_pfx_primary_path !== false
      || bridgeAdmission.client_certificate?.pfx_admin_recovery_path !== 'OPTIONAL_NOT_B7_PRIMARY_PATH') {
    errors.push('profile security must keep Bridge enrollment local-key, automatic and free of operator PFX on the B7 primary path');
  }
  const deviceKey = profile.security_domains?.find((entry) => entry.id === 'profile-bridge.device-private-key');
  const enrollmentClaim = profile.security_domains?.find((entry) => entry.id === 'profile-bridge.enrollment-claim');
  if (!deviceKey
      || deviceKey.application_boundary !== 'HANDLE_ONLY'
      || deviceKey.material_readback !== false
      || deviceKey.raw_handle_visibility !== false
      || deviceKey.primary_enrollment_use !== 'LOCAL_NON_EXPORTABLE_KEY_FOR_CLIENT_CERTIFICATE_CSR'
      || enrollmentClaim?.replay_policy !== 'REJECT_REPLAY_AND_DEVICE_REBIND'
      || enrollmentClaim?.retirement_policy !== 'EXPIRE_OR_SINGLE_SUCCESSFUL_REDEMPTION'
      || enrollmentClaim?.certificate_enrollment_scope !== 'ONE_LOCAL_DEVICE_KEY_ONE_CERTIFICATE_BINDING') {
    errors.push('Bridge device-key/enrollment claim ownership drifted');
  }

  for (const [name, subject] of Object.entries({ authority, lifecycle, profile })) {
    scanForbidden(subject, name, errors);
  }
  for (const forbidden of [
    'architecture/ar8-completion-lifecycle.json',
    'architecture/ar8-operator-rehearsal.json',
    RETIRED_OPERATOR_PATH,
  ]) {
    if (sources.opsctl.includes(forbidden)) {
      errors.push(`opsctl still depends on retired/transitional architecture path: ${forbidden}`);
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
    assertRejected('competing credential authority', duplicate, sources);

    const predecessor = structuredClone(subjects);
    predecessor.authority.operator_contract_source = RETIRED_OPERATOR_PATH;
    assertRejected('retired operator predecessor source', predecessor, sources);

    const revokeFirst = structuredClone(subjects);
    revokeFirst.lifecycle.concerns[0].retire_previous_requires_verified_replacement = false;
    assertRejected('revoke-before-verify', revokeFirst, sources);

    const bridgeCaAuthorizesDevice = structuredClone(subjects);
    bridgeCaAuthorizesDevice.lifecycle.concerns.find((entry) => entry.id === 'profile-bridge.client-certificate-pki').ca_or_common_name_grants_device_authorization = true;
    assertRejected('Bridge CA trust becoming device authorization', bridgeCaAuthorizesDevice, sources);

    const bridgeProduction = structuredClone(subjects);
    bridgeProduction.lifecycle.concerns.find((entry) => entry.id === 'profile-bridge.client-certificate-pki').production_enabled = true;
    assertRejected('Bridge PKI Production pre-enable', bridgeProduction, sources);

    const pfxPrimary = structuredClone(subjects);
    pfxPrimary.lifecycle.concerns.find((entry) => entry.id === 'profile-bridge.client-certificate-pki').operator_pfx_primary_path = true;
    assertRejected('Bridge operator PFX becoming primary enrollment', pfxPrimary, sources);

    const exportableDeviceKey = structuredClone(subjects);
    exportableDeviceKey.profile.bridge_machine_mtls_admission.client_certificate.private_key_exportable = true;
    assertRejected('Bridge primary device key becoming exportable', exportableDeviceKey, sources);

    const reusableEnrollment = structuredClone(subjects);
    reusableEnrollment.profile.security_domains.find((entry) => entry.id === 'profile-bridge.enrollment-claim').retirement_policy = 'REUSABLE';
    assertRejected('Bridge enrollment claim becoming reusable', reusableEnrollment, sources);

    const insecureProfile = structuredClone(subjects);
    insecureProfile.profile.status = 'historical';
    assertRejected('profile authority rollback', insecureProfile, sources);

    console.log('Credential/profile authority negative fixtures rejected; automatic Bridge enrollment remains local-key and fail-closed.');
    return;
  }
  console.log('Credential lifecycle and profile security authorities are canonical; automatic Bridge enrollment is local-key and operator-PFX-free on the primary path.');
}

try {
  main();
} catch (error) {
  console.error(`architecture authority check failed: ${error.message}`);
  process.exit(1);
}
