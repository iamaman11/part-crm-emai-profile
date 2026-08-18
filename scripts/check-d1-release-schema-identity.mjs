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

function conservativeWindow(authority, componentId) {
  const component = authority.components.find((entry) => entry.component_id === componentId);
  if (!component) fail(`${componentId} D1 authority component is missing`);
  const target = component.current_repository_revision;
  if (!target) fail(`${componentId} current repository revision is missing`);
  const compatibility = component.component_release_compatibility;
  if (!compatibility || compatibility.target_schema_revision !== target) {
    fail(`${componentId} component release compatibility target is stale`);
  }
  if (compatibility.supported_schema_min !== target || compatibility.supported_schema_max !== target) {
    fail(`${componentId} frozen epoch must keep supported_min = target = supported_max`);
  }
  for (const field of ['migration_history_digest', 'compatibility_policy_digest']) {
    const value = compatibility[field];
    if (typeof value !== 'string' || value.length === 0) fail(`${componentId} compatibility lacks ${field}`);
  }
  return compatibility;
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

async function validate() {
  const [catalog, resolver, authorityText] = await Promise.all([
    readFile(path.join(ROOT, CATALOG_RELEASE), 'utf8'),
    readFile(path.join(ROOT, RESOLVER_RELEASE), 'utf8'),
    readFile(path.join(ROOT, AUTHORITY), 'utf8'),
  ]);
  const authority = JSON.parse(authorityText);

  requireOrderedMarkers(catalog, 'Catalog release', [
    'def build_manifest_payload(',
    '"schema_contract": load_schema_contract(root)',
    'def finalized_manifest(',
    'release_id = release_id_for(payload)',
  ]);
  if (!catalog.includes('return RELEASE_PREFIX + sha256_bytes(canonical(payload))')) {
    fail('Catalog release_id_for must hash the complete canonical payload');
  }

  requireOrderedMarkers(resolver, 'Resolver release', [
    'def manifest_for(',
    '"schema_contract": load_schema_contract(root)',
    'release_id = RELEASE_PREFIX + sha256_bytes(canonical(payload))',
  ]);

  conservativeWindow(authority, 'catalog');
  conservativeWindow(authority, 'resolver');
  proveSyntheticIdentity('catalog');
  proveSyntheticIdentity('resolver');
}

async function selfTest() {
  await validate();
  const payload = { schema_contract: { compatibility_policy_digest: 'a'.repeat(64) } };
  const mutated = structuredClone(payload);
  mutated.schema_contract.compatibility_policy_digest = 'b'.repeat(64);
  if (releaseId(payload) === releaseId(mutated)) fail('schema policy negative fixture retained release identity');
  console.log('D1 release schema-identity negative fixture passed.');
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
