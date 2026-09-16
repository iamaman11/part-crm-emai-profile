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
  'profile-bridge.device-application-identity',
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

  const bridgeIdentity = lifecycle.concerns?.find((entry) => entry.id === 'profile-bridge.device-application-identity');
  if (!bridgeIdentity
      || bridgeIdentity.externally_issued !== false
      || bridgeIdentity.environment_ownership !== 'BACKEND_REGISTERED_DEVICE_AND_APPLICATION_SESSION_AUTHORITY'
      || bridgeIdentity.v2_environment !== 'staging'
      || bridgeIdentity.production_enabled !== false
      || bridgeIdentity.routine_release_rotation !== false
      || bridgeIdentity.key_platform !== 'WINDOWS_CNG'
      || bridgeIdentity.key_algorithm !== 'P-256'
      || bridgeIdentity.device_key_scope !== 'UNIQUE_PER_DEVICE'
      || bridgeIdentity.private_key_origin !== 'TARGET_WINDOWS_HOST'
      || bridgeIdentity.private_key_exportable !== false
      || bridgeIdentity.private_key_transport !== 'FORBIDDEN'
      || bridgeIdentity.material_readback !== false
      || bridgeIdentity.registration_authority !== 'AUTHENTICATED_USER_PLUS_ONE_TIME_BROWSER_PAIRING'
      || bridgeIdentity.registration_binding !== 'AUTHENTICATED_USER_PLUS_DEVICE_ID_PLUS_PUBLIC_KEY_PLUS_PROOF_OF_POSSESSION'
      || bridgeIdentity.application_session_owner !== 'BACKEND_APPLICATION_AUTHORIZATION'
      || bridgeIdentity.application_session_binding !== 'USER_PLUS_REGISTERED_DEVICE_PLUS_AUTH_EPOCH'
      || bridgeIdentity.application_session_revocable !== true
      || bridgeIdentity.request_proof !== 'BOUNDED_FRESH_CHALLENGE_OR_REQUEST_PROOF'
      || bridgeIdentity.legacy_service_token_fallback !== false
      || bridgeIdentity.legacy_access_mtls_fallback !== false
      || bridgeIdentity.x509_client_certificate_required !== false
      || bridgeIdentity.csr_required !== false
      || bridgeIdentity.overlap_model !== 'REPLACEMENT_KEY_REGISTERED_AND_PROVED_BEFORE_PREVIOUS_DEVICE_KEY_RETIREMENT') {
    errors.push('Bridge identity lifecycle must remain local CNG device registration plus revocable application session/proof with no PKI fallback');
  }

  if (profile.kind !== 'PROFILE_SECURITY_AUTHORITY' || profile.status !== 'current'
      || profile.credential_authority !== PATHS.authority) {
    errors.push('profile security authority root drifted');
  }
  if (profile.bridge_machine_mtls_admission !== undefined) {
    errors.push('historical Bridge mTLS admission must not remain current profile authority');
  }
  const bridgeAdmission = profile.bridge_device_application_admission;
  if (!bridgeAdmission
      || bridgeAdmission.owner !== 'profile-bridge-device-identity-authority'
      || bridgeAdmission.purpose !== 'DEVICE_BOUND_APPLICATION_AUTHORIZATION'
      || bridgeAdmission.v2_environment !== 'staging'
      || bridgeAdmission.production_enabled !== false
      || bridgeAdmission.human_identity?.owner !== 'CLOUDFLARE_ACCESS_INITIAL_BROWSER_USER_IDENTITY_ONLY'
      || bridgeAdmission.human_identity?.audience_var !== 'ACCESS_AUDIENCE'
      || bridgeAdmission.human_identity?.ordinary_restart_browser_login_required !== false
      || bridgeAdmission.human_identity?.machine_identity_grants_human_actor !== false
      || bridgeAdmission.device_key?.platform !== 'WINDOWS_CNG'
      || bridgeAdmission.device_key?.algorithm !== 'P-256'
      || bridgeAdmission.device_key?.scope !== 'UNIQUE_PER_DEVICE'
      || bridgeAdmission.device_key?.private_key_origin !== 'TARGET_WINDOWS_HOST'
      || bridgeAdmission.device_key?.private_key_exportable !== false
      || bridgeAdmission.device_key?.private_key_transport !== 'FORBIDDEN'
      || bridgeAdmission.device_key?.material_readback !== false
      || bridgeAdmission.device_registration?.binding !== 'AUTHENTICATED_USER_PLUS_DEVICE_ID_PLUS_PUBLIC_KEY_PLUS_PROOF_OF_POSSESSION'
      || bridgeAdmission.device_registration?.one_time_browser_pairing !== true
      || bridgeAdmission.device_registration?.backend_registered_device_state !== true
      || bridgeAdmission.device_registration?.disabled_user_or_device !== 'FAIL_CLOSED'
      || bridgeAdmission.application_session?.owner !== 'BACKEND_APPLICATION_AUTHORIZATION'
      || bridgeAdmission.application_session?.binding !== 'USER_PLUS_REGISTERED_DEVICE_PLUS_AUTH_EPOCH'
      || bridgeAdmission.application_session?.revocable !== true
      || bridgeAdmission.application_session?.shared_by_ui_and_bridge_runtime !== true
      || bridgeAdmission.application_session?.cloudflare_access_session_revoke_alone_is_sufficient !== false
      || bridgeAdmission.request_proof?.key !== 'REGISTERED_DEVICE_P256_PUBLIC_KEY'
      || bridgeAdmission.request_proof?.private_key_provider !== 'WINDOWS_CNG_NON_EXPORTABLE'
      || bridgeAdmission.request_proof?.freshness !== 'BOUNDED_FRESH_CHALLENGE_OR_REQUEST_PROOF'
      || bridgeAdmission.request_proof?.invalid_signature !== 'REJECT'
      || bridgeAdmission.request_proof?.replay_or_expiry !== 'REJECT'
      || bridgeAdmission.request_proof?.stale_session_or_auth_epoch !== 'REJECT'
      || bridgeAdmission.legacy_admission?.bridge_access_audience !== 'FORBIDDEN'
      || bridgeAdmission.legacy_admission?.service_token_fallback !== false
      || bridgeAdmission.legacy_admission?.access_mtls_fallback !== false
      || bridgeAdmission.legacy_admission?.custom_ca_required !== false
      || bridgeAdmission.legacy_admission?.x509_client_certificate_required !== false
      || bridgeAdmission.legacy_admission?.csr_required !== false
      || bridgeAdmission.machine_projection?.provider_ids !== 'OBSERVED_NOT_SOURCE_AUTHORED'
      || bridgeAdmission.machine_projection?.missing_required_input !== 'NOT_READY_FAIL_CLOSED'
      || bridgeAdmission.machine_projection?.manual_provider_payload !== 'FORBIDDEN'
      || bridgeAdmission.mutation_authorization !== 'SEPARATE_EXACT_CANDIDATE_ONE_SHOT_REQUIRED'
      || bridgeAdmission.production_mutation !== false) {
    errors.push('profile security must keep Bridge admission on local CNG device registration, revocable application session and fresh proof with no PKI/service-token fallback');
  }

  const requiredInputs = ['canonical_environment', 'canonical_target_hostname', 'canonical_access_audience'];
  if (!sameSet(bridgeAdmission?.machine_projection?.required_non_secret_inputs, new Set(requiredInputs))) {
    errors.push('Bridge device/application projection inputs must remain exact and non-secret');
  }
  const requiredEffects = new Set([
    'ENSURE_HUMAN_ACCESS_IDENTITY_REMAINS_BROWSER_ONLY',
    'ENSURE_LEGACY_BRIDGE_SERVICE_TOKEN_OR_MTLS_ADMISSION_ABSENT',
  ]);
  if (!sameSet(bridgeAdmission?.machine_projection?.desired_access_effects, requiredEffects)) {
    errors.push('Bridge device/application desired Access effects drifted');
  }

  const deviceKey = profile.security_domains?.find((entry) => entry.id === 'profile-bridge.device-private-key');
  const pairingToken = profile.security_domains?.find((entry) => entry.id === 'profile-bridge.device-pairing-token');
  if (!deviceKey
      || deviceKey.application_boundary !== 'HANDLE_ONLY'
      || deviceKey.material_readback !== false
      || deviceKey.raw_handle_visibility !== false
      || deviceKey.primary_enrollment_use !== 'APPLICATION_DEVICE_REGISTRATION_AND_FRESH_PROOF_OF_POSSESSION'
      || !pairingToken
      || pairingToken.class !== 'EPHEMERAL_DEVICE_PAIRING_SECRET'
      || pairingToken.owner !== 'profile-bridge-device-identity-authority'
      || pairingToken.legitimate_mutable_authority !== 'single device application pairing state machine'
      || pairingToken.replay_policy !== 'REJECT_REPLAY_AND_DEVICE_REBIND'
      || pairingToken.retirement_policy !== 'EXPIRE_OR_SINGLE_SUCCESSFUL_PAIRING_COMPLETION'
      || pairingToken.device_registration_scope !== 'ONE_AUTHENTICATED_USER_ONE_DEVICE_KEY_ONE_DEVICE_REGISTRATION'
      || pairingToken.certificate_enrollment_scope !== undefined) {
    errors.push('Bridge device-key/pairing-token ownership drifted from the single device application authority');
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

    const bridgeProduction = structuredClone(subjects);
    bridgeProduction.lifecycle.concerns.find((entry) => entry.id === 'profile-bridge.device-application-identity').production_enabled = true;
    assertRejected('Bridge device identity Production pre-enable', bridgeProduction, sources);

    const lifecycleExportableKey = structuredClone(subjects);
    lifecycleExportableKey.lifecycle.concerns.find((entry) => entry.id === 'profile-bridge.device-application-identity').private_key_exportable = true;
    assertRejected('Bridge lifecycle key becoming exportable', lifecycleExportableKey, sources);

    const lifecycleMtlsFallback = structuredClone(subjects);
    lifecycleMtlsFallback.lifecycle.concerns.find((entry) => entry.id === 'profile-bridge.device-application-identity').legacy_access_mtls_fallback = true;
    assertRejected('Bridge lifecycle mTLS fallback', lifecycleMtlsFallback, sources);

    const lifecycleServiceTokenFallback = structuredClone(subjects);
    lifecycleServiceTokenFallback.lifecycle.concerns.find((entry) => entry.id === 'profile-bridge.device-application-identity').legacy_service_token_fallback = true;
    assertRejected('Bridge lifecycle service-token fallback', lifecycleServiceTokenFallback, sources);

    const lifecycleCertificate = structuredClone(subjects);
    lifecycleCertificate.lifecycle.concerns.find((entry) => entry.id === 'profile-bridge.device-application-identity').x509_client_certificate_required = true;
    assertRejected('Bridge lifecycle certificate admission', lifecycleCertificate, sources);

    const legacyAdmission = structuredClone(subjects);
    legacyAdmission.profile.bridge_machine_mtls_admission = { owner: 'legacy' };
    assertRejected('legacy Bridge mTLS profile authority', legacyAdmission, sources);

    const exportableDeviceKey = structuredClone(subjects);
    exportableDeviceKey.profile.bridge_device_application_admission.device_key.private_key_exportable = true;
    assertRejected('Bridge primary device key becoming exportable', exportableDeviceKey, sources);

    const profileServiceTokenFallback = structuredClone(subjects);
    profileServiceTokenFallback.profile.bridge_device_application_admission.legacy_admission.service_token_fallback = true;
    assertRejected('Bridge profile service-token fallback', profileServiceTokenFallback, sources);

    const reusablePairing = structuredClone(subjects);
    reusablePairing.profile.security_domains.find((entry) => entry.id === 'profile-bridge.device-pairing-token').retirement_policy = 'REUSABLE';
    assertRejected('Bridge pairing token becoming reusable', reusablePairing, sources);

    const duplicatePairingOwner = structuredClone(subjects);
    duplicatePairingOwner.profile.security_domains.find((entry) => entry.id === 'profile-bridge.device-pairing-token').owner = 'profile-bridge-enrollment-authority';
    assertRejected('Bridge duplicate pairing owner resurrection', duplicatePairingOwner, sources);

    const certificateEnrollment = structuredClone(subjects);
    certificateEnrollment.profile.security_domains.find((entry) => entry.id === 'profile-bridge.device-pairing-token').certificate_enrollment_scope = 'legacy';
    assertRejected('Bridge certificate enrollment resurrection', certificateEnrollment, sources);

    const insecureProfile = structuredClone(subjects);
    insecureProfile.profile.status = 'historical';
    assertRejected('profile authority rollback', insecureProfile, sources);

    console.log('Credential/profile authority negative fixtures rejected; Bridge admission remains one-owner local CNG device-bound, session-revocable and PKI-fallback-free.');
    return;
  }
  console.log('Credential lifecycle and profile security authorities are canonical; Bridge admission and pairing are device-bound under one current owner with historical PKI fallback absent.');
}

try {
  main();
} catch (error) {
  console.error(`architecture authority check failed: ${error.message}`);
  process.exit(1);
}