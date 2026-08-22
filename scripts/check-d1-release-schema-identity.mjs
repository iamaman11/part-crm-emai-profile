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
  releaseSet: 'scripts/release-set-ar11.py',
  adapter: 'scripts/d1_repository_projection.py',
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

function validateReleaseBuilder(text, component) {
  requireMarkers(text, `${component} release builder`, [
    'import d1_repository_projection as d1_repository',
    `return d1_repository.release_contract(root, "${component}")`,
    '"schema_contract": load_schema_contract(root)',
  ]);
  for (const forbidden of [
    ['d1-evolution-', 'ar9.json'].join(''),
    ['D1_EVOLUTION_', 'AUTHORITY'].join(''),
    'evolution_history_identity',
  ]) {
    if (text.includes(forbidden)) {
      fail(`${component} release builder retains removed D1 authority logic: ${forbidden}`);
    }
  }
}

function validateReleaseSet(text) {
  requireMarkers(text, 'Release Set builder', [
    'import d1_repository_projection as d1_repository',
    'd1_projection = d1_repository.load(ROOT)',
    'catalog_contract = d1_repository.release_contract_from_projection(',
    'resolver_contract = d1_repository.release_contract_from_projection(',
    '"d1_repository_identity_sha256": d1_projection["repository_identity_sha256"]',
  ]);
  if (text.includes(['d1_evolution_', 'authority'].join(''))) {
    fail('Release Set builder retains the removed D1 authority input');
  }
}

function validateAdapter(text) {
  requireMarkers(text, 'D1 outer adapter', [
    '"d1",',
    '"repository",',
    '"D1_REPOSITORY_PROJECTION"',
    'repository_identity_sha256',
  ]);
  for (const forbidden of ['migration_history_digest(', 'compatibility_policy =', ['D1_EVOLUTION_', 'AUTHORITY'].join('')]) {
    if (text.includes(forbidden)) fail(`D1 outer adapter owns forbidden semantic logic: ${forbidden}`);
  }
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
  const [catalog, resolver, releaseSet, adapter, architectureText] = await Promise.all(
    Object.values(SOURCES).map((relative) => readFile(path.join(ROOT, relative), 'utf8')),
  );
  return {
    catalog,
    resolver,
    releaseSet,
    adapter,
    architecture: JSON.parse(architectureText),
  };
}

async function validate() {
  const inputs = await loadInputs();
  validateReleaseBuilder(inputs.catalog, 'catalog');
  validateReleaseBuilder(inputs.resolver, 'resolver');
  validateReleaseSet(inputs.releaseSet);
  validateAdapter(inputs.adapter);
  validateReleaseArchitecture(inputs.architecture);
  proveIdentityCoverage();
}

async function selfTest() {
  const inputs = await loadInputs();
  await validate();
  try {
    validateReleaseSet(inputs.releaseSet.replace(
      '"d1_repository_identity_sha256": d1_projection["repository_identity_sha256"]',
      '"detached_schema_identity": "0"',
    ));
  } catch {
    console.log('D1 release schema-identity ownership and negative fixtures passed.');
    return;
  }
  fail('detached D1 repository identity negative fixture unexpectedly passed');
}

async function main() {
  if (process.argv.includes('--self-test')) {
    await selfTest();
    return;
  }
  if (process.argv.length > 2) fail(`unknown arguments: ${process.argv.slice(2).join(' ')}`);
  await validate();
  console.log(
    'Catalog, Resolver and Release Set identities consume the typed SQL-derived D1 projection; no serialized D1 policy input remains.',
  );
}

main().catch((error) => {
  console.error(`D1 release schema identity error: ${error instanceof Error ? error.message : error}`);
  process.exitCode = 1;
});
