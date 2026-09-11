#!/usr/bin/env node

import { readdirSync, readFileSync } from 'node:fs';
import { extname, join, relative } from 'node:path';
import process from 'node:process';

const AUTHORITY_PATH = 'architecture/profile-security.json';
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
const INTERNAL_PROXY_HANDLE_PREFIXES = [
  'migrations/d1/',
  'crates/cloudflare-adapters/src/',
];
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
      errors.push(`${domain.id}: AR-8 must not claim production runtime proof`);
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
  if (!device || device.application_boundary !== 'HANDLE_ONLY'
      || device.material_readback !== false || device.raw_handle_visibility !== false) {
    errors.push('device private key must remain handle-only with no readback');
  }
  const mtls = authority.bridge_machine_mtls_admission;
  if (!mtls
      || mtls.owner !== 'profile-bridge-device-identity-authority'
      || mtls.purpose !== 'TRANSPORT_AUTHENTICATION_ONLY_NOT_DEVICE_OR_RESOURCE_AUTHORIZATION'
      || mtls.v2_environment !== 'staging'
      || mtls.production_enabled !== false
      || mtls.trust_model !== 'DEDICATED_ENVIRONMENT_SCOPED_CLIENT_CA_CHAIN'
      || mtls.environment_cross_trust !== 'FORBIDDEN'
      || mtls.access_policy?.action !== 'SERVICE_AUTH'
      || mtls.access_policy?.include_selector !== 'VALID_CERTIFICATE'
      || mtls.access_policy?.common_name_authorization !== false
      || mtls.access_policy?.ca_trust_grants_device_authorization !== false
      || mtls.device_authorization?.certificate_identity !== 'CANONICAL_LOWERCASE_SHA256_FINGERPRINT'
      || mtls.device_authorization?.admission_rule !== 'ACCESS_ASSERTION_AND_VERIFIED_MTLS_AND_ACTIVE_D1_FINGERPRINT_BINDING'
      || mtls.device_authorization?.ca_or_common_name_alone_authorizes_device !== false
      || mtls.ca_chain?.basic_constraints_ca_required !== true
      || mtls.ca_chain?.minimum_signature_policy !== 'SHA256_OR_STRONGER'
      || mtls.ca_chain?.public_chain_classification !== 'NON_SECRET_EXTERNAL_FACT'
      || mtls.ca_chain?.public_chain_digest !== 'SHA256_REQUIRED'
      || mtls.ca_chain?.associated_hostname_source !== 'CANONICAL_ENVIRONMENT_DEPLOYMENT_TARGET'
      || mtls.ca_chain?.provider_generated_certificate_ids_are_source_authority !== false
      || mtls.client_certificate?.scope !== 'UNIQUE_PER_DEVICE'
      || mtls.client_certificate?.client_auth_eku_oid !== '1.3.6.1.5.5.7.3.2'
      || mtls.client_certificate?.windows_import_private_key_policy !== 'NON_EXPORTABLE'
      || mtls.client_certificate?.fingerprint_binding_must_precede_effective_device_use !== true
      || mtls.machine_projection?.provider_ids !== 'OBSERVED_NOT_SOURCE_AUTHORED'
      || mtls.machine_projection?.missing_required_input !== 'NOT_READY_FAIL_CLOSED'
      || mtls.machine_projection?.manual_provider_payload !== 'FORBIDDEN'
      || mtls.mutation_authorization !== 'SEPARATE_EXACT_CANDIDATE_ONE_SHOT_REQUIRED'
      || mtls.production_mutation !== false) {
    errors.push('Bridge mTLS transport trust must remain environment-scoped, fingerprint-authorized and fail-closed');
  }
  const mtlsInputs = mtls?.machine_projection?.required_non_secret_inputs ?? [];
  const requiredMtlsInputs = ['canonical_environment', 'canonical_target_hostname', 'ca_chain_pem', 'ca_chain_sha256'];
  if (mtlsInputs.length !== requiredMtlsInputs.length
      || requiredMtlsInputs.some((input) => !mtlsInputs.includes(input))) {
    errors.push('Bridge mTLS projection inputs must remain exact and non-secret');
  }
  const mtlsEffects = mtls?.machine_projection?.desired_access_effects ?? [];
  const requiredMtlsEffects = [
    'ENSURE_MTLS_CA_CHAIN_ASSOCIATED_WITH_CANONICAL_HOSTNAME',
    'ENSURE_SERVICE_AUTH_VALID_CERTIFICATE_POLICY_ATTACHED_TO_EXISTING_ACCESS_APPLICATION',
  ];
  if (mtlsEffects.length !== requiredMtlsEffects.length
      || requiredMtlsEffects.some((effect) => !mtlsEffects.includes(effect))) {
    errors.push('Bridge mTLS desired provider effects drifted');
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

function proxyHandleProof(errors, injectedPublicSource = null) {
  let occurrences = 0;
  for (const publicPath of PUBLIC_BOUNDARY_FILES) {
    const source = publicPath === PUBLIC_BOUNDARY_FILES[0] && injectedPublicSource !== null
      ? injectedPublicSource
      : readFileSync(publicPath, 'utf8');
    for (const identifier of PROXY_IDENTIFIERS) {
      if (source.includes(identifier)) {
        errors.push(`${publicPath}: raw proxy secret handle reached a public/read/operator boundary`);
      }
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
      const lines = source.split(/\r?\n/);
      lines.forEach((line, index) => {
        if (!line.includes(identifier)) return;
        occurrences += 1;
        if (path.endsWith('.rs') || path.endsWith('.sql')) {
          const internal = INTERNAL_PROXY_HANDLE_PREFIXES.some((prefix) => path.startsWith(prefix));
          if (!internal) {
            errors.push(`${path}:${index + 1}: raw proxy handle identifier is outside trusted D1/adapter boundary`);
          }
          if (LOG_MARKERS.some((marker) => line.includes(marker))) {
            errors.push(`${path}:${index + 1}: raw proxy handle identifier appears on a log/debug line`);
          }
        }
      });
    }
  }
  return occurrences;
}

function main() {
  const authority = load(AUTHORITY_PATH);
  const errors = [];
  validateAuthority(authority, errors);
  const occurrences = proxyHandleProof(errors);
  if (errors.length > 0) throw new Error(errors.join('\n'));

  if (process.argv.includes('--self-test')) {
    const mutated = structuredClone(authority);
    mutated.security_domains.find((entry) => entry.id === 'profile-network.proxy-credential').raw_handle_visibility = true;
    const mutatedErrors = [];
    validateAuthority(mutated, mutatedErrors);
    if (mutatedErrors.length === 0) throw new Error('proxy visibility negative fixture unexpectedly passed');

    const mtlsMutated = structuredClone(authority);
    mtlsMutated.bridge_machine_mtls_admission.access_policy.common_name_authorization = true;
    const mtlsErrors = [];
    validateAuthority(mtlsMutated, mtlsErrors);
    if (mtlsErrors.length === 0) throw new Error('Bridge mTLS authorization negative fixture unexpectedly passed');

    const publicSource = `${readFileSync(PUBLIC_BOUNDARY_FILES[0], 'utf8')}\npub const proxy_secret_handle: &str = "forbidden";\n`;
    const boundaryErrors = [];
    proxyHandleProof(boundaryErrors, publicSource);
    if (boundaryErrors.length === 0) throw new Error('public proxy handle negative fixture unexpectedly passed');

    console.log('Profile-security, Bridge-mTLS and proxy-handle negative fixtures rejected as expected.');
    return;
  }
  console.log(`Profile security authority validated; proxy raw-handle repository occurrences inspected=${occurrences}; public/API/operator/log boundaries clean.`);
}

try {
  main();
} catch (error) {
  console.error(`profile security authority check failed: ${error.message}`);
  process.exit(1);
}
