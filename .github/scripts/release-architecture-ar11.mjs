import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(scriptDir, '..', '..');
const authorityPath = path.join(root, 'architecture/release-architecture-ar11.json');
const wranglerPath = path.join(root, 'deploy/cloudflare/wrangler.jsonc');
const releaseBuildWorkflowPath = path.join(root, '.github/workflows/release-set-build.yml');

function fail(message) {
  throw new Error(`AR-11 release architecture gate: ${message}`);
}

function loadJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function requireArray(value, label) {
  if (!Array.isArray(value) || value.length === 0) fail(`${label} must be a non-empty array`);
  return value;
}

function unique(items, key, label) {
  const seen = new Set();
  for (const item of items) {
    const value = item?.[key];
    if (typeof value !== 'string' || value.length === 0) fail(`${label} has invalid ${key}`);
    if (seen.has(value)) fail(`${label} duplicates ${value}`);
    seen.add(value);
  }
  return seen;
}

function validateAuthorityOwnership(authority) {
  for (const forbidden of [
    'release_set',
    'activation_units',
    'release_profiles',
    'execution_surfaces',
    'effective_state_model',
  ]) {
    if (forbidden in authority) fail(`${forbidden} must not be owned by AR-11 release architecture JSON`);
  }
}

function validateCapabilityProjectionReference(authority) {
  const projection = authority.capability_policy_projection;
  if (!projection || typeof projection !== 'object') fail('capability policy projection reference is missing');
  if (projection.semantic_owner !== 'crates/capability-policy') fail('capability policy semantic owner drifted');
  if (projection.typed_snapshot !== 'capability-policy::snapshot_v1') fail('typed capability snapshot reference drifted');
  if (projection.generated_manifest !== 'capability-policy-v1.json') fail('generated capability manifest path drifted');
  if (projection.generated_manifest_kind !== 'capability-policy') fail('generated capability manifest kind drifted');
  if (projection.generated_manifest_role !== 'IMMUTABLE_RELEASE_SET_PROJECTION_ONLY') fail('generated capability manifest role drifted');
  if (projection.manifest_semantic_input !== false || projection.runtime_authorization_from_manifest !== false) {
    fail('generated capability manifest must remain evidence-only and non-authoritative');
  }
}

function validateAuthority(authority) {
  if (authority.schema_version !== 1 || authority.kind !== 'AR11_RELEASE_ARCHITECTURE_SOURCE') {
    fail('authority identity/schema mismatch');
  }
  if (authority.owning_slice !== 'AR-11' || authority.owning_issue !== 372) fail('authority owner drifted');
  if (authority.authority_role !== 'NATURAL_OWNER') fail('release architecture ownership drifted');
  if (
    authority.production_mutation !== false
    || authority.architecture_complete !== false
    || authority.production_core_gate !== 'BLOCKED'
    || authority.production_ready !== false
  ) {
    fail('AR-11 attempted to authorize production');
  }
  if (authority.principles?.source_present_not_equal_production_enabled !== true) {
    fail('source_present != production_enabled principle drifted');
  }
  if (authority.principles?.backend_security_boundary !== true || authority.principles?.frontend_projection_only !== true) {
    fail('backend/frontend authority boundary drifted');
  }
  validateAuthorityOwnership(authority);
  validateCapabilityProjectionReference(authority);
}

function validateDeploymentClosures(authority) {
  const closures = requireArray(authority.deployment_closures, 'deployment_closures');
  const closureIds = unique(closures, 'closure_id', 'deployment_closures');
  for (const closure of closures) {
    for (const field of ['required_components', 'required_resources', 'required_bindings', 'required_credentials']) {
      if (!Array.isArray(closure[field])) fail(`deployment closure ${closure.closure_id} lacks ${field}`);
    }
    if (closure.extends && !closureIds.has(closure.extends)) {
      fail(`deployment closure ${closure.closure_id} extends unknown closure ${closure.extends}`);
    }
    if (typeof closure.profile_id !== 'string' || closure.profile_id.length === 0) {
      fail(`deployment closure ${closure.closure_id} lacks typed capability profile reference`);
    }
  }

  for (const profileId of [
    'rehearsal-core-v1',
    'production-core-v1',
    'rehearsal-core-v2',
    'production-core-v2',
  ]) {
    if (!closures.some((closure) => closure.closure_id === profileId)) {
      fail(`${profileId} deployment closure missing`);
    }
  }

  const core = closures.find((closure) => closure.closure_id === 'production-core-v2');
  if (!core) fail('production-core-v2 deployment closure missing');
  for (const forbidden of ['MAILBOX_JOBS', 'MAILBOX_SECRET_RESOLVER']) {
    if (core.required_bindings.includes(forbidden)) fail(`core requires disabled mail binding ${forbidden}`);
  }
  if (core.required_credentials.includes('MAILBOX_RESOLVER_CALLER_AUTH_KEY')) {
    fail('core requires mailbox resolver credential');
  }

  const wrangler = fs.readFileSync(wranglerPath, 'utf8');
  for (const forbidden of ['"MAILBOX_JOBS"', '"MAILBOX_SECRET_RESOLVER"', '"MAILBOX_RESOLVER_CALLER_AUTH_KEY"']) {
    if (wrangler.includes(forbidden)) fail(`core Wrangler overlay still contains disabled mail dependency ${forbidden}`);
  }
  for (const currentProfile of ['rehearsal-core-v2', 'production-core-v2']) {
    if (!wrangler.includes(`"CAPABILITY_PROFILE_ID": "${currentProfile}"`)) {
      fail(`Core Wrangler overlay does not select ${currentProfile}`);
    }
  }
  for (const historicalProfile of ['rehearsal-core-v1', 'production-core-v1']) {
    if (wrangler.includes(`"CAPABILITY_PROFILE_ID": "${historicalProfile}"`)) {
      fail(`historical profile remains a current Wrangler selector: ${historicalProfile}`);
    }
  }
}

function validateReleasePromotion(authority) {
  const policy = authority.promotion_policy;
  if (
    policy?.build_once !== true
    || policy?.promotion_rebuild !== false
    || policy?.environment_overlay_may_change_application_bits !== false
    || policy?.opsctl_network !== false
    || policy?.opsctl_credentials !== false
    || policy?.opsctl_provider_mutation !== false
    || policy?.production_execution_before_ar17 !== false
  ) {
    fail('promotion authority boundary drifted');
  }
  const artifacts = authority.artifact_authority;
  if (
    artifacts?.canonical_durable_publication !== 'GITHUB_RELEASE_ASSETS'
    || artifacts?.overwrite_existing_release_id !== false
    || artifacts?.same_id_republish_policy !== 'VERIFY_BYTE_EQUALITY_OR_FATAL'
  ) {
    fail('durable artifact authority incomplete');
  }
}

function validateReleaseInputs(authority) {
  const inputs = requireArray(authority.release_inputs, 'release_inputs');
  unique(inputs, 'input_id', 'release_inputs');
  const identitySources = new Set();
  for (const input of inputs) {
    if (typeof input.semantic_owner !== 'string' || input.semantic_owner.length === 0) {
      fail(`release input ${input.input_id} lacks semantic owner`);
    }
    const hasCanonical = typeof input.canonical_source === 'string' && input.canonical_source.length > 0;
    const hasGenerated = typeof input.generated_projection === 'string' && input.generated_projection.length > 0;
    if (hasCanonical === hasGenerated) fail(`release input ${input.input_id} must select one source form`);
    const selected = hasCanonical ? input.canonical_source : input.generated_projection;
    if (input.release_identity_source !== selected) fail(`release input ${input.input_id} identity source drifted`);
    if (identitySources.has(input.release_identity_source)) fail(`duplicate release identity source ${input.release_identity_source}`);
    identitySources.add(input.release_identity_source);
    if (input.required_for_release_set !== true) fail(`release input ${input.input_id} is not bound to Release Set identity`);
    if (!Array.isArray(input.verification) || input.verification.length === 0) fail(`release input ${input.input_id} lacks verification`);
    if (!Array.isArray(input.consumers) || input.consumers.length === 0) fail(`release input ${input.input_id} lacks consumers`);
    if (hasGenerated && (typeof input.generator !== 'string' || input.generator.length === 0)) {
      fail(`generated release input ${input.input_id} lacks deterministic generator`);
    }
    if (!hasGenerated && 'generator' in input) fail(`canonical release input ${input.input_id} must not declare generator`);
  }

  const architectureInput = inputs.find((input) => input.input_id === 'release_architecture_authority');
  if (!architectureInput) fail('release architecture build-provenance input missing');
  if (architectureInput.semantic_owner.includes('capability')) {
    fail('AR-11 release input must not claim capability semantics');
  }
}

function validateComponentOwners(authority) {
  const owners = requireArray(authority.component_release_owners, 'component_release_owners');
  unique(owners, 'component_id', 'component_release_owners');
  for (const component of owners) {
    if (typeof component.owner !== 'string' || component.owner.length === 0 || component.build_once !== true) {
      fail(`component release owner ${component.component_id} is incomplete`);
    }
  }
}

function validatePublicationWiring(authority) {
  const workflow = fs.readFileSync(releaseBuildWorkflowPath, 'utf8');
  for (const marker of [
    'capability-policy-manifest',
    'capability-policy-v1.json',
    'capability_policy_sha=',
    'capability_policy_size=',
    '.path == "capability-policy-v1.json"',
    '.kind == "capability-policy"',
    'cp "$capability_policy_json" "$release_dir/capability-policy-v1.json"',
    'cp "$RELEASE_DIR/capability-policy-v1.json" "$asset_dir/capability-policy-v1.json"',
  ]) {
    if (!workflow.includes(marker)) fail(`Release Set build workflow lacks capability manifest wiring: ${marker}`);
  }
  if (authority.capability_policy_projection.generated_manifest !== 'capability-policy-v1.json') {
    fail('release workflow and architecture projection path disagree');
  }
}

function selfTest(authority) {
  const duplicateOwner = structuredClone(authority);
  duplicateOwner.activation_units = [];
  let rejected = false;
  try {
    validateAuthority(duplicateOwner);
  } catch {
    rejected = true;
  }
  if (!rejected) fail('duplicate capability semantic authority fixture was accepted');

  const semanticInput = structuredClone(authority);
  semanticInput.capability_policy_projection.manifest_semantic_input = true;
  rejected = false;
  try {
    validateAuthority(semanticInput);
  } catch {
    rejected = true;
  }
  if (!rejected) fail('manifest-as-semantic-input fixture was accepted');

  const forbiddenBinding = structuredClone(authority);
  forbiddenBinding.deployment_closures
    .find((closure) => closure.closure_id === 'production-core-v2')
    .required_bindings.push('MAILBOX_JOBS');
  rejected = false;
  try {
    validateDeploymentClosures(forbiddenBinding);
  } catch {
    rejected = true;
  }
  if (!rejected) fail('forbidden core provider binding fixture was accepted');
}

const authority = loadJson(authorityPath);
validateAuthority(authority);
validateDeploymentClosures(authority);
validateReleasePromotion(authority);
validateReleaseInputs(authority);
validateComponentOwners(authority);
validatePublicationWiring(authority);

if (process.argv.includes('--self-test')) {
  selfTest(authority);
  console.log('AR-11 release architecture negative self-test passed.');
} else {
  console.log(
    `AR-11 release architecture valid: ${authority.deployment_closures.length} deployment closures, ${authority.release_inputs.length} release inputs; capability semantics owned by crates/capability-policy.`,
  );
}
