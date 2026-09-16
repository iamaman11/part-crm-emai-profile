#!/usr/bin/env node

import { readdirSync, readFileSync } from 'node:fs';
import { extname, join, relative } from 'node:path';
import process from 'node:process';

const AUTHORITY_PATH = 'architecture/profile-security.json';
const WRANGLER_PATH = 'deploy/cloudflare/wrangler.jsonc';
const EXPECTED_DOMAINS = new Set([
  'profile-generation.encryption-key-hierarchy',
  'profile-identity.entropy-root',
  'profile-bridge.device-private-key',
  'profile-network.proxy-credential',
  'profile-bridge.enrollment-claim',
  'profile-generation.short-lived-object-access',
]);
const EXPECTED_NONCREDENTIAL = new Set([
  'profile-bridge.workspace-lock-token',
  'profile-session.launch-intent-state',
  'profile-session.fencing-token',
]);
const PROXY_IDENTIFIERS = ['proxy_secret_handle', 'proxySecretHandle'];
const PUBLIC_BOUNDARY_FILES = [
  'crates/profile-domain/src/lib.rs',
  'crates/application-ports/src/profiles.rs',
  'crates/cloudflare-adapters/src/d1_identity_queries.rs',
  'crates/cloudflare-adapters/src/d1_profile_application.rs',
  'crates/cloudflare-adapters/src/d1_profiles.rs',
  'apps/control-plane-worker/src/profiles.rs',
  'apps/control-plane-worker/src/composition.rs',
  'tools/opsctl/src/lib.rs',
];
const INTERNAL_PROXY_HANDLE_PREFIXES = ['migrations/d1/', 'crates/cloudflare-adapters/src/'];
const SCANNED_EXTENSIONS = new Set(['.rs', '.sql', '.py', '.mjs', '.js', '.ts', '.json', '.md', '.yml', '.yaml', '.toml']);
const IGNORED_DIRS = new Set(['.git', 'target', 'node_modules', 'dist', 'coverage']);
const LOG_MARKERS = ['println!', 'eprintln!', 'dbg!', 'tracing::', 'log::', 'debug!', 'info!', 'warn!', 'error!'];
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

function exactSet(actual, expected) {
  return Array.isArray(actual)
    && actual.length === expected.length
    && expected.every((entry) => actual.includes(entry));
}

function walk(directory, result = []) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.isDirectory() && IGNORED_DIRS.has(entry.name)) continue;
    const path = join(directory, entry.name);
    if (entry.isDirectory()) walk(path, result);
    else if (entry.isFile() && SCANNED_EXTENSIONS.has(extname(entry.name))) result.push(path);
  }
  return result;
}

function scanForbidden(value, path, errors) {
  if (Array.isArray(value)) {
    value.forEach((entry, index) => scanForbidden(entry, `${path}[${index}]`, errors));
    return;
  }
  if (value === null || typeof value !== 'object') return;
  for (const [key, nested] of Object.entries(value)) {
    if (FORBIDDEN_VALUE_KEYS.has(key.toLowerCase())) {
      errors.push(`${path}.${key}: secret value-bearing field is forbidden`);
    }
    scanForbidden(nested, `${path}.${key}`, errors);
  }
}

function validateAuthority(authority, errors) {
  if (authority.kind !== 'PROFILE_SECURITY_AUTHORITY' || authority.status !== 'current') {
    errors.push('profile security authority must be current subject-domain authority');
  }

  const domains = authority.security_domains;
  const ids = Array.isArray(domains) ? domains.map((entry) => entry.id) : [];
  if (ids.length !== EXPECTED_DOMAINS.size || ids.some((id) => !EXPECTED_DOMAINS.has(id))) {
    errors.push('profile security authority must contain exactly the six audited security domains');
  }
  for (const domain of domains ?? []) {
    if (domain.production_proof_complete !== false) {
      errors.push(`${domain.id}: authority must not claim production runtime proof`);
    }
  }

  const proxy = domains?.find((entry) => entry.id === 'profile-network.proxy-credential');
  if (!proxy || proxy.raw_handle_visibility !== false || proxy.public_model_visibility !== false
      || proxy.api_visibility !== false || proxy.operator_visibility !== false
      || proxy.log_audit_debug_visibility !== false || proxy.implementation_owner !== 'AR-10') {
    errors.push('proxy credential raw handle must remain internal-only and AR-10-owned');
  }

  const entropy = domains?.find((entry) => entry.id === 'profile-identity.entropy-root');
  if (!entropy || entropy.entropy_bits !== 256 || entropy.scope !== 'UNIQUE_PER_PROFILE_GENERATION'
      || entropy.raw_handle_visibility !== false || entropy.implementation_owner !== 'AR-10') {
    errors.push('profile entropy root policy drifted');
  }

  const device = domains?.find((entry) => entry.id === 'profile-bridge.device-private-key');
  if (!device
      || device.protected_value_authority !== 'native Windows CNG non-exportable P-256 application key behind opaque key handle'
      || device.legitimate_mutable_authority !== 'single native Windows CNG device-key adapter behind DeviceKeyPort'
      || device.application_boundary !== 'HANDLE_ONLY'
      || device.material_readback !== false
      || device.raw_handle_visibility !== false
      || device.primary_enrollment_use !== 'APPLICATION_DEVICE_REGISTRATION_AND_FRESH_PROOF_OF_POSSESSION') {
    errors.push('Bridge device private key must remain native Windows CNG, P-256, non-exportable and proof-only');
  }

  if (authority.bridge_machine_mtls_admission !== undefined) {
    errors.push('historical Bridge mTLS admission must not be current authority');
  }
  const admission = authority.bridge_device_application_admission;
  if (!admission
      || admission.owner !== 'profile-bridge-device-identity-authority'
      || admission.purpose !== 'DEVICE_BOUND_APPLICATION_AUTHORIZATION'
      || admission.v2_environment !== 'staging'
      || admission.production_enabled !== false
      || admission.human_identity?.owner !== 'CLOUDFLARE_ACCESS_INITIAL_BROWSER_USER_IDENTITY_ONLY'
      || admission.human_identity?.audience_var !== 'ACCESS_AUDIENCE'
      || admission.human_identity?.ordinary_restart_browser_login_required !== false
      || admission.human_identity?.machine_identity_grants_human_actor !== false
      || admission.device_key?.platform !== 'WINDOWS_CNG'
      || admission.device_key?.algorithm !== 'P-256'
      || admission.device_key?.scope !== 'UNIQUE_PER_DEVICE'
      || admission.device_key?.private_key_origin !== 'TARGET_WINDOWS_HOST'
      || admission.device_key?.private_key_exportable !== false
      || admission.device_key?.private_key_transport !== 'FORBIDDEN'
      || admission.device_key?.material_readback !== false
      || admission.device_registration?.binding !== 'AUTHENTICATED_USER_PLUS_DEVICE_ID_PLUS_PUBLIC_KEY_PLUS_PROOF_OF_POSSESSION'
      || admission.device_registration?.one_time_browser_pairing !== true
      || admission.device_registration?.backend_registered_device_state !== true
      || admission.device_registration?.disabled_user_or_device !== 'FAIL_CLOSED'
      || admission.application_session?.owner !== 'BACKEND_APPLICATION_AUTHORIZATION'
      || admission.application_session?.binding !== 'USER_PLUS_REGISTERED_DEVICE_PLUS_AUTH_EPOCH'
      || admission.application_session?.revocable !== true
      || admission.application_session?.shared_by_ui_and_bridge_runtime !== true
      || admission.application_session?.cloudflare_access_session_revoke_alone_is_sufficient !== false
      || admission.request_proof?.key !== 'REGISTERED_DEVICE_P256_PUBLIC_KEY'
      || admission.request_proof?.private_key_provider !== 'WINDOWS_CNG_NON_EXPORTABLE'
      || admission.request_proof?.freshness !== 'BOUNDED_FRESH_CHALLENGE_OR_REQUEST_PROOF'
      || admission.request_proof?.invalid_signature !== 'REJECT'
      || admission.request_proof?.replay_or_expiry !== 'REJECT'
      || admission.request_proof?.stale_session_or_auth_epoch !== 'REJECT'
      || admission.legacy_admission?.bridge_access_audience !== 'FORBIDDEN'
      || admission.legacy_admission?.service_token_fallback !== false
      || admission.legacy_admission?.access_mtls_fallback !== false
      || admission.legacy_admission?.custom_ca_required !== false
      || admission.legacy_admission?.x509_client_certificate_required !== false
      || admission.legacy_admission?.csr_required !== false
      || admission.machine_projection?.provider_ids !== 'OBSERVED_NOT_SOURCE_AUTHORED'
      || admission.machine_projection?.missing_required_input !== 'NOT_READY_FAIL_CLOSED'
      || admission.machine_projection?.manual_provider_payload !== 'FORBIDDEN'
      || admission.mutation_authorization !== 'SEPARATE_EXACT_CANDIDATE_ONE_SHOT_REQUIRED'
      || admission.production_mutation !== false) {
    errors.push('Bridge admission must remain human-Access plus registered-device CNG/application-session proof and fail closed');
  }

  const requiredInputs = ['canonical_environment', 'canonical_target_hostname', 'canonical_access_audience'];
  if (!exactSet(admission?.machine_projection?.required_non_secret_inputs, requiredInputs)) {
    errors.push('Bridge device/application projection inputs must remain exact and non-secret');
  }
  const requiredEffects = [
    'ENSURE_HUMAN_ACCESS_IDENTITY_REMAINS_BROWSER_ONLY',
    'ENSURE_LEGACY_BRIDGE_SERVICE_TOKEN_OR_MTLS_ADMISSION_ABSENT',
  ];
  if (!exactSet(admission?.machine_projection?.desired_access_effects, requiredEffects)) {
    errors.push('Bridge device/application desired Access effects drifted');
  }

  const enrollment = domains?.find((entry) => entry.id === 'profile-bridge.enrollment-claim');
  if (!enrollment
      || enrollment.device_registration_scope !== 'ONE_AUTHENTICATED_USER_ONE_DEVICE_KEY_ONE_DEVICE_REGISTRATION'
      || enrollment.certificate_enrollment_scope !== undefined
      || enrollment.replay_policy !== 'REJECT_REPLAY_AND_DEVICE_REBIND') {
    errors.push('Bridge enrollment claim must remain one-shot device registration, not certificate enrollment');
  }

  const objectAccess = domains?.find((entry) => entry.id === 'profile-generation.short-lived-object-access');
  if (!objectAccess || objectAccess.lifetime_policy !== 'SHORT_LIVED_NO_STATIC_BRIDGE_R2_CREDENTIAL'
      || objectAccess.raw_bearer_visibility !== false) {
    errors.push('Profile Bridge object access must remain short-lived and bearer-hidden');
  }

  const runtime = authority.runtime_scope ?? {};
  if (runtime.real_runtime_implementation_owner !== 'AR-10'
      || runtime.rotation_rehearsal_owner !== 'AR-13'
      || runtime.remote_recovery_rehearsal_owner !== 'AR-14'
      || runtime.windows_delivery_owner !== 'AR-15'
      || runtime.windows_signing_trust_owner !== 'AR-15B'
      || runtime.runtime_bundle_signing_trust_domain !== 'windows.release-signing-trust'
      || runtime.real_runtime_implemented_in_ar8 !== false
      || runtime.production_mutation !== false) {
    errors.push('Camoufox/Profile ownership boundary drifted');
  }

  const noncredential = authority.protected_noncredential_state ?? [];
  const noncredentialIds = noncredential.map((entry) => entry.id);
  if (noncredentialIds.length !== EXPECTED_NONCREDENTIAL.size
      || noncredentialIds.some((id) => !EXPECTED_NONCREDENTIAL.has(id))
      || noncredential.some((entry) => entry.credential_authority !== false)) {
    errors.push('coordination/authorization state must not become credential authority');
  }

  const payload = authority.credential_equivalent_assets?.find((entry) => entry.id === 'browser-profile-generation-payload');
  if (!payload || payload.classification !== 'CREDENTIAL_EQUIVALENT'
      || payload.cloud_storage_policy !== 'APPLICATION_LAYER_ENCRYPTED_IMMUTABLE_GENERATION_ONLY'
      || payload.ordinary_log_audit_export !== false
      || !payload.includes?.includes('cookies')
      || !payload.includes?.includes('key4.db')
      || !payload.includes?.includes('logins.db')) {
    errors.push('browser profile generation payload classification/protection drifted');
  }
  scanForbidden(authority, 'profile-security', errors);
}

function validateBridgeAudienceBindings(config, errors) {
  const stagingVars = config.env?.staging?.vars;
  const productionVars = config.env?.production?.vars;
  if (!stagingVars || stagingVars.ACCESS_AUDIENCE !== '${STAGING_ACCESS_AUDIENCE}') {
    errors.push('staging: canonical human Access audience binding drifted');
  }
  if (stagingVars?.BRIDGE_ACCESS_AUDIENCE !== undefined
      || productionVars?.BRIDGE_ACCESS_AUDIENCE !== undefined) {
    errors.push('application-session Bridge admission must not reintroduce the legacy Bridge Access audience');
  }
}

function proxyHandleProof(errors, injectedPublicSource = null) {
  let occurrences = 0;
  for (const publicPath of PUBLIC_BOUNDARY_FILES) {
    const source = publicPath === PUBLIC_BOUNDARY_FILES[0] && injectedPublicSource !== null
      ? injectedPublicSource
      : readFileSync(publicPath, 'utf8');
    for (const identifier of PROXY_IDENTIFIERS) {
      if (source.includes(identifier)) errors.push(`${publicPath}: raw proxy secret handle reached a public/read/operator boundary`);
    }
  }
  if (injectedPublicSource !== null) return occurrences;

  const self = relative('.', new URL(import.meta.url).pathname).replace(/^\//, '').replaceAll('\\', '/');
  for (const absolutePath of walk('.')) {
    const path = relative('.', absolutePath).replaceAll('\\', '/');
    if (path === self || path.endsWith('profile-security-authority-check.mjs')) continue;
    const source = readFileSync(absolutePath, 'utf8');
    for (const identifier of PROXY_IDENTIFIERS) {
      if (!source.includes(identifier)) continue;
      source.split(/\r?\n/).forEach((line, index) => {
        if (!line.includes(identifier)) return;
        occurrences += 1;
        if (path.endsWith('.rs') || path.endsWith('.sql')) {
          const internal = INTERNAL_PROXY_HANDLE_PREFIXES.some((prefix) => path.startsWith(prefix));
          if (!internal) errors.push(`${path}:${index + 1}: raw proxy handle identifier is outside trusted D1/adapter boundary`);
          if (LOG_MARKERS.some((marker) => line.includes(marker))) {
            errors.push(`${path}:${index + 1}: raw proxy handle identifier appears on a log/debug line`);
          }
        }
      });
    }
  }
  return occurrences;
}

function expectRejected(authority, mutate, message) {
  const candidate = structuredClone(authority);
  mutate(candidate);
  const errors = [];
  validateAuthority(candidate, errors);
  if (errors.length === 0) throw new Error(`${message} negative fixture unexpectedly passed`);
}

function main() {
  const authority = load(AUTHORITY_PATH);
  const wrangler = load(WRANGLER_PATH);
  const errors = [];
  validateAuthority(authority, errors);
  validateBridgeAudienceBindings(wrangler, errors);
  const occurrences = proxyHandleProof(errors);
  if (errors.length > 0) throw new Error(errors.join('\n'));

  if (process.argv.includes('--self-test')) {
    expectRejected(authority, (candidate) => {
      candidate.security_domains.find((entry) => entry.id === 'profile-network.proxy-credential').raw_handle_visibility = true;
    }, 'proxy visibility');
    expectRejected(authority, (candidate) => {
      candidate.bridge_machine_mtls_admission = { owner: 'legacy' };
    }, 'legacy Bridge mTLS authority');
    expectRejected(authority, (candidate) => {
      candidate.bridge_device_application_admission.legacy_admission.service_token_fallback = true;
    }, 'legacy Bridge service-token fallback');
    expectRejected(authority, (candidate) => {
      candidate.bridge_device_application_admission.legacy_admission.access_mtls_fallback = true;
    }, 'legacy Bridge mTLS fallback');
    expectRejected(authority, (candidate) => {
      candidate.bridge_device_application_admission.device_key.private_key_exportable = true;
    }, 'exportable Windows device key');
    expectRejected(authority, (candidate) => {
      candidate.security_domains.find((entry) => entry.id === 'profile-bridge.enrollment-claim').certificate_enrollment_scope = 'legacy';
    }, 'certificate enrollment');

    const audienceMutated = structuredClone(wrangler);
    audienceMutated.env.staging.vars.BRIDGE_ACCESS_AUDIENCE = '${STAGING_BRIDGE_ACCESS_AUDIENCE}';
    const audienceErrors = [];
    validateBridgeAudienceBindings(audienceMutated, audienceErrors);
    if (audienceErrors.length === 0) throw new Error('legacy Bridge Access audience negative fixture unexpectedly passed');

    const productionMutated = structuredClone(wrangler);
    productionMutated.env.production.vars.BRIDGE_ACCESS_AUDIENCE = '${PRODUCTION_ACCESS_AUDIENCE}';
    const productionErrors = [];
    validateBridgeAudienceBindings(productionMutated, productionErrors);
    if (productionErrors.length === 0) throw new Error('Bridge production audience negative fixture unexpectedly passed');

    const publicSource = `${readFileSync(PUBLIC_BOUNDARY_FILES[0], 'utf8')}\npub const proxy_secret_handle: &str = "forbidden";\n`;
    const boundaryErrors = [];
    proxyHandleProof(boundaryErrors, publicSource);
    if (boundaryErrors.length === 0) throw new Error('public proxy handle negative fixture unexpectedly passed');

    console.log('Profile-security current device/session authority and legacy Bridge admission negative fixtures rejected as expected.');
    return;
  }
  console.log(`Profile security authority validated; current Bridge device/session admission enforced; proxy raw-handle repository occurrences inspected=${occurrences}; public/API/operator/log boundaries clean.`);
}

try {
  main();
} catch (error) {
  console.error(`profile security authority check failed: ${error.message}`);
  process.exit(1);
}
