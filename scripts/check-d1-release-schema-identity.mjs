#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { DatabaseSync } from 'node:sqlite';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const MIGRATIONS_DIR = path.join(ROOT, 'migrations', 'd1');
const PAS2_FINGERPRINT_MIGRATION = '0027_pas2_payload_fingerprint.sql';
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

function quoteIdentifier(value) {
  return `"${value.replaceAll('"', '""')}"`;
}

function expectSqlFailure(label, operation) {
  try {
    operation();
  } catch {
    return;
  }
  fail(`${label} negative D1 fixture unexpectedly passed`);
}

function assertPayloadFingerprintShape(database, label) {
  const columns = database.prepare('PRAGMA table_info(idempotency_records)').all();
  const columnNames = columns.map((column) => String(column.name));
  if (columnNames.includes('request_digest')) fail(`${label} retained request_digest`);
  const fingerprintColumn = columns.find((column) => column.name === 'payload_fingerprint');
  if (!fingerprintColumn) fail(`${label} omitted payload_fingerprint`);
  if (Number(fingerprintColumn.notnull) !== 0) {
    fail(`${label} payload_fingerprint must permit migrated NULL tombstones`);
  }
}

async function provePas2PayloadFingerprintCutover() {
  const names = (await readdir(MIGRATIONS_DIR))
    .filter((name) => /^\d{4}_[a-z0-9_]+\.sql$/u.test(name))
    .sort();
  const targetIndex = names.indexOf(PAS2_FINGERPRINT_MIGRATION);
  if (targetIndex < 0) fail(`missing ${PAS2_FINGERPRINT_MIGRATION}`);

  const database = new DatabaseSync(':memory:');
  try {
    database.exec('PRAGMA foreign_keys = ON;');
    for (const name of names.slice(0, targetIndex)) {
      database.exec(await readFile(path.join(MIGRATIONS_DIR, name), 'utf8'));
    }

    const inboundReferences = [];
    const tables = database.prepare(
      "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    ).all();
    for (const { name } of tables) {
      const foreignKeys = database.prepare(`PRAGMA foreign_key_list(${quoteIdentifier(String(name))})`).all();
      for (const foreignKey of foreignKeys) {
        if (foreignKey.table === 'idempotency_records') {
          inboundReferences.push(`${name}:${foreignKey.from ?? '?'}->${foreignKey.to ?? '?'}`);
        }
      }
    }
    if (inboundReferences.length > 0) {
      fail(`PAS-2 idempotency table rebuild has inbound foreign keys: ${inboundReferences.join(', ')}`);
    }

    database.exec(`
      INSERT INTO tenants(tenant_id, display_name, status, version, created_at_ms, updated_at_ms)
      VALUES ('tenant_01JPAS2', 'PAS2 fixture', 'ACTIVE', 1, 1, 1);
      INSERT INTO identities(identity_id, access_subject, verified_contact_hint, created_at_ms)
      VALUES ('identity_01JPAS2', 'pas2-fixture-subject', NULL, 1);
      INSERT INTO memberships(
        tenant_id, actor_id, identity_id, role, status, version, created_at_ms, updated_at_ms
      ) VALUES (
        'tenant_01JPAS2', 'actor_01JPAS2', 'identity_01JPAS2', 'TENANT_OWNER', 'ACTIVE', 1, 1, 1
      );
      INSERT INTO idempotency_records(
        tenant_id, actor_id, idempotency_key, command_name, request_digest,
        result_code, result_reference, created_at_ms, expires_at_ms
      ) VALUES (
        'tenant_01JPAS2', 'actor_01JPAS2', 'idem_01JPAS2', 'profile_generation.activate',
        'legacy-browser-digest-should-die', 'activated', 'generation_01JPAS2', 10, 100
      );
    `);

    database.exec(await readFile(path.join(MIGRATIONS_DIR, PAS2_FINGERPRINT_MIGRATION), 'utf8'));
    assertPayloadFingerprintShape(database, 'PAS-2 migration');

    const legacy = database.prepare(`
      SELECT command_name, payload_fingerprint, result_code, result_reference,
             created_at_ms, expires_at_ms
      FROM idempotency_records
      WHERE tenant_id = 'tenant_01JPAS2'
        AND actor_id = 'actor_01JPAS2'
        AND idempotency_key = 'idem_01JPAS2'
    `).get();
    if (!legacy) fail('PAS-2 migration lost the legacy idempotency key tombstone');
    if (legacy.payload_fingerprint !== null) {
      fail('PAS-2 migration trusted or copied a legacy request digest');
    }
    if (
      legacy.command_name !== 'profile_generation.activate'
      || legacy.result_code !== 'activated'
      || legacy.result_reference !== 'generation_01JPAS2'
      || Number(legacy.created_at_ms) !== 10
      || Number(legacy.expires_at_ms) !== 100
    ) {
      fail('PAS-2 migration changed legacy tombstone replay metadata');
    }
    if (database.prepare(
      "SELECT name FROM sqlite_schema WHERE type = 'table' AND name = 'idempotency_records_pas2_legacy'",
    ).get()) {
      fail('PAS-2 migration retained the predecessor idempotency table');
    }

    const validFingerprint = 'a'.repeat(64);
    database.prepare(`
      INSERT INTO idempotency_records(
        tenant_id, actor_id, idempotency_key, command_name, payload_fingerprint,
        result_code, result_reference, created_at_ms, expires_at_ms
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
    `).run(
      'tenant_01JPAS2',
      'actor_01JPAS2',
      'idem_02JPAS2',
      'profile_generation.verify',
      validFingerprint,
      'verified',
      'generation_01JPAS2',
      20,
      120,
    );

    expectSqlFailure('legacy browser digest as current payload fingerprint', () => {
      database.prepare(`
        INSERT INTO idempotency_records(
          tenant_id, actor_id, idempotency_key, command_name, payload_fingerprint,
          result_code, created_at_ms, expires_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
      `).run(
        'tenant_01JPAS2',
        'actor_01JPAS2',
        'idem_03JPAS2',
        'profile_generation.verify',
        'legacy-browser-digest-should-die',
        'verified',
        20,
        120,
      );
    });
    expectSqlFailure('non-lowercase SHA-256 payload fingerprint', () => {
      database.prepare(`
        INSERT INTO idempotency_records(
          tenant_id, actor_id, idempotency_key, command_name, payload_fingerprint,
          result_code, created_at_ms, expires_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
      `).run(
        'tenant_01JPAS2',
        'actor_01JPAS2',
        'idem_04JPAS2',
        'profile_generation.verify',
        'A'.repeat(64),
        'verified',
        20,
        120,
      );
    });

    for (const name of names.slice(targetIndex + 1)) {
      database.exec(await readFile(path.join(MIGRATIONS_DIR, name), 'utf8'));
    }
    assertPayloadFingerprintShape(database, 'post-PAS-2 migration history');

    const durableLegacy = database.prepare(`
      SELECT command_name, payload_fingerprint, result_code, result_reference,
             created_at_ms, expires_at_ms
      FROM idempotency_records
      WHERE tenant_id = 'tenant_01JPAS2'
        AND actor_id = 'actor_01JPAS2'
        AND idempotency_key = 'idem_01JPAS2'
    `).get();
    if (!durableLegacy || durableLegacy.payload_fingerprint !== null) {
      fail('later catalog migrations changed the PAS-2 legacy tombstone semantics');
    }

    const violations = database.prepare('PRAGMA foreign_key_check').all();
    if (violations.length > 0) fail(`PAS-2 migration foreign_key_check failed: ${JSON.stringify(violations)}`);
    const integrity = database.prepare('PRAGMA integrity_check').all();
    if (integrity.length !== 1 || integrity[0].integrity_check !== 'ok') {
      fail(`PAS-2 migration integrity_check failed: ${JSON.stringify(integrity)}`);
    }
  } finally {
    database.close();
  }
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
  await provePas2PayloadFingerprintCutover();
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
  await provePas2PayloadFingerprintCutover();

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

  console.log('D1 release schema-identity ownership, PAS-2 migration semantics, and negative fixtures passed.');
}

async function main() {
  if (process.argv.includes('--self-test')) {
    await selfTest();
    return;
  }
  if (process.argv.length > 2) fail(`unknown arguments: ${process.argv.slice(2).join(' ')}`);
  await validate();
  console.log(
    'Catalog and Resolver identities consume the canonical D1 projection; Release Set v3 consumes typed Rust D1 identity through pure core and its content-addressed DTO; PAS-2 idempotency migration destroys legacy digest trust.',
  );
}

main().catch((error) => {
  console.error(`D1 release schema identity error: ${error instanceof Error ? error.message : error}`);
  process.exitCode = 1;
});
