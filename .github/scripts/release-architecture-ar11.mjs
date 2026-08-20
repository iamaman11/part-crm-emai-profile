import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(scriptDir, '..', '..');
const authorityPath = path.join(root, 'architecture/release-architecture-ar11.json');
const contractPath = path.join(root, 'crates/control-plane-contract/src/lib.rs');
const workerPath = path.join(root, 'apps/control-plane-worker/src/lib.rs');
const gatePath = path.join(root, 'apps/control-plane-worker/src/capability_gate.rs');
const resolverPath = path.join(root, 'apps/mailbox-secret-resolver-worker/src/lib.rs');
const routerPath = path.join(root, 'frontend/src/app/router.tsx');
const wranglerPath = path.join(root, 'deploy/cloudflare/wrangler.jsonc');

function fail(message) {
  throw new Error(`AR-11 release architecture gate: ${message}`);
}

function loadJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
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

function stableJson(value) {
  if (Array.isArray(value)) return `[${value.map(stableJson).join(',')}]`;
  if (value && typeof value === 'object') {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${stableJson(value[key])}`).join(',')}}`;
  }
  return JSON.stringify(value);
}

function activationDigest(profile) {
  const payload = {
    profile_id: profile.profile_id,
    profile_version: profile.profile_version,
    allowed_environments: profile.allowed_environments,
    enabled_activation_units: profile.enabled_activation_units,
    disabled_activation_units: profile.disabled_activation_units,
  };
  if (profile.extends) payload.extends = profile.extends;
  return crypto.createHash('sha256').update(stableJson(payload), 'utf8').digest('hex');
}

function loadAuthority() {
  const authority = loadJson(authorityPath);
  if (authority.schema_version !== 1 || authority.kind !== 'AR11_RELEASE_ARCHITECTURE_SOURCE') fail('authority identity/schema mismatch');
  if (authority.owning_slice !== 'AR-11' || authority.owning_issue !== 372) fail('authority owner drifted');
  if (authority.canonical_projection !== 'architecture/inventory.json::release_architecture') fail('canonical inventory projection drifted');
  if (authority.production_mutation !== false || authority.architecture_complete !== false || authority.production_core_gate !== 'BLOCKED' || authority.production_ready !== false) fail('AR-11 attempted to authorize production');
  if (authority.effective_state_model?.production_enabled_is_derived !== true) fail('production_enabled must be derived');
  return authority;
}

function validateActivationUnits(authority) {
  if (!Array.isArray(authority.activation_units) || authority.activation_units.length === 0) fail('activation_units must be non-empty');
  const ids = unique(authority.activation_units, 'activation_unit', 'activation_units');
  const byId = new Map(authority.activation_units.map((unit) => [unit.activation_unit, unit]));
  for (const unit of authority.activation_units) {
    if ('production_enabled' in unit) fail(`activation unit ${unit.activation_unit} stores production_enabled`);
    if (unit.source_present !== true || unit.accepted !== true) fail(`activation unit ${unit.activation_unit} is not accepted/source-present`);
    if (!Array.isArray(unit.dependencies)) fail(`activation unit ${unit.activation_unit} dependencies must be an array`);
    for (const dependency of unit.dependencies) {
      if (!ids.has(dependency) || dependency === unit.activation_unit) fail(`invalid dependency ${unit.activation_unit} -> ${dependency}`);
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
  if (!Array.isArray(profiles) || profiles.length === 0) fail('release_profiles must be non-empty');
  unique(profiles, 'profile_id', 'release_profiles');
  const byId = new Map(profiles.map((profile) => [profile.profile_id, profile]));
  const canonicalEnvironments = new Set(['rehearsal', 'staging', 'production']);

  function effective(profileId, environment, visiting = new Set()) {
    const profile = byId.get(profileId);
    if (!profile) fail(`unknown profile ${profileId}`);
    if (!profile.allowed_environments?.includes(environment)) fail(`profile ${profileId} not allowed in ${environment}`);
    if (visiting.has(profileId)) fail(`profile inheritance cycle at ${profileId}`);
    visiting.add(profileId);
    let state = new Map([...units.keys()].map((id) => [id, false]));
    if (profile.extends) state = effective(profile.extends, environment, visiting);
    for (const id of profile.enabled_activation_units ?? []) {
      if (!units.has(id)) fail(`profile ${profileId} enables unknown ${id}`);
      state.set(id, true);
    }
    for (const id of profile.disabled_activation_units ?? []) {
      if (!units.has(id)) fail(`profile ${profileId} disables unknown ${id}`);
      state.set(id, false);
    }
    visiting.delete(profileId);
    for (const [id, enabled] of state) {
      if (!enabled) continue;
      for (const dependency of units.get(id).dependencies) {
        if (state.get(dependency) !== true) fail(`CAPABILITY_DEPENDENCY_UNSATISFIED: ${id} requires ${dependency} in ${profileId}`);
      }
    }
    return state;
  }

  for (const profile of profiles) {
    if (!Array.isArray(profile.allowed_environments) || profile.allowed_environments.length === 0) fail(`profile ${profile.profile_id} has no environment`);
    for (const environment of profile.allowed_environments) {
      if (!canonicalEnvironments.has(environment)) fail(`non-canonical environment ${environment}`);
      effective(profile.profile_id, environment);
    }
  }

  const core = effective('production-core-v1', 'production');
  for (const required of ['foundation', 'identity', 'clients', 'browser_profiles', 'profile_runtime', 'camoufox', 'notifications']) {
    if (core.get(required) !== true) fail(`production-core-v1 must enable ${required}`);
  }
  for (const forbidden of ['mailbox_admin', 'mailbox_client_binding', 'mailbox_browser_binding', 'mailbox_read', 'mailbox_jobs', 'outbound_mail']) {
    if (core.get(forbidden) !== false) fail(`production-core-v1 must disable ${forbidden}`);
  }
  const coreProfile = byId.get('production-core-v1');
  if (coreProfile.current_authorization !== 'BLOCKED' || !coreProfile.blockers?.includes('AR-15_UNSATISFIED') || !coreProfile.blockers?.includes('AR-17_NOT_ACCEPTED')) fail('production-core-v1 must remain blocked by AR-15/AR-17');
  return byId;
}

function validateProfileProjection(profiles) {
  const rust = fs.readFileSync(gatePath, 'utf8');
  const wrangler = loadJson(wranglerPath);
  const expected = new Map([...profiles].map(([id, profile]) => [id, activationDigest(profile)]));
  for (const [id, digest] of expected) {
    if (!rust.includes(`"${digest}"`)) fail(`Rust capability projection lacks semantic digest for ${id}`);
  }
  const staging = wrangler.env?.staging?.vars;
  const production = wrangler.env?.production?.vars;
  if (staging?.CANONICAL_ENVIRONMENT !== 'staging' || staging?.CAPABILITY_PROFILE_ID !== 'rehearsal-core-v1' || staging?.CAPABILITY_PROFILE_DIGEST !== expected.get('rehearsal-core-v1')) fail('staging capability overlay drifted');
  if (production?.CANONICAL_ENVIRONMENT !== 'production' || production?.CAPABILITY_PROFILE_ID !== 'production-core-v1' || production?.CAPABILITY_PROFILE_DIGEST !== expected.get('production-core-v1')) fail('production capability overlay drifted');
  for (const vars of [staging, production]) {
    for (const key of Object.keys(vars ?? {})) {
      if (/^(ENABLE_|FEATURE_|SHOW_)/.test(key)) fail(`independent capability flag forbidden: ${key}`);
    }
  }
  if (!rust.includes('ProfileAuthorization::ProductionBlocked') || !rust.includes('ProfileAuthorization::ProductionNotAuthorized')) {
    if (!rust.includes('ProductionBlocked') || !rust.includes('ProductionNotAuthorized')) fail('production runtime fail-closed authorization proof missing');
  }
}

function validateExecutionSurfaces(authority, units) {
  const surfaces = authority.execution_surfaces;
  if (!Array.isArray(surfaces) || surfaces.length === 0) fail('execution_surfaces must be non-empty');
  unique(surfaces, 'surface_id', 'execution_surfaces');
  for (const surface of surfaces) {
    if (surface.activation_unit !== 'PROFILE_PROJECTED' && !units.has(surface.activation_unit)) fail(`surface ${surface.surface_id} has unknown activation unit`);
    if (!surface.enforcement_point || !surface.disabled_behavior) fail(`surface ${surface.surface_id} lacks fail-closed semantics`);
  }

  const contract = fs.readFileSync(contractPath, 'utf8');
  const enumMatch = contract.match(/pub enum RouteClass\s*\{([\s\S]*?)\n\}/);
  if (!enumMatch) fail('cannot parse RouteClass');
  const routes = enumMatch[1].split(',').map((value) => value.trim()).filter(Boolean).filter((value) => !['DynamicRouteNotFound', 'BridgeDeniedByDefault', 'StaticAssets'].includes(value));
  const covered = new Set();
  for (const surface of surfaces.filter((item) => item.kind === 'HTTP')) {
    for (const selector of surface.selector ?? []) covered.add(selector.split(':', 1)[0]);
  }
  for (const route of routes) if (!covered.has(route)) fail(`HTTP RouteClass ${route} has no activation surface`);

  const required = new Map([
    ['queue.mailbox_jobs.consumer', ['ControlPlaneQueueMessage::MailboxJob', workerPath]],
    ['queue.integration_events.consumer', ['ControlPlaneQueueMessage::IntegrationEvent', workerPath]],
    ['schedule.mailbox_jobs.dispatcher', ['mailbox_scheduling::dispatch_pending', workerPath]],
    ['schedule.integration_events.dispatcher', ['integration_events::dispatch_pending', workerPath]],
    ['service.mailbox_secret_resolver.ingress', ['#[event(fetch', resolverPath]],
    ['schedule.mailbox_secret_resolver.reconciliation', ['#[event(scheduled)]', resolverPath]],
    ['bridge.camoufox.launch', ['Camoufox', path.join(root, 'docs/ARCHITECTURE_REBASELINE_V3_AR10.md')]],
    ['frontend.navigation_actions', ['createRouter', routerPath]],
  ]);
  const ids = new Set(surfaces.map((surface) => surface.surface_id));
  for (const [surfaceId, [marker, filePath]] of required) {
    if (!ids.has(surfaceId)) fail(`missing non-HTTP surface ${surfaceId}`);
    if (!fs.readFileSync(filePath, 'utf8').includes(marker)) fail(`surface marker missing for ${surfaceId}`);
  }
}

function validateBackendGate() {
  const worker = fs.readFileSync(workerPath, 'utf8');
  const gate = fs.readFileSync(gatePath, 'utf8');
  const requiredWorkerMarkers = [
    'capability_gate::route_enabled(&env, route, &path)',
    'ActivationUnit::MailboxJobs',
    'ActivationUnit::Notifications',
    'capability_gate::unit_enabled(env, ActivationUnit::MailboxAdmin)',
  ];
  for (const marker of requiredWorkerMarkers) if (!worker.includes(marker)) fail(`backend gate marker missing: ${marker}`);
  if (!gate.includes('RouteClass::ClientMailSendApi => Some(ActivationUnit::OutboundMail)')) fail('outbound mail route is not independently gated');
  if (!gate.includes('path.contains("/client-association")')) fail('mailbox client binding is not independently gated');
}

function validateDeploymentClosures(authority) {
  const closures = authority.deployment_closures;
  unique(closures, 'closure_id', 'deployment_closures');
  const core = closures.find((closure) => closure.closure_id === 'production-core-v1');
  if (!core) fail('production-core-v1 deployment closure missing');
  for (const forbidden of ['MAILBOX_JOBS', 'MAILBOX_SECRET_RESOLVER']) if (core.required_bindings.includes(forbidden)) fail(`core requires disabled mail binding ${forbidden}`);
  if (core.required_credentials.includes('MAILBOX_RESOLVER_CALLER_AUTH_KEY')) fail('core requires mailbox resolver credential');

  const wrangler = fs.readFileSync(wranglerPath, 'utf8');
  for (const forbidden of ['"MAILBOX_JOBS"', '"MAILBOX_SECRET_RESOLVER"', '"MAILBOX_RESOLVER_CALLER_AUTH_KEY"']) {
    if (wrangler.includes(forbidden)) fail(`core Wrangler overlay still contains disabled mail dependency ${forbidden}`);
  }
}

function validateReleasePromotion(authority) {
  if (authority.release_set?.schema_version !== 2
      || authority.release_set?.id_scheme !== 'release-set-v2-sha256-SHA256(canonical_identity_payload)'
      || authority.release_set?.dependency_graph !== 'ACYCLIC_REQUIRED'
      || authority.release_set?.unknown_result !== 'FAIL_CLOSED') fail('release-set v2 invariants incomplete');
  const policy = authority.promotion_policy;
  if (policy?.build_once !== true || policy?.promotion_rebuild !== false || policy?.opsctl_network !== false || policy?.opsctl_credentials !== false || policy?.opsctl_provider_mutation !== false || policy?.production_execution_before_ar17 !== false) fail('promotion authority boundary drifted');
  if (authority.artifact_authority?.canonical_durable_publication !== 'GITHUB_RELEASE_ASSETS' || authority.artifact_authority?.overwrite_existing_release_id !== false) fail('durable artifact authority incomplete');
}

function selfTest(authority) {
  const copy = structuredClone(authority);
  copy.activation_units.find((unit) => unit.activation_unit === 'outbound_mail').dependencies = ['missing_dependency'];
  let rejected = false;
  try { validateActivationUnits(copy); } catch { rejected = true; }
  if (!rejected) fail('unknown dependency negative fixture was accepted');

  const copy2 = structuredClone(authority);
  const core = copy2.release_profiles.find((profile) => profile.profile_id === 'production-core-v1');
  core.enabled_activation_units.push('outbound_mail');
  core.disabled_activation_units = core.disabled_activation_units.filter((id) => id !== 'outbound_mail');
  rejected = false;
  try { validateProfiles(copy2, validateActivationUnits(copy2)); } catch { rejected = true; }
  if (!rejected) fail('production Core outbound-mail negative fixture was accepted');

  const copy3 = structuredClone(authority);
  copy3.release_set.schema_version = 1;
  copy3.release_set.id_scheme = 'release-set-v1-sha256-SHA256(canonical_identity_payload)';
  rejected = false;
  try { validateReleasePromotion(copy3); } catch { rejected = true; }
  if (!rejected) fail('Release Set v1 regression fixture was accepted');
}

const authority = loadAuthority();
const units = validateActivationUnits(authority);
const profiles = validateProfiles(authority, units);
validateProfileProjection(profiles);
validateExecutionSurfaces(authority, units);
validateBackendGate();
validateDeploymentClosures(authority);
validateReleasePromotion(authority);

if (process.argv.includes('--self-test')) {
  selfTest(authority);
  console.log('AR-11 release architecture negative self-test passed.');
} else {
  console.log(`AR-11 release architecture valid: ${units.size} activation units, ${profiles.size} profiles, ${authority.execution_surfaces.length} execution surfaces.`);
}
