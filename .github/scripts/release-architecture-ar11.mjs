import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';

const root = path.resolve(new URL('../..', import.meta.url).pathname);
const authorityPath = path.join(root, 'architecture/release-architecture-ar11.json');
const contractPath = path.join(root, 'crates/control-plane-contract/src/lib.rs');
const workerPath = path.join(root, 'apps/control-plane-worker/src/lib.rs');
const resolverPath = path.join(root, 'apps/mailbox-secret-resolver-worker/src/lib.rs');
const wranglerPath = path.join(root, 'deploy/cloudflare/wrangler.jsonc');

function fail(message) {
  throw new Error(`AR-11 release architecture gate: ${message}`);
}

function loadAuthority() {
  const value = JSON.parse(fs.readFileSync(authorityPath, 'utf8'));
  if (value.schema_version !== 1 || value.kind !== 'AR11_RELEASE_ARCHITECTURE_SOURCE') {
    fail('authority identity/schema mismatch');
  }
  if (value.owning_slice !== 'AR-11' || value.owning_issue !== 372) {
    fail('authority owner drifted');
  }
  if (value.canonical_projection !== 'architecture/inventory.json::release_architecture') {
    fail('canonical inventory projection drifted');
  }
  if (
    value.production_mutation !== false ||
    value.architecture_complete !== false ||
    value.production_core_gate !== 'BLOCKED' ||
    value.production_ready !== false
  ) {
    fail('AR-11 attempted to authorize production');
  }
  if (value.effective_state_model?.production_enabled_is_derived !== true) {
    fail('production_enabled must be derived from profile/environment/gates');
  }
  return value;
}

function uniqueBy(items, key, label) {
  const seen = new Set();
  for (const item of items) {
    const value = item?.[key];
    if (typeof value !== 'string' || value.length === 0) {
      fail(`${label} has invalid ${key}`);
    }
    if (seen.has(value)) {
      fail(`${label} duplicates ${value}`);
    }
    seen.add(value);
  }
  return seen;
}

function validateActivationUnits(authority) {
  const units = authority.activation_units;
  if (!Array.isArray(units) || units.length === 0) {
    fail('activation_units must be non-empty');
  }
  const ids = uniqueBy(units, 'activation_unit', 'activation_units');
  const byId = new Map(units.map((unit) => [unit.activation_unit, unit]));
  for (const unit of units) {
    if ('production_enabled' in unit) {
      fail(`activation unit ${unit.activation_unit} stores mutable production_enabled`);
    }
    if (unit.source_present !== true || unit.accepted !== true) {
      fail(`current AR-11 unit ${unit.activation_unit} must be accepted/source-present`);
    }
    if (!Array.isArray(unit.dependencies)) {
      fail(`activation unit ${unit.activation_unit} dependencies must be an array`);
    }
    for (const dependency of unit.dependencies) {
      if (!ids.has(dependency) || dependency === unit.activation_unit) {
        fail(`invalid activation dependency ${unit.activation_unit} -> ${dependency}`);
      }
    }
  }

  const visiting = new Set();
  const visited = new Set();
  function visit(id) {
    if (visited.has(id)) return;
    if (visiting.has(id)) fail(`activation dependency cycle at ${id}`);
    visiting.add(id);
    for (const dependency of byId.get(id).dependencies) visit(dependency);
    visiting.delete(id);
    visited.add(id);
  }
  for (const id of ids) visit(id);
  return byId;
}

function validateProfiles(authority, units) {
  const profiles = authority.release_profiles;
  if (!Array.isArray(profiles) || profiles.length === 0) {
    fail('release_profiles must be non-empty');
  }
  uniqueBy(profiles, 'profile_id', 'release_profiles');
  const byId = new Map(profiles.map((profile) => [profile.profile_id, profile]));
  const canonicalEnvironments = new Set(['rehearsal', 'staging', 'production']);

  function effective(profileId, environment, visiting = new Set()) {
    const profile = byId.get(profileId);
    if (!profile) fail(`unknown profile ${profileId}`);
    if (!profile.allowed_environments?.includes(environment)) {
      fail(`profile ${profileId} is not allowed in ${environment}`);
    }
    if (visiting.has(profileId)) fail(`profile inheritance cycle at ${profileId}`);
    visiting.add(profileId);
    let state = new Map([...units.keys()].map((id) => [id, false]));
    if (profile.extends) {
      state = effective(profile.extends, environment, visiting);
    }
    for (const id of profile.enabled_activation_units ?? []) {
      if (!units.has(id)) fail(`profile ${profileId} enables unknown unit ${id}`);
      state.set(id, true);
    }
    for (const id of profile.disabled_activation_units ?? []) {
      if (!units.has(id)) fail(`profile ${profileId} disables unknown unit ${id}`);
      state.set(id, false);
    }
    visiting.delete(profileId);
    for (const [id, enabled] of state) {
      if (!enabled) continue;
      for (const dependency of units.get(id).dependencies) {
        if (state.get(dependency) !== true) {
          fail(`CAPABILITY_DEPENDENCY_UNSATISFIED: ${id} requires ${dependency} in ${profileId}`);
        }
      }
    }
    return state;
  }

  for (const profile of profiles) {
    if (!Array.isArray(profile.allowed_environments) || profile.allowed_environments.length === 0) {
      fail(`profile ${profile.profile_id} has no allowed environment`);
    }
    for (const environment of profile.allowed_environments) {
      if (!canonicalEnvironments.has(environment)) {
        fail(`profile ${profile.profile_id} uses non-canonical environment ${environment}`);
      }
      effective(profile.profile_id, environment);
    }
  }

  const core = effective('production-core-v1', 'production');
  for (const required of ['foundation', 'identity', 'clients', 'browser_profiles', 'profile_runtime', 'camoufox']) {
    if (core.get(required) !== true) fail(`production-core-v1 must enable ${required}`);
  }
  for (const forbidden of [
    'mailbox_admin',
    'mailbox_client_binding',
    'mailbox_browser_binding',
    'mailbox_read',
    'mailbox_jobs',
    'outbound_mail',
  ]) {
    if (core.get(forbidden) !== false) fail(`production-core-v1 must disable ${forbidden}`);
  }
  const coreProfile = byId.get('production-core-v1');
  if (
    coreProfile.current_authorization !== 'BLOCKED' ||
    !coreProfile.blockers?.includes('AR-15_UNSATISFIED') ||
    !coreProfile.blockers?.includes('AR-17_NOT_ACCEPTED')
  ) {
    fail('production-core-v1 must remain blocked by Windows delivery and AR-17');
  }
}

function validateExecutionSurfaceCoverage(authority, units) {
  const surfaces = authority.execution_surfaces;
  if (!Array.isArray(surfaces) || surfaces.length === 0) {
    fail('execution_surfaces must be non-empty');
  }
  uniqueBy(surfaces, 'surface_id', 'execution_surfaces');
  for (const surface of surfaces) {
    if (
      surface.activation_unit !== 'PROFILE_PROJECTED' &&
      !units.has(surface.activation_unit)
    ) {
      fail(`surface ${surface.surface_id} references unknown activation unit`);
    }
    if (!surface.enforcement_point || !surface.disabled_behavior) {
      fail(`surface ${surface.surface_id} lacks enforcement/disabled behavior`);
    }
  }

  const contract = fs.readFileSync(contractPath, 'utf8');
  const enumMatch = contract.match(/pub enum RouteClass\s*\{([\s\S]*?)\n\}/);
  if (!enumMatch) fail('cannot parse RouteClass enum');
  const routeClasses = enumMatch[1]
    .split(',')
    .map((value) => value.trim())
    .filter(Boolean)
    .filter((value) => !['DynamicRouteNotFound', 'BridgeDeniedByDefault', 'StaticAssets'].includes(value));
  const covered = new Set();
  for (const surface of surfaces.filter((item) => item.kind === 'HTTP')) {
    for (const selector of surface.selector ?? []) {
      covered.add(selector.split(':', 1)[0]);
    }
  }
  for (const route of routeClasses) {
    if (!covered.has(route)) fail(`HTTP RouteClass ${route} has no activation surface`);
  }

  const requiredSurfaces = new Map([
    ['queue.mailbox_jobs.consumer', ['ControlPlaneQueueMessage::MailboxJob', workerPath]],
    ['queue.integration_events.consumer', ['ControlPlaneQueueMessage::IntegrationEvent', workerPath]],
    ['schedule.mailbox_jobs.dispatcher', ['mailbox_scheduling::dispatch_pending', workerPath]],
    ['schedule.integration_events.dispatcher', ['integration_events::dispatch_pending', workerPath]],
    ['service.mailbox_secret_resolver.ingress', ['#[event(fetch', resolverPath]],
    ['schedule.mailbox_secret_resolver.reconciliation', ['#[event(scheduled)]', resolverPath]],
    ['bridge.camoufox.launch', ['Camoufox', path.join(root, 'docs/ARCHITECTURE_REBASELINE_V3_AR10.md')]],
    ['frontend.navigation_actions', ['createRouter', path.join(root, 'frontend/src/app/router.tsx')]],
  ]);
  const ids = new Set(surfaces.map((surface) => surface.surface_id));
  for (const [surfaceId, [marker, filePath]] of requiredSurfaces) {
    if (!ids.has(surfaceId)) fail(`missing non-HTTP surface ${surfaceId}`);
    if (!fs.readFileSync(filePath, 'utf8').includes(marker)) {
      fail(`surface ${surfaceId} marker not found in ${path.relative(root, filePath)}`);
    }
  }
}

function validateDeploymentClosures(authority) {
  const closures = authority.deployment_closures;
  uniqueBy(closures, 'closure_id', 'deployment_closures');
  const core = closures.find((closure) => closure.closure_id === 'production-core-v1');
  if (!core) fail('production-core-v1 deployment closure missing');
  for (const forbidden of ['MAILBOX_JOBS', 'MAILBOX_SECRET_RESOLVER']) {
    if (core.required_bindings.includes(forbidden)) {
      fail(`production core incorrectly requires disabled mail binding ${forbidden}`);
    }
  }
  if (core.required_credentials.includes('MAILBOX_RESOLVER_CALLER_AUTH_KEY')) {
    fail('production core incorrectly requires mailbox resolver credential');
  }
  for (const requiredDisabledResource of [
    'mailbox_jobs',
    'mailbox_jobs_dlq',
    'mailbox_secret_resolver_worker',
    'resolver_d1',
    'mailbox_secret_resolver_service',
  ]) {
    if (!core.optional_or_disabled_resources.includes(requiredDisabledResource)) {
      fail(`production core does not classify disabled resource ${requiredDisabledResource}`);
    }
  }

  const wrangler = fs.readFileSync(wranglerPath, 'utf8');
  if (!wrangler.includes('"MAILBOX_JOBS"') || !wrangler.includes('"MAILBOX_SECRET_RESOLVER"')) {
    fail('expected pre-AR-11 operational coupling disappeared without authority update');
  }
}

function validateReleaseAndPromotion(authority) {
  if (
    authority.release_set?.schema_version !== 1 ||
    authority.release_set?.dependency_graph !== 'ACYCLIC_REQUIRED' ||
    authority.release_set?.unknown_result !== 'FAIL_CLOSED'
  ) {
    fail('release-set compatibility invariants are incomplete');
  }
  const policy = authority.promotion_policy;
  if (
    policy?.build_once !== true ||
    policy?.promotion_rebuild !== false ||
    policy?.opsctl_network !== false ||
    policy?.opsctl_credentials !== false ||
    policy?.opsctl_provider_mutation !== false ||
    policy?.production_execution_before_ar17 !== false
  ) {
    fail('promotion authority boundary drifted');
  }
  if (
    authority.artifact_authority?.canonical_durable_publication !== 'GITHUB_RELEASE_ASSETS' ||
    authority.artifact_authority?.overwrite_existing_release_id !== false
  ) {
    fail('durable artifact authority is incomplete');
  }
}

function selfTest(authority) {
  const copy = structuredClone(authority);
  const outbound = copy.activation_units.find((unit) => unit.activation_unit === 'outbound_mail');
  outbound.dependencies = ['missing_dependency'];
  let detected = false;
  try {
    validateActivationUnits(copy);
  } catch {
    detected = true;
  }
  if (!detected) fail('negative fixture failed to detect unknown dependency');

  const copy2 = structuredClone(authority);
  const core = copy2.release_profiles.find((profile) => profile.profile_id === 'production-core-v1');
  core.enabled_activation_units.push('outbound_mail');
  core.disabled_activation_units = core.disabled_activation_units.filter((id) => id !== 'outbound_mail');
  detected = false;
  try {
    const units = validateActivationUnits(copy2);
    validateProfiles(copy2, units);
  } catch {
    detected = true;
  }
  if (!detected) fail('negative fixture failed to detect Core outbound mail activation');
}

const authority = loadAuthority();
const units = validateActivationUnits(authority);
validateProfiles(authority, units);
validateExecutionSurfaceCoverage(authority, units);
validateDeploymentClosures(authority);
validateReleaseAndPromotion(authority);

if (process.argv.includes('--self-test')) {
  selfTest(authority);
  console.log('AR-11 release architecture negative self-test passed.');
} else {
  console.log(
    `AR-11 release architecture valid: ${units.size} activation units, ` +
      `${authority.release_profiles.length} profiles, ${authority.execution_surfaces.length} execution surfaces.`
  );
}
