#!/usr/bin/env node

import { readFile } from 'node:fs/promises';
import process from 'node:process';

const FORBIDDEN_VALUE_KEYS = new Set([
  'value',
  'plaintext',
  'secret_value',
  'token_value',
  'credential_value',
  'private_key',
  'key_hex',
  'keyhex',
]);
const CANONICAL_ENVIRONMENTS = new Set(['staging', 'production']);
const ALLOWED_SECRET_TYPES = new Set(['secret_text']);
const MAX_DOCUMENT_BYTES = 256 * 1024;

function object(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function fail(message) {
  throw new Error(message);
}

function validBindingName(value) {
  return typeof value === 'string'
    && value.length >= 1
    && value.length <= 128
    && /^[A-Z0-9_]+$/.test(value);
}

function rejectValueShapedFields(value, path = '$SECRET_LIST') {
  if (Array.isArray(value)) {
    value.forEach((item, index) => rejectValueShapedFields(item, `${path}[${index}]`));
    return;
  }
  if (!object(value)) return;
  for (const [name, nested] of Object.entries(value)) {
    if (FORBIDDEN_VALUE_KEYS.has(name.toLowerCase())) {
      fail(`secret-list metadata contains forbidden value-shaped field ${path}.${name}`);
    }
    rejectValueShapedFields(nested, `${path}.${name}`);
  }
}

function requiredSecretNames(config, environment) {
  if (!object(config)) fail('rendered Wrangler config must be one JSON object');
  const selected = config.env?.[environment];
  const required = selected?.secrets?.required;
  if (!Array.isArray(required) || required.length === 0) {
    fail(`rendered Wrangler env.${environment}.secrets.required must be non-empty`);
  }
  if (required.some((name) => !validBindingName(name))) {
    fail('required Worker secret binding names are malformed');
  }
  if (new Set(required).size !== required.length) {
    fail('required Worker secret binding names contain duplicates');
  }
  return [...required].sort();
}

function listedSecretNames(secretList) {
  rejectValueShapedFields(secretList);
  if (!Array.isArray(secretList)) {
    fail('wrangler secret list --format json must produce one JSON array');
  }
  const names = [];
  for (const [index, entry] of secretList.entries()) {
    if (!object(entry)) fail(`secret-list entry ${index} must be one JSON object`);
    if (!validBindingName(entry.name)) fail('secret-list entry has an invalid binding name');
    if (!ALLOWED_SECRET_TYPES.has(entry.type)) {
      fail(`secret-list entry ${entry.name} has unsupported type ${String(entry.type)}`);
    }
    names.push(entry.name);
  }
  if (new Set(names).size !== names.length) {
    fail('secret-list metadata contains duplicate binding names');
  }
  return names.sort();
}

function validate(config, environment, secretList) {
  if (!CANONICAL_ENVIRONMENTS.has(environment)) {
    fail('secret binding validation environment is not canonical');
  }
  const expected = requiredSecretNames(config, environment);
  const actual = listedSecretNames(secretList);
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    const actualSet = new Set(actual);
    const expectedSet = new Set(expected);
    const missing = expected.filter((name) => !actualSet.has(name));
    const unexpected = actual.filter((name) => !expectedSet.has(name));
    fail(`Worker secret binding inventory drifted: missing=${JSON.stringify(missing)}, unexpected=${JSON.stringify(unexpected)}`);
  }
}

async function readJson(path, label) {
  const bytes = await readFile(path);
  if (bytes.length === 0 || bytes.length > MAX_DOCUMENT_BYTES) {
    fail(`${label} has an invalid bounded size`);
  }
  try {
    return JSON.parse(bytes.toString('utf8'));
  } catch {
    fail(`${label} is not strict UTF-8 JSON`);
  }
}

function expectRejected(label, operation) {
  try {
    operation();
  } catch {
    return;
  }
  fail(`negative Worker secret binding fixture unexpectedly passed: ${label}`);
}

function selfTest() {
  const config = {
    env: {
      staging: { secrets: { required: ['CALLER_AUTH_KEY', 'ENCRYPTION_KEYRING'] } },
      production: { secrets: { required: ['CALLER_AUTH_KEY', 'ENCRYPTION_KEYRING'] } },
    },
  };
  const valid = [
    { name: 'ENCRYPTION_KEYRING', type: 'secret_text' },
    { name: 'CALLER_AUTH_KEY', type: 'secret_text' },
  ];
  validate(config, 'staging', valid);
  expectRejected('missing required secret', () => validate(config, 'staging', valid.slice(0, 1)));
  expectRejected('unexpected stale secret', () => validate(config, 'staging', [
    ...valid,
    { name: 'STALE_SECRET', type: 'secret_text' },
  ]));
  expectRejected('secret value field', () => validate(config, 'staging', [
    { name: 'CALLER_AUTH_KEY', type: 'secret_text', value: 'forbidden' },
  ]));
  expectRejected('missing secrets.required', () => validate({ env: { staging: {} } }, 'staging', valid));
  expectRejected('noncanonical environment', () => validate(config, 'prod', valid));
  console.log('Worker secret binding metadata validator self-test passed.');
}

function argument(name) {
  const index = process.argv.indexOf(name);
  if (index < 0 || index + 1 >= process.argv.length) fail(`missing ${name}`);
  return process.argv[index + 1];
}

async function main() {
  if (process.argv.includes('--self-test')) {
    selfTest();
    return;
  }
  const configPath = argument('--config');
  const environment = argument('--environment');
  const secretListPath = argument('--secret-list');
  validate(
    await readJson(configPath, 'rendered Wrangler config'),
    environment,
    await readJson(secretListPath, 'wrangler secret-list metadata'),
  );
  console.log(`Worker secret binding metadata exactly matches secrets.required for ${environment}.`);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});
