#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const SOURCES = {
  catalog: 'scripts/cloudflare-release.py',
  resolver: 'scripts/mailbox-secret-resolver-release.py',
  adapter: 'scripts/d1_repository_projection.py',
  releaseD1: 'tools/opsctl/src/d1.rs',
  releaseFinalize: 'tools/opsctl/src/release/finalize/mod.rs',
  releaseV3Dto: 'tools/opsctl/src/release/v3_dto.rs',
  releaseV3Output: 'tools/opsctl/src/release/v3_output.rs',
  releaseArchitecture: 'architecture/release-architecture-ar11.json',
};

function fail(message) {
  throw new Error(message);
}

function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value && typeof value === 'object') {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`).join(',')}}`;
  }
  return JSON.stringify(value);
}

function identity(value) {
  return createHash('sha256').update(canonical(value)).digest('hex');
}

function requireMarkers(text, label, markers) {
  for (const marker of markers) {
    if (!text.includes(marker)) fail(`${label} lost required marker: ${marker}`);
  }
}

function forbidMarkers(text, label, markers) {
  for (const marker of markers) {
    if (text.includes(marker)) fail(`${label} retains forbidden marker: ${marker}`);
  }
}

function validateReleaseBuilder(text, component) {
  requireMarkers(text, `${component} release builder`, [
    'import d1_repository_projection as d1_repository',
    `return d1_repository.release_contract(root, "${component}")`,
    '"schema_contract": load_schema_contract(root)',
  ]);
  forbidMarkers(text, `${component} release builder`, [
    ['d1-evolution-', 'ar9.json'].join(''),
    ['D1_EVOLUTION_', 'AUTHORITY'].join(''),
    'evolution_history_identity',
  ]);
}

function validateCurrentReleaseSet(inputs) {
  requireMarkers(inputs.releaseD1, 'typed D1 Release Set adapter', [
    'pub(crate) use catalog::{release_contract, repository_identity_sha256};',
    'pub(crate) fn release_schema_identity(',
    'compatibility_policy_digest: authority.policy_digest',
  ]);

  requireMarkers(inputs.releaseFinalize, 'Release Set v3 finalize composition', [
    'let catalog = d1_schema_window(root, "catalog")?;',
    'let resolver = d1_schema_window(root, "resolver")?;',
    'let d1_repository_identity_sha256 = d1::repository_identity_sha256(root)',
    'schemas: core::SchemaIdentity {',
    'd1_repository_identity_sha256,',
    'catalog,',
    'resolver,',
    'let rendered = render_release_set_v3(&release_set)',
  ]);

  requireMarkers(inputs.releaseV3Dto, 'Release Set v3 DTO', [
    'pub schemas: SchemaIdentityDto,',
    'pub d1_repository_identity_sha256: String,',
    'pub catalog: SchemaCompatibilityWindowDto,',
    'pub resolver: SchemaCompatibilityWindowDto,',
    'pub compatibility_policy_digest: String,',
  ]);

  requireMarkers(inputs.releaseV3Output, 'Release Set v3 output adapter', [
    'let identity = ReleaseSetV3Dto::from(release_set);',
    'let canonical_identity_bytes = canonical_bytes(&identity, "Release Set v3 identity")?;',
    'sha256_hex(&canonical_identity_bytes)',
  ]);

  const removedAuthority = ['d1_evolution_', 'authority'].join('');
  for (const [label, text] of [
    ['typed D1 Release Set adapter', inputs.releaseD1],
    ['Release Set v3 finalize composition', inputs.releaseFinalize],
    ['Release Set v3 DTO', inputs.releaseV3Dto],
    ['Release Set v3 output adapter', inputs.releaseV3Output],
  ]) {
    forbidMarkers(text, label, [removedAuthority, ['D1_EVOLUTION_', 'AUTHORITY'].join('')]);
  }
}

function validateAdapter(text) {
  requireMarkers(text, 'D1 outer adapter', [
    '"d1",',
    '"repository",',
    '"D1_REPOSITORY_PROJECTION"',
    'repository_identity_sha256',
  ]);
  forbidMarkers(text, 'D1 outer adapter', [
    'migration_history_digest(',
    'compatibility_policy =',
    ['D1_EVOLUTION_', 'AUTHORITY'].join(''),
  ]);
}

function validateReleaseArchitecture(value) {
  if (!Array.isArray(value.release_inputs)) fail('release architecture release_inputs is missing');
  const removedInput = ['d1_evolution_', 'authority'].join('');
  if (value.release_inputs.some((entry) => entry?.input_id === removedInput)) {
    fail('release architecture retains the removed D1 authority file input');
  }
}

function proveIdentityCoverage() {
  const payload = {
    binary: 'a'.repeat(64),
    schemas: {
      d1_repository_identity_sha256: 'b'.repeat(64),
      compatibility_policy_digest: 'c'.repeat(64),
    },
  };
  const changed = structuredClone(payload);
  changed.schemas.compatibility_policy_digest = 'd'.repeat(64);
  if (identity(payload) === identity(changed)) fail('D1 policy change did not alter release identity');
  changed.schemas.compatibility_policy_digest = payload.schemas.compatibility_policy_digest;
  changed.schemas.d1_repository_identity_sha256 = 'e'.repeat(64);
  if (identity(payload) === identity(changed)) fail('D1 repository change did not alter release identity');
}

async function loadInputs() {
  const loaded = await Promise.all(
    Object.entries(SOURCES).map(async ([key, relative]) => [key, await readFile(path.join(ROOT, relative), 'utf8')]),
  );
  const inputs = Object.fromEntries(loaded);
  inputs.releaseArchitecture = JSON.parse(inputs.releaseArchitecture);
  return inputs;
}

function validateInputs(inputs) {
  validateReleaseBuilder(inputs.catalog, 'catalog');
  validateReleaseBuilder(inputs.resolver, 'resolver');
  validateAdapter(inputs.adapter);
  validateCurrentReleaseSet(inputs);
  validateReleaseArchitecture(inputs.releaseArchitecture);
  proveIdentityCoverage();
}

async function validate() {
  validateInputs(await loadInputs());
}

function expectValidationFailure(label, inputs) {
  try {
    validateInputs(inputs);
  } catch {
    return;
  }
  fail(`${label} negative fixture unexpectedly passed`);
}

async function selfTest() {
  const inputs = await loadInputs();
  validateInputs(inputs);

  expectValidationFailure('detached typed D1 repository identity', {
    ...inputs,
    releaseFinalize: inputs.releaseFinalize.replace(
      'let d1_repository_identity_sha256 = d1::repository_identity_sha256(root)',
      'let d1_repository_identity_sha256 = detached_repository_identity(root)',
    ),
  });

  expectValidationFailure('detached Release Set v3 schema DTO', {
    ...inputs,
    releaseV3Dto: inputs.releaseV3Dto.replace(
      'pub d1_repository_identity_sha256: String,',
      'pub detached_schema_identity: String,',
    ),
  });

  expectValidationFailure('detached Release Set v3 content-address scope', {
    ...inputs,
    releaseV3Output: inputs.releaseV3Output.replace(
      'let canonical_identity_bytes = canonical_bytes(&identity, "Release Set v3 identity")?;',
      'let canonical_identity_bytes = b"detached".to_vec();',
    ),
  });

  console.log('D1 release schema-identity ownership and negative fixtures passed.');
}

async function main() {
  if (process.argv.includes('--self-test')) {
    await selfTest();
    return;
  }
  if (process.argv.length > 2) fail(`unknown arguments: ${process.argv.slice(2).join(' ')}`);
  await validate();
  console.log(
    'Catalog and Resolver identities consume the canonical D1 projection; Release Set v3 consumes typed Rust D1 identity through pure core and its content-addressed DTO.',
  );
}

main().catch((error) => {
  console.error(`D1 release schema identity error: ${error instanceof Error ? error.message : error}`);
  process.exitCode = 1;
});
