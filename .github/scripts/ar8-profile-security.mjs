#!/usr/bin/env node

import { readFileSync } from 'node:fs';
import process from 'node:process';

const OVERLAY_PATH = 'architecture/ar8-completion-lifecycle.json';
const OPERATOR_PATH = 'architecture/ar8-operator-rehearsal.json';
const CANONICAL_PATH = 'architecture/credential-authority-ar8b.json';
const ENCRYPTED_GENERATION_PATH = 'crates/encrypted-generation-domain/src/container.rs';
const DIRTY_GENERATION_PATH = 'apps/profile-bridge/src/dirty_generation.rs';
const BRIDGE_DOMAIN_PATH = 'crates/bridge-domain/src/lib.rs';
const OPERATOR_FLOW_PATH = 'apps/profile-bridge/src/operator_flow.rs';
const CAMOUFOX_PLAN_PATH = 'docs/CAMOUFOX_RUNTIME_CUTOVER_PLAN.md';
const PYTHON_ESTATE_PATH = 'architecture/python-estate-ar6.json';
const DATA_CLASSIFICATION_PATH = 'docs/DATA_CLASSIFICATION.md';
const THREAT_MODEL_PATH = 'docs/THREAT_MODEL.md';
const ADR1_PATH = 'docs/adr/ADR-0001-fingerprint-stability-policy.md';
const ADR2_PATH = 'docs/adr/ADR-0002-cloud-profile-materialization.md';
const ADR6_PATH = 'docs/adr/ADR-0006-cloud-profile-key-management.md';

const EXPECTED_PROFILE_DOMAINS = new Set([
  'profile-generation.encryption-key-hierarchy',
  'profile-identity.entropy-root',
  'profile-bridge.device-private-key',
  'profile-network.proxy-credential',
  'profile-bridge.enrollment-claim',
  'profile-generation.short-lived-object-access',
]);

const EXPECTED_OPERATOR_GATES = new Map([
  ['profile-generation.encryption-key-hierarchy', 'AR-10_CONSUMER_INTEGRATION_THEN_AR-13_ROTATION_AND_AR-14_RECOVERY'],
  ['profile-identity.entropy-root', 'AR-10'],
  ['profile-bridge.device-private-key', 'AR-10_AND_LATER_PLATFORM_REHEARSAL'],
  ['profile-network.proxy-credential', 'AR-10'],
  ['profile-bridge.enrollment-claim', 'AR-10_RUNTIME_INTEGRATION'],
  ['profile-generation.short-lived-object-access', 'AR-10_RUNTIME_CONTRACT_THEN_STAGING_RECOVERY_REHEARSAL'],
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

function load(path) {
  const value = JSON.parse(readFileSync(path, 'utf8'));
  if (value === null || Array.isArray(value) || typeof value !== 'object') {
    throw new Error(`${path}: root must be one JSON object`);
  }
  return value;
}

function text(path) {
  return readFileSync(path, 'utf8');
}

function clone(value) {
  return structuredClone(value);
}

function requireString(value, label, errors) {
  if (typeof value !== 'string' || value.length === 0) {
    errors.push(`${label}: expected non-empty string`);
  }
}

function requireStringList(value, label, errors) {
  if (
    !Array.isArray(value) ||
    value.length === 0 ||
    value.some((item) => typeof item !== 'string' || item.length === 0)
  ) {
    errors.push(`${label}: expected non-empty string list`);
  }
}

function requireMarkers(source, markers, label, errors) {
  for (const marker of markers) {
    if (!source.includes(marker)) {
      errors.push(`${label}: missing source marker ${marker}`);
    }
  }
}

function scanForbiddenValueFields(value, path, errors) {
  if (Array.isArray(value)) {
    value.forEach((entry, index) => scanForbiddenValueFields(entry, `${path}[${index}]`, errors));
    return;
  }
  if (value === null || typeof value !== 'object') {
    return;
  }
  for (const [key, nested] of Object.entries(value)) {
    if (FORBIDDEN_VALUE_KEYS.has(key.toLowerCase())) {
      errors.push(`${path}.${key}: secret value-bearing field is forbidden`);
    }
    scanForbiddenValueFields(nested, `${path}.${key}`, errors);
  }
}

function canonicalWindowsTrust(canonical, errors) {
  const entries = canonical.future_trust_domains;
  if (!Array.isArray(entries)) {
    errors.push(`${CANONICAL_PATH}: future_trust_domains must be an array`);
    return;
  }
  const windows = entries.filter((entry) => entry?.id === 'windows.release-signing-trust');
  if (windows.length !== 1) {
    errors.push('Windows release-signing trust must remain exactly one canonical future trust domain');
    return;
  }
  const entry = windows[0];
  if (
    entry.future_cutover !== 'AR-15B' ||
    entry.externally_issued !== true ||
    !String(entry.legitimate_mutable_authority ?? '').includes('AR-15B')
  ) {
    errors.push('Windows release-signing trust must remain owned by AR-15B, not AR-8 or AR-10');
  }
}

function validateScope(scope, errors) {
  if (scope === null || Array.isArray(scope) || typeof scope !== 'object') {
    errors.push('camoufox_profile_scope must be an object');
    return;
  }
  const expected = {
    real_runtime_implementation_owner: 'AR-10',
    runtime_cutover_authority: 'docs/CAMOUFOX_RUNTIME_CUTOVER_PLAN.md',
    fingerprint_policy_authority: 'AR-10_MUST_ACCEPT_OR_REPLACE_ADR-0001',
    cloud_profile_key_provider_status: 'ADR-0006_PROPOSED_AND_PRODUCTION_BLOCKING',
    rotation_rehearsal_owner: 'AR-13',
    remote_recovery_rehearsal_owner: 'AR-14',
    windows_delivery_owner: 'AR-15',
    windows_signing_trust_owner: 'AR-15B',
    real_runtime_implemented_in_ar8: false,
    production_runtime_proof_in_ar8: false,
    production_key_recovery_proof_in_ar8: false,
    production_mutation: false,
  };
  for (const [key, wanted] of Object.entries(expected)) {
    if (scope[key] !== wanted) {
      errors.push(`camoufox_profile_scope.${key} must be ${JSON.stringify(wanted)}`);
    }
  }
  if (Object.keys(scope).length !== Object.keys(expected).length) {
    errors.push('camoufox_profile_scope field set drifted');
  }
}

function validateDomainCommon(domain, errors) {
  for (const field of [
    'id',
    'class',
    'owner',
    'protected_value_authority',
    'legitimate_mutable_authority',
    'version_state_source',
  ]) {
    requireString(domain[field], `${domain.id ?? '<missing>'}.${field}`, errors);
  }
  requireStringList(domain.current_source_contracts, `${domain.id ?? '<missing>'}.current_source_contracts`, errors);
  if (domain.production_proof_complete !== false) {
    errors.push(`${domain.id}: AR-8 must not claim production proof for profile/Camoufox credential domains`);
  }
}

function validateDomains(overlay, sources, errors) {
  const domains = overlay.profile_security_domains;
  if (!Array.isArray(domains)) {
    errors.push('profile_security_domains must be an array');
    return;
  }
  const seen = new Set();
  for (const domain of domains) {
    if (domain === null || Array.isArray(domain) || typeof domain !== 'object') {
      errors.push('profile security domain must be an object');
      continue;
    }
    const id = domain.id;
    if (typeof id !== 'string' || seen.has(id)) {
      errors.push(`duplicate/invalid profile security domain id: ${String(id)}`);
      continue;
    }
    seen.add(id);
    if (!EXPECTED_PROFILE_DOMAINS.has(id)) {
      errors.push(`unexpected profile security domain: ${id}`);
      continue;
    }
    validateDomainCommon(domain, errors);

    switch (id) {
      case 'profile-generation.encryption-key-hierarchy':
        if (
          domain.class !== 'CREDENTIAL_EQUIVALENT_ENVELOPE_KEY_HIERARCHY' ||
          domain.plaintext_policy !== 'BOUNDED_MEMORY_ONLY_ZEROIZED_WHERE_OWNED' ||
          domain.metadata_policy !== 'KEY_IDS_WRAPPED_REFERENCES_ALGORITHM_VERSION_AND_DEPENDENCY_STATUS_ONLY' ||
          domain.generation_dek_policy !== 'ONE_RANDOM_DEK_PER_GENERATION_NO_IN_PLACE_ROTATION' ||
          domain.root_kek_overlap_model !== 'DUAL_READ_SINGLE_WRITE' ||
          !String(domain.implementation_owner ?? '').includes('AR-13') ||
          !String(domain.implementation_owner ?? '').includes('AR-14')
        ) {
          errors.push(`${id}: key hierarchy policy/later rehearsal ownership drifted`);
        }
        requireStringList(domain.retirement_preconditions, `${id}.retirement_preconditions`, errors);
        requireMarkers(
          sources.encryptedGeneration,
          ['pub struct GenerationDek', 'impl Drop for GenerationDek', 'self.bytes.zeroize();'],
          ENCRYPTED_GENERATION_PATH,
          errors,
        );
        requireMarkers(
          sources.dirtyGeneration,
          ['pub trait GenerationSealingMaterialPort', 'fn material_for(', 'GenerationDek'],
          DIRTY_GENERATION_PATH,
          errors,
        );
        requireMarkers(
          sources.adr6,
          [
            '**Статус:** proposed, blocks production cloud promotion',
            'Root Wrapping Key version N',
            'Tenant KEK version M',
            'Generation DEK',
            'dual-read/single-write',
            'plaintext key живет в bounded memory',
          ],
          ADR6_PATH,
          errors,
        );
        break;
      case 'profile-identity.entropy-root':
        if (
          domain.class !== 'PROFILE_FINGERPRINT_SECRET_ROOT' ||
          domain.entropy_bits !== 256 ||
          domain.scope !== 'UNIQUE_PER_PROFILE_GENERATION' ||
          domain.plaintext_policy !== 'NO_LOG_AUDIT_SUPPORT_R2_OR_PUBLIC_METADATA' ||
          domain.rotation_policy !== 'NEW_PROFILE_GENERATION_ONLY_NO_IN_PLACE_ENTROPY_REWRITE' ||
          domain.implementation_owner !== 'AR-10'
        ) {
          errors.push(`${id}: fingerprint entropy must remain AR-10-owned, generation-scoped and non-exportable`);
        }
        requireMarkers(
          sources.adr1,
          ['**Статус:** proposed', 'profile_entropy_root', 'HMAC(profile_entropy_root', '256', 'KeyProviderPort'],
          ADR1_PATH,
          errors,
        );
        break;
      case 'profile-bridge.device-private-key':
        if (
          domain.class !== 'DEVICE_PRIVATE_KEY' ||
          domain.application_boundary !== 'HANDLE_ONLY' ||
          domain.material_readback !== false ||
          !String(domain.implementation_owner ?? '').includes('AR-10')
        ) {
          errors.push(`${id}: device private key must stay handle-only with no material readback and later platform proof`);
        }
        requireMarkers(
          sources.bridgeDomain,
          [
            'pub trait DeviceKeyPort',
            'fn ensure_key_handle(&mut self, device_id: &DeviceId) -> Result<String, BridgePortError>;',
          ],
          BRIDGE_DOMAIN_PATH,
          errors,
        );
        requireMarkers(
          sources.operatorFlow,
          ['.ensure_key_handle(&device_id)', '.authenticate(&device_id, &key_handle)'],
          OPERATOR_FLOW_PATH,
          errors,
        );
        break;
      case 'profile-network.proxy-credential':
        if (
          domain.class !== 'TENANT_DYNAMIC_PROXY_CREDENTIAL' ||
          domain.runtime_boundary !== 'HANDLE_OR_SCOPED_EPHEMERAL_MATERIAL_ONLY' ||
          domain.implementation_owner !== 'AR-10'
        ) {
          errors.push(`${id}: proxy credential must remain an AR-10 handle/scoped-material domain`);
        }
        requireMarkers(sources.dataClassification, ['proxy', 'CREDENTIAL_EQUIVALENT'], DATA_CLASSIFICATION_PATH, errors);
        requireMarkers(sources.threatModel, ['proxy', 'device private'], THREAT_MODEL_PATH, errors);
        break;
      case 'profile-bridge.enrollment-claim':
        if (
          domain.class !== 'EPHEMERAL_DEVICE_ENROLLMENT_SECRET' ||
          domain.debug_policy !== 'REDACTED' ||
          domain.replay_policy !== 'REJECT_REPLAY_AND_DEVICE_REBIND' ||
          domain.retirement_policy !== 'EXPIRE_OR_SINGLE_SUCCESSFUL_REDEMPTION'
        ) {
          errors.push(`${id}: enrollment claim expiry/redaction/replay policy drifted`);
        }
        requireMarkers(
          sources.bridgeDomain,
          ['ClaimCode([REDACTED])', 'EnrollmentClaimError::ReplayRejected', 'EnrollmentClaimError::DeviceRebindRejected', 'if now >= self.expires_at'],
          BRIDGE_DOMAIN_PATH,
          errors,
        );
        break;
      case 'profile-generation.short-lived-object-access':
        if (
          domain.class !== 'EPHEMERAL_SCOPED_OBJECT_ACCESS_CREDENTIAL' ||
          domain.scope_policy !== 'TENANT_PROFILE_GENERATION_AND_OPERATION_BOUND' ||
          domain.lifetime_policy !== 'SHORT_LIVED_NO_STATIC_BRIDGE_R2_CREDENTIAL' ||
          !String(domain.implementation_owner ?? '').includes('AR-10')
        ) {
          errors.push(`${id}: Bridge object access must remain short-lived/scoped and distinct from static control-plane R2 access`);
        }
        requireMarkers(
          sources.adr2,
          ['короткоживущие credentials', 'short-lived object credentials', 'key wrapping через `KeyProviderPort`'],
          ADR2_PATH,
          errors,
        );
        break;
      default:
        break;
    }
  }
  for (const id of EXPECTED_PROFILE_DOMAINS) {
    if (!seen.has(id)) {
      errors.push(`missing profile/Camoufox security domain: ${id}`);
    }
  }
  if (seen.size !== EXPECTED_PROFILE_DOMAINS.size || domains.length !== EXPECTED_PROFILE_DOMAINS.size) {
    errors.push('profile_security_domains must contain exactly the audited AR-8 Camoufox/profile domain set');
  }
}

function validateOperatorPlans(operator, errors) {
  if (
    operator.kind !== 'AR8F_OPERATOR_REHEARSAL_AUTHORITY' ||
    operator.status !== 'candidate' ||
    operator.tracking_issue !== 361 ||
    operator.parent_issue !== 308 ||
    operator.canonical_lifecycle !== OVERLAY_PATH
  ) {
    errors.push('AR-8F operator authority provenance drifted for profile security projection');
  }
  const plans = operator.profile_security_plans;
  if (!Array.isArray(plans)) {
    errors.push('AR-8F profile_security_plans must be an array');
    return;
  }
  const seen = new Set();
  for (const plan of plans) {
    const id = plan?.id;
    if (typeof id !== 'string' || seen.has(id) || !EXPECTED_PROFILE_DOMAINS.has(id)) {
      errors.push(`invalid/duplicate AR-8F profile security plan: ${String(id)}`);
      continue;
    }
    seen.add(id);
    if (
      plan.operator_visibility !== 'METADATA_ONLY' ||
      plan.mutation_executed !== false ||
      plan.secret_readback !== false ||
      plan.next_owning_gate !== EXPECTED_OPERATOR_GATES.get(id)
    ) {
      errors.push(`${id}: operator plan must remain metadata-only/non-mutating and point to its exact later owning gate`);
    }
    requireString(plan.ar8_status, `${id}.ar8_status`, errors);
    requireString(plan.operator_rule, `${id}.operator_rule`, errors);
    requireString(plan.recovery_rule, `${id}.recovery_rule`, errors);
  }
  for (const id of EXPECTED_PROFILE_DOMAINS) {
    if (!seen.has(id)) {
      errors.push(`missing AR-8F profile security plan: ${id}`);
    }
  }
  if (seen.size !== EXPECTED_PROFILE_DOMAINS.size || plans.length !== EXPECTED_PROFILE_DOMAINS.size) {
    errors.push('AR-8F profile_security_plans must exactly match the audited profile security domain set');
  }
}

function validateNonCredentialState(overlay, bridgeDomain, errors) {
  const entries = overlay.profile_protected_noncredential_state;
  if (!Array.isArray(entries) || entries.length !== 1) {
    errors.push('profile_protected_noncredential_state must contain exactly workspace-lock-token');
    return;
  }
  const entry = entries[0];
  if (
    entry?.id !== 'profile-bridge.workspace-lock-token' ||
    entry?.class !== 'PROTECTED_COORDINATION_FENCING_STATE' ||
    entry?.credential_authority !== false ||
    entry?.scope !== 'LOCAL_WORKSPACE_SINGLE_WRITER' ||
    entry?.debug_policy !== 'REDACTED' ||
    entry?.persistence_policy !== 'LOCAL_COORDINATION_ONLY_NOT_PROVIDER_CREDENTIAL' ||
    entry?.failure_policy !== 'STALE_OR_MISMATCHED_OWNER_FAILS_CLOSED'
  ) {
    errors.push('workspace lock token must remain explicitly classified as protected non-credential coordination state');
  }
  requireMarkers(bridgeDomain, ['WorkspaceLockToken([REDACTED])', 'WorkspaceLockError::StaleWriter'], BRIDGE_DOMAIN_PATH, errors);
}

function validateCredentialEquivalentAsset(overlay, dataClassification, errors) {
  const assets = overlay.profile_credential_equivalent_assets;
  if (!Array.isArray(assets) || assets.length !== 1) {
    errors.push('profile_credential_equivalent_assets must contain exactly browser-profile-generation-payload');
    return;
  }
  const asset = assets[0];
  if (
    asset?.id !== 'browser-profile-generation-payload' ||
    asset?.classification !== 'CREDENTIAL_EQUIVALENT' ||
    asset?.authority !== DATA_CLASSIFICATION_PATH ||
    asset?.cloud_storage_policy !== 'APPLICATION_LAYER_ENCRYPTED_IMMUTABLE_GENERATION_ONLY' ||
    asset?.ordinary_log_audit_export !== false
  ) {
    errors.push('browser profile payload credential-equivalent classification drifted');
  }
  requireStringList(asset?.includes, 'browser-profile-generation-payload.includes', errors);
  requireMarkers(dataClassification, ['CREDENTIAL_EQUIVALENT', 'key4.db', 'logins.db', 'DEK', 'KEK'], DATA_CLASSIFICATION_PATH, errors);
}

function validateAr10Boundary(sources, errors) {
  requireMarkers(
    sources.camoufoxPlan,
    ['AR-10', 'real Camouhost', 'tools/profile_browser.py', 'AR-15', 'Camoufox'],
    CAMOUFOX_PLAN_PATH,
    errors,
  );
  requireMarkers(
    sources.pythonEstate,
    ['tools/profile_browser.py', '"cutover_slice": "AR-10"', 'runtime/camouhost/main.py', 'legacy_direct_browser_runtime_tool'],
    PYTHON_ESTATE_PATH,
    errors,
  );
}

function validate(overlay, operator, canonical, sources) {
  const errors = [];
  if (
    overlay.schema_version !== 1 ||
    overlay.kind !== 'AR8_COMPLETION_LIFECYCLE_OVERLAY' ||
    overlay.status !== 'candidate' ||
    overlay.tracking_issue !== 361 ||
    overlay.parent_issue !== 308 ||
    overlay.canonical_inventory !== 'architecture/inventory.json'
  ) {
    errors.push('AR-8 profile security overlay provenance drifted');
  }
  if (overlay.production_mutation !== false || overlay.secret_plaintext_in_git !== false) {
    errors.push('AR-8 profile security audit must remain non-production and plaintext-free');
  }
  scanForbiddenValueFields(overlay, 'overlay', errors);
  scanForbiddenValueFields(operator, 'operator', errors);
  validateScope(overlay.camoufox_profile_scope, errors);
  validateDomains(overlay, sources, errors);
  validateOperatorPlans(operator, errors);
  validateNonCredentialState(overlay, sources.bridgeDomain, errors);
  validateCredentialEquivalentAsset(overlay, sources.dataClassification, errors);
  validateAr10Boundary(sources, errors);
  canonicalWindowsTrust(canonical, errors);
  return errors;
}

function assertRejected(label, overlay, operator, canonical, sources) {
  const errors = validate(overlay, operator, canonical, sources);
  if (errors.length === 0) {
    throw new Error(`negative fixture unexpectedly passed: ${label}`);
  }
}

function selfTest(overlay, operator, canonical, sources) {
  const realRuntime = clone(overlay);
  realRuntime.camoufox_profile_scope.real_runtime_implemented_in_ar8 = true;
  assertRejected('real Camoufox runtime claimed in AR-8', realRuntime, operator, canonical, sources);

  const missingDomain = clone(overlay);
  missingDomain.profile_security_domains.pop();
  assertRejected('missing profile security domain', missingDomain, operator, canonical, sources);

  const entropyLeak = clone(overlay);
  entropyLeak.profile_security_domains.find((item) => item.id === 'profile-identity.entropy-root').secret_value = 'forbidden';
  assertRejected('entropy secret value field', entropyLeak, operator, canonical, sources);

  const deviceReadback = clone(overlay);
  deviceReadback.profile_security_domains.find((item) => item.id === 'profile-bridge.device-private-key').material_readback = true;
  assertRejected('device private-key material readback', deviceReadback, operator, canonical, sources);

  const staticBridgeR2 = clone(overlay);
  staticBridgeR2.profile_security_domains.find((item) => item.id === 'profile-generation.short-lived-object-access').lifetime_policy = 'STATIC_R2_PAIR_ON_BRIDGE';
  assertRejected('static Bridge R2 credential', staticBridgeR2, operator, canonical, sources);

  const entropyInPlace = clone(overlay);
  entropyInPlace.profile_security_domains.find((item) => item.id === 'profile-identity.entropy-root').rotation_policy = 'IN_PLACE_MUTATION';
  assertRejected('in-place profile entropy mutation', entropyInPlace, operator, canonical, sources);

  const workspaceCredential = clone(overlay);
  workspaceCredential.profile_protected_noncredential_state[0].credential_authority = true;
  assertRejected('workspace lock promoted to credential authority', workspaceCredential, operator, canonical, sources);

  const productionProof = clone(overlay);
  productionProof.profile_security_domains[0].production_proof_complete = true;
  assertRejected('premature profile key production proof', productionProof, operator, canonical, sources);

  const missingOperatorPlan = clone(operator);
  missingOperatorPlan.profile_security_plans.pop();
  assertRejected('missing operator profile security plan', overlay, missingOperatorPlan, canonical, sources);

  const operatorReadback = clone(operator);
  operatorReadback.profile_security_plans[0].secret_readback = true;
  assertRejected('operator profile secret readback', overlay, operatorReadback, canonical, sources);

  const windowsInAr8 = clone(canonical);
  windowsInAr8.future_trust_domains.find((item) => item.id === 'windows.release-signing-trust').future_cutover = 'AR-8';
  assertRejected('Windows signing ownership pulled into AR-8', overlay, operator, windowsInAr8, sources);
}

const overlay = load(OVERLAY_PATH);
const operator = load(OPERATOR_PATH);
const canonical = load(CANONICAL_PATH);
const sources = {
  encryptedGeneration: text(ENCRYPTED_GENERATION_PATH),
  dirtyGeneration: text(DIRTY_GENERATION_PATH),
  bridgeDomain: text(BRIDGE_DOMAIN_PATH),
  operatorFlow: text(OPERATOR_FLOW_PATH),
  camoufoxPlan: text(CAMOUFOX_PLAN_PATH),
  pythonEstate: text(PYTHON_ESTATE_PATH),
  dataClassification: text(DATA_CLASSIFICATION_PATH),
  threatModel: text(THREAT_MODEL_PATH),
  adr1: text(ADR1_PATH),
  adr2: text(ADR2_PATH),
  adr6: text(ADR6_PATH),
};

const errors = validate(overlay, operator, canonical, sources);
if (errors.length > 0) {
  for (const error of errors) {
    console.error(`ERROR: ${error}`);
  }
  process.exit(1);
}

if (process.argv.includes('--self-test')) {
  selfTest(overlay, operator, canonical, sources);
  console.log('AR-8 Camoufox/profile security authority and negative fixtures passed.');
} else {
  console.log('AR-8 Camoufox/profile security authority passed.');
}
