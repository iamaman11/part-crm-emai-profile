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
  opsctlCli: 'tools/opsctl/src/cli.rs',
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

const EXPECTED_RESERVED_OWNERS = new Map([
  ['credentials', 'AR-13'],
  ['recovery', 'AR-14'],
  ['readiness', 'AR-16'],
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

function functionSlice(source, functionName) {
  const marker = `fn ${functionName}`;
  const start = source.indexOf(marker);
  if (start < 0) throw new Error(`opsctl parser function is missing: ${functionName}`);
  const nextFunction = source.indexOf('\nfn ', start + marker.length);
  const nextTestModule = source.indexOf('\n#[cfg(test)]', start + marker.length);
  const candidates = [nextFunction, nextTestModule].filter((value) => value >= 0);
  const end = candidates.length > 0 ? Math.min(...candidates) : source.length;
  return source.slice(start, end);
}

function actionLiterals(source, functionName) {
  const body = functionSlice(source, functionName);
  const action = body.match(/let action = match action_text \{(?<body>[\s\S]*?)\n\s*\};/u);
  if (!action?.groups?.body) throw new Error(`opsctl action match is missing: ${functionName}`);
  return [...action.groups.body.matchAll(/^\s*"([a-z][a-z0-9_-]*)"\s*=>/gmu)]
    .map((match) => match[1]);
}

function readCommandLiterals(source) {
  const body = functionSlice(source, 'parse_command');
  return [...body.matchAll(/^\s*"([a-z][a-z0-9_-]*)"\s*=>\s*Ok\(/gmu)]
    .map((match) => match[1]);
}

function namespaceLiterals(source) {
  const body = functionSlice(source, 'parse_invocation');
  const namespaces = [];
  for (const match of body.matchAll(/if command == "([a-z][a-z0-9_-]*)"/gmu)) {
    namespaces.push(match[1]);
  }
  const dispatch = body.match(/match command\.as_str\(\) \{(?<body>[\s\S]*?)\n\s*_ =>/u);
  if (!dispatch?.groups?.body) throw new Error('opsctl namespaced command dispatch is missing');
  for (const match of dispatch.groups.body.matchAll(/^\s*"([a-z][a-z0-9_-]*)"\s*=>/gmu)) {
    namespaces.push(match[1]);
  }
  return namespaces;
}

function implementationCommandSet(source) {
  const namespaces = namespaceLiterals(source);
  if (new Set(namespaces).size !== namespaces.length) {
    throw new Error('opsctl parser namespace dispatch contains duplicates');
  }
  const commands = new Set(readCommandLiterals(source).map((action) => `opsctl ${action}`));
  for (const namespace of namespaces) {
    for (const action of actionLiterals(source, `parse_${namespace}_invocation`)) {
      commands.add(`opsctl ${namespace} ${action}`);
    }
  }
  return { commands, namespaces: new Set(namespaces) };
}

function validateOperatorRegistry(operator, cliSource, errors) {
  const policy = operator.command_registry_policy;
  if (!policy || typeof policy !== 'object'
      || policy.authority !== 'architecture/operator-contract.json::operator_surfaces'
      || policy.implementation !== PATHS.opsctlCli
      || policy.active_authority_to_implementation_parity_required !== true
      || policy.active_implementation_to_authority_parity_required !== true
      || policy.reserved_namespaces_must_not_parse !== true
      || policy.provider_credentials_allowed !== false
      || policy.child_process_provider_execution_allowed !== false
      || policy.network_provider_execution_allowed !== false
      || policy.database_mutation_allowed !== false
      || policy.production_mutation_allowed !== false) {
    errors.push('operator command registry policy drifted');
  }

  const surfaces = operator.operator_surfaces;
  if (!surfaces || Array.isArray(surfaces) || typeof surfaces !== 'object') {
    errors.push('operator command registry must be one object');
    return;
  }
  const registryCommands = new Set();
  const registryNamespaces = new Set();
  for (const [id, entry] of Object.entries(surfaces)) {
    if (!entry || Array.isArray(entry) || typeof entry !== 'object') {
      errors.push(`operator surface ${id} must be one object`);
      continue;
    }
    const requiredText = ['command', 'namespace', 'action', 'activation_owner', 'source', 'input_class', 'output_semantics'];
    for (const field of requiredText) {
      if (typeof entry[field] !== 'string' || entry[field].length === 0) {
        errors.push(`operator surface ${id}.${field} must be a non-empty string`);
      }
    }
    if (entry.status !== 'ACTIVE' || entry.mode !== 'READ_ONLY_METADATA_ONLY' || entry.side_effects !== 'NONE'
        || entry.network_authority !== false || entry.provider_mutation_authority !== false
        || entry.secret_readback !== false) {
      errors.push(`operator surface ${id} must remain ACTIVE/read-only/metadata-only/non-mutating`);
    }
    if (!Array.isArray(entry.parser_probe_args) || entry.parser_probe_args.length === 0
        || entry.parser_probe_args.some((value) => typeof value !== 'string' || value.length === 0)) {
      errors.push(`operator surface ${id}.parser_probe_args must contain non-empty strings`);
    }
    const expectedCommand = entry.namespace === 'root'
      ? `opsctl ${entry.action}`
      : `opsctl ${entry.namespace} ${entry.action}`;
    if (entry.command !== expectedCommand) {
      errors.push(`operator surface ${id}.command must equal ${expectedCommand}`);
    }
    if (registryCommands.has(entry.command)) errors.push(`duplicate operator command: ${entry.command}`);
    registryCommands.add(entry.command);
    if (entry.namespace !== 'root') registryNamespaces.add(entry.namespace);
  }

  let implementation;
  try {
    implementation = implementationCommandSet(cliSource);
  } catch (error) {
    errors.push(error.message);
    return;
  }
  if (registryCommands.size !== implementation.commands.size
      || [...registryCommands].some((command) => !implementation.commands.has(command))) {
    errors.push(`operator authority↔implementation command parity drifted: authority=${JSON.stringify([...registryCommands].sort())} implementation=${JSON.stringify([...implementation.commands].sort())}`);
  }
  if (registryNamespaces.size !== implementation.namespaces.size
      || [...registryNamespaces].some((namespace) => !implementation.namespaces.has(namespace))) {
    errors.push(`operator namespace parity drifted: authority=${JSON.stringify([...registryNamespaces].sort())} implementation=${JSON.stringify([...implementation.namespaces].sort())}`);
  }

  const reserved = operator.reserved_namespaces;
  if (!Array.isArray(reserved) || reserved.length !== EXPECTED_RESERVED_OWNERS.size) {
    errors.push('operator reserved namespace registry drifted');
    return;
  }
  const observedReserved = new Set();
  for (const entry of reserved) {
    if (!entry || Array.isArray(entry) || typeof entry !== 'object' || typeof entry.namespace !== 'string') {
      errors.push('operator reserved namespace entry is malformed');
      continue;
    }
    const expectedOwner = EXPECTED_RESERVED_OWNERS.get(entry.namespace);
    if (!expectedOwner || entry.activation_owner !== expectedOwner || observedReserved.has(entry.namespace)) {
      errors.push(`operator reserved namespace ownership drifted: ${entry.namespace}`);
    }
    observedReserved.add(entry.namespace);
    if (entry.status !== 'RESERVED' || !Array.isArray(entry.actions) || entry.actions.length === 0
        || entry.actions.some((action) => typeof action !== 'string' || action.length === 0)
        || !Array.isArray(entry.parser_probe_args) || entry.parser_probe_args.length === 0
        || entry.parser_probe_args.some((args) => !Array.isArray(args) || args.length === 0
          || args.some((arg) => typeof arg !== 'string' || arg.length === 0))
        || entry.provider_mutation_authority !== false || entry.network_authority !== false
        || entry.production_authorization_authority !== false) {
      errors.push(`operator reserved namespace ${entry.namespace} must remain explicit and non-mutating`);
    }
    for (const args of entry.parser_probe_args ?? []) {
      const command = `opsctl ${args.join(' ')}`;
      if (implementation.commands.has(command)) {
        errors.push(`reserved operator command became active: ${command}`);
      }
    }
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
  validateOperatorRegistry(operator, sources.opsctlCli, errors);
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
    opsctlCli: readFileSync(PATHS.opsctlCli, 'utf8'),
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

    const missingCommand = structuredClone(subjects);
    delete missingCommand.operator.operator_surfaces.promotion_verify;
    assertRejected('operator authority missing implemented command', missingCommand, sources);

    const mutableCommand = structuredClone(subjects);
    mutableCommand.operator.operator_surfaces.release_verify.network_authority = true;
    assertRejected('operator command network authority', mutableCommand, sources);

    const activatedReserved = structuredClone(subjects);
    activatedReserved.operator.reserved_namespaces[1].status = 'ACTIVE';
    assertRejected('reserved recovery activation', activatedReserved, sources);

    console.log('Subject architecture authority and opsctl command-registry negative fixtures rejected as expected.');
    return;
  }
  console.log('Subject credential lifecycle, profile security and exact opsctl command authorities are canonical and non-competing.');
}

try {
  main();
} catch (error) {
  console.error(`architecture authority check failed: ${error.message}`);
  process.exit(1);
}
