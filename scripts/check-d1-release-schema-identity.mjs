#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const CATALOG_RELEASE = 'scripts/cloudflare-release.py';
const RESOLVER_RELEASE = 'scripts/mailbox-secret-resolver-release.py';
const AUTHORITY = 'architecture/d1-evolution-ar9.json';

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

function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
}

function releaseId(payload, prefix = 'release-') {
  return `${prefix}${sha256(canonical(payload))}`;
}

function requireOrderedMarkers(text, label, markers) {
  let cursor = -1;
  for (const marker of markers) {
    const index = text.indexOf(marker, cursor + 1);
    if (index < 0) fail(`${label} lost required release-identity marker: ${marker}`);
    if (index <= cursor) fail(`${label} release-identity markers are out of order`);
    cursor = index;
  }
}

function validateReleaseSources(catalog, resolver) {
  requireOrderedMarkers(catalog, 'Catalog release', [
    'def build_manifest_payload(',
    '"schema_contract": load_schema_contract(root)',
    'def finalized_manifest(',
    'manifest["release_id"] = release_id_for(payload)',
  ]);
  if (!catalog.includes('def release_id_for(payload: dict[str, Any]) -> str:')) {
    fail('Catalog release_id_for function is missing');
  }
  if (!catalog.includes('return RELEASE_PREFIX + sha256_bytes(canonical_compact(payload))')) {
    fail('Catalog release_id_for must hash the complete canonical compact payload');
  }
  if (!catalog.includes('del payload["release_id"]') || !catalog.includes('if release_id_for(payload) != release_id:')) {
    fail('Catalog release verifier must recompute immutable identity from the complete payload');
  }

  requireOrderedMarkers(resolver, 'Resolver release', [
    'def manifest_for(',
    '"schema_contract": load_schema_contract(root)',
    'release_id = RELEASE_PREFIX + sha256_bytes(canonical(payload))',
    'return {"release_id": release_id, **payload}',
  ]);
  if (!resolver.includes('del payload["release_id"]') || !resolver.includes('if release_id != RELEASE_PREFIX + sha256_bytes(canonical(payload)):')) {
    fail('Resolver release verifier must recompute immutable identity from the complete payload');
  }
}

function derivedSchemaContract(authority, componentId) {
  const component = authority.components.find((entry) => entry.component_id === componentId);
  if (!component) fail(`${componentId} D1 authority component is missing`);
  const target = component.current_repository_revision;
  if (typeof target !== 'string' || target.length === 0) {
    fail(`${componentId} current repository revision is missing`);
  }
  const historyDigest = component.history_digest;
  if (typeof historyDigest !== 'string' || historyDigest.length === 0) {
    fail(`${componentId} history digest is missing`);
  }
  const policy = component.compatibility_policy;
  if (!policy || typeof policy !== 'object' || Array.isArray(policy)) {
    fail(`${componentId} compatibility policy is missing`);
  }
  const historical = component.historical_epoch;
  if (!historical || typeof historical !== 'object' || historical.per_file_sha256_freeze?.status !== 'FROZEN') {
    fail(`${componentId} historical epoch is not fully frozen`);
  }
  return {
    database_component: componentId,
    target_schema_revision: target,
    supported_schema_min: target,
    supported_schema_max: target,
    migration_history_digest: historyDigest,
    compatibility_policy_digest: sha256(canonical(policy)),
  };
}

function proveSyntheticIdentity(component) {
  const payload = {
    binary_bits_sha256: 'a'.repeat(64),
    configuration_sha256: 'b'.repeat(64),
    schema_contract: {
      database_component: component,
      target_schema_revision: '0001_fixture.sql',
      supported_schema_min: '0001_fixture.sql',
      supported_schema_max: '0001_fixture.sql',
      migration_history_digest: 'c'.repeat(64),
      compatibility_policy_digest: 'd'.repeat(64),
    },
  };
  const changedPolicy = structuredClone(payload);
  changedPolicy.schema_contract.compatibility_policy_digest = 'e'.repeat(64);
  if (releaseId(payload) === releaseId(changedPolicy)) {
    fail(`${component} schema policy mutation did not change immutable release ID`);
  }
  const changedBinary = structuredClone(payload);
  changedBinary.binary_bits_sha256 = 'f'.repeat(64);
  if (releaseId(payload) === releaseId(changedBinary)) {
    fail(`${component} binary mutation did not change immutable release ID`);
  }
}

function expectSourceRejected(label, catalog, resolver) {
  try {
    validateReleaseSources(catalog, resolver);
  } catch {
    return;
  }
  fail(`negative release source fixture unexpectedly passed: ${label}`);
}

async function loadInputs() {
  const [catalog, resolver, authorityText] = await Promise.all([
    readFile(path.join(ROOT, CATALOG_RELEASE), 'utf8'),
    readFile(path.join(ROOT, RESOLVER_RELEASE), 'utf8'),
    readFile(path.join(ROOT, AUTHORITY), 'utf8'),
  ]);
  return { catalog, resolver, authority: JSON.parse(authorityText) };
}

function validateAuthority(authority) {
  if (!authority || authority.kind !== 'D1_EVOLUTION_AUTHORITY' || !Array.isArray(authority.components)) {
    fail('D1 evolution authority identity is invalid');
  }
  const catalogContract = derivedSchemaContract(authority, 'catalog');
  const resolverContract = derivedSchemaContract(authority, 'resolver');
  for (const contract of [catalogContract, resolverContract]) {
    if (contract.supported_schema_min !== contract.target_schema_revision || contract.supported_schema_max !== contract.target_schema_revision) {
      fail(`${contract.database_component} frozen epoch schema window is not conservative`);
    }
    if (!/^[0-9a-f]{64}$/.test(contract.migration_history_digest) || !/^[0-9a-f]{64}$/.test(contract.compatibility_policy_digest)) {
      fail(`${contract.database_component} derived schema contract digests are invalid`);
    }
  }
  proveSyntheticIdentity('catalog');
  proveSyntheticIdentity('resolver');
}

async function validate() {
  const { catalog, resolver, authority } = await loadInputs();
  validateReleaseSources(catalog, resolver);
  validateAuthority(authority);
}

async function selfTest() {
  const { catalog, resolver, authority } = await loadInputs();
  validateReleaseSources(catalog, resolver);
  validateAuthority(authority);

  expectSourceRejected(
    'Catalog schema contract removed from identity payload',
    catalog.replace('        "schema_contract": load_schema_contract(root),\n', ''),
    resolver,
  );
  expectSourceRejected(
    'Catalog release ID detached from full payload',
    catalog.replace(
      'manifest["release_id"] = release_id_for(payload)',
      'manifest["release_id"] = RELEASE_PREFIX + "0" * 64',
    ),
    resolver,
  );
  expectSourceRejected(
    'Resolver schema contract removed from identity payload',
    catalog,
    resolver.replace('        "schema_contract": load_schema_contract(root),\n', ''),
  );
  expectSourceRejected(
    'Resolver release ID detached from full payload',
    catalog,
    resolver.replace(
      'release_id = RELEASE_PREFIX + sha256_bytes(canonical(payload))',
      'release_id = RELEASE_PREFIX + "0" * 64',
    ),
  );

  const payload = { schema_contract: { compatibility_policy_digest: 'a'.repeat(64) } };
  const mutated = structuredClone(payload);
  mutated.schema_contract.compatibility_policy_digest = 'b'.repeat(64);
  if (releaseId(payload) === releaseId(mutated)) fail('schema policy negative fixture retained release identity');
  console.log('D1 release schema-identity static and policy negative fixtures passed.');
}

async function main() {
  if (process.argv.includes('--self-test')) {
    await selfTest();
    return;
  }
  if (process.argv.length > 2) fail(`unknown arguments: ${process.argv.slice(2).join(' ')}`);
  await validate();
  console.log('Catalog and Resolver immutable release identities bind schema target/min/max, history digest and compatibility-policy digest; policy changes alter release ID with binary bits held constant.');
}

main().catch((error) => {
  console.error(`D1 release schema identity error: ${error instanceof Error ? error.message : error}`);
  process.exitCode = 1;
});
