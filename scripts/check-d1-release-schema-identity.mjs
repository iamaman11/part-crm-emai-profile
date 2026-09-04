#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { DatabaseSync } from 'node:sqlite';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const MIGRATIONS_DIR = path.join(ROOT, 'migrations', 'd1');
const PAS2_EXPAND_MIGRATION = '0027_pas2_payload_fingerprint_expand.sql';
const PAS2_CONTRACT_MIGRATION = '0032_pas2_payload_fingerprint_contract.sql';
const SUPERSEDED_PAS2_MIGRATION = '0027_pas2_payload_fingerprint.sql';
const PAS2_RUNTIME_TARGET = '0031_device_binding_governance.sql';
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
    'let projection = release_contract(root, component)?;',
    'target_schema_revision: required("target_schema_revision")?',
    'supported_schema_min: required("supported_schema_min")?',
    'supported_schema_max: required("supported_schema_max")?',
    'compatibility_policy_digest: required("compatibility_policy_digest")?',
  ]);
  forbidMarkers(inputs.releaseD1, 'typed D1 Release Set adapter', [
    'target_schema_revision: authority.current_repository_revision.clone()',
    'supported_schema_min: authority.current_repository_revision.clone()',
    'supported_schema_max: authority.current_repository_revision',
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

function columns(database, table) {
  return database.prepare(`PRAGMA table_info(${quoteIdentifier(table)})`).all();
}

function columnNames(database, table) {
  return columns(database, table).map((column) => String(column.name));
}

function assertBridgeShape(database, table, label) {
  const tableColumns = columns(database, table);
  const requestDigest = tableColumns.find((column) => column.name === 'request_digest');
  const payloadFingerprint = tableColumns.find((column) => column.name === 'payload_fingerprint');
  if (!requestDigest || !payloadFingerprint) fail(`${label} must contain both transitional trust columns`);
  if (Number(requestDigest.notnull) !== 0 || Number(payloadFingerprint.notnull) !== 0) {
    fail(`${label} transitional trust columns must both be nullable at the column level`);
  }
}

function assertContractShape(database, table, label) {
  const tableColumns = columns(database, table);
  const names = tableColumns.map((column) => String(column.name));
  if (names.includes('request_digest')) fail(`${label} retained retired request_digest`);
  const fingerprintColumn = tableColumns.find((column) => column.name === 'payload_fingerprint');
  if (!fingerprintColumn) fail(`${label} omitted payload_fingerprint`);
  if (Number(fingerprintColumn.notnull) !== 0) {
    fail(`${label} payload_fingerprint must permit migrated NULL tombstones`);
  }
}

function assertTrigger(database, trigger) {
  const row = database.prepare(
    "SELECT name FROM sqlite_schema WHERE type = 'trigger' AND name = ?",
  ).get(trigger);
  if (!row) fail(`missing D1 trigger after PAS-2 transition: ${trigger}`);
}

function assertNoPas2TemporaryTables(database) {
  const temporary = database.prepare(
    "SELECT name FROM sqlite_schema WHERE type = 'table' AND name LIKE '%pas2_%_legacy' ORDER BY name",
  ).all();
  if (temporary.length !== 0) fail(`PAS-2 retained temporary tables: ${JSON.stringify(temporary)}`);
}

function assertDatabaseIntegrity(database, label) {
  const violations = database.prepare('PRAGMA foreign_key_check').all();
  if (violations.length > 0) fail(`${label} foreign_key_check failed: ${JSON.stringify(violations)}`);
  const integrity = database.prepare('PRAGMA integrity_check').all();
  if (integrity.length !== 1 || integrity[0].integrity_check !== 'ok') {
    fail(`${label} integrity_check failed: ${JSON.stringify(integrity)}`);
  }
}

async function provePas2PayloadFingerprintCutover() {
  const names = (await readdir(MIGRATIONS_DIR))
    .filter((name) => /^\d{4}_[a-z0-9_]+\.sql$/u.test(name))
    .sort();
  if (names.includes(SUPERSEDED_PAS2_MIGRATION)) fail(`superseded migration remains canonical: ${SUPERSEDED_PAS2_MIGRATION}`);
  const expandIndex = names.indexOf(PAS2_EXPAND_MIGRATION);
  const contractIndex = names.indexOf(PAS2_CONTRACT_MIGRATION);
  if (expandIndex < 0) fail(`missing ${PAS2_EXPAND_MIGRATION}`);
  if (contractIndex < 0) fail(`missing ${PAS2_CONTRACT_MIGRATION}`);
  if (contractIndex <= expandIndex) fail('PAS-2 contract migration must trail the expand migration');
  if (names[contractIndex - 1] !== PAS2_RUNTIME_TARGET) {
    fail(`PAS-2 contract predecessor must be ${PAS2_RUNTIME_TARGET}`);
  }

  const database = new DatabaseSync(':memory:');
  try {
    database.exec('PRAGMA foreign_keys = ON;');
    for (const name of names.slice(0, expandIndex)) {
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
        'legacy-browser-digest-should-remain-untrusted', 'activated', 'generation_01JPAS2', 10, 100
      );
    `);

    database.exec(await readFile(path.join(MIGRATIONS_DIR, PAS2_EXPAND_MIGRATION), 'utf8'));
    assertBridgeShape(database, 'idempotency_records', 'PAS-2 idempotency expand bridge');
    assertBridgeShape(database, 'outbound_mail_intents', 'PAS-2 outbound-mail expand bridge');
    assertNoPas2TemporaryTables(database);

    const legacyBridge = database.prepare(`
      SELECT request_digest, payload_fingerprint, command_name, result_code, result_reference,
             created_at_ms, expires_at_ms
      FROM idempotency_records
      WHERE tenant_id = 'tenant_01JPAS2'
        AND actor_id = 'actor_01JPAS2'
        AND idempotency_key = 'idem_01JPAS2'
    `).get();
    if (!legacyBridge) fail('PAS-2 expand lost the legacy idempotency row');
    if (legacyBridge.request_digest !== 'legacy-browser-digest-should-remain-untrusted') {
      fail('PAS-2 expand changed the historical request digest bytes');
    }
    if (legacyBridge.payload_fingerprint !== null) {
      fail('PAS-2 expand copied or reclassified a legacy request digest');
    }

    const validFingerprint = 'a'.repeat(64);
    database.prepare(`
      INSERT INTO idempotency_records(
        tenant_id, actor_id, idempotency_key, command_name, request_digest, payload_fingerprint,
        result_code, result_reference, created_at_ms, expires_at_ms
      ) VALUES (?, ?, ?, ?, NULL, ?, ?, ?, ?, ?)
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

    expectSqlFailure('PAS-2 bridge both trust representations NULL', () => {
      database.prepare(`
        INSERT INTO idempotency_records(
          tenant_id, actor_id, idempotency_key, command_name, request_digest, payload_fingerprint,
          result_code, created_at_ms, expires_at_ms
        ) VALUES (?, ?, ?, ?, NULL, NULL, ?, ?, ?)
      `).run('tenant_01JPAS2', 'actor_01JPAS2', 'idem_03JPAS2', 'profile_generation.verify', 'verified', 20, 120);
    });
    expectSqlFailure('PAS-2 bridge both trust representations present', () => {
      database.prepare(`
        INSERT INTO idempotency_records(
          tenant_id, actor_id, idempotency_key, command_name, request_digest, payload_fingerprint,
          result_code, created_at_ms, expires_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
      `).run(
        'tenant_01JPAS2', 'actor_01JPAS2', 'idem_04JPAS2', 'profile_generation.verify',
        'legacy-browser-digest-should-not-mix', validFingerprint, 'verified', 20, 120,
      );
    });
    expectSqlFailure('PAS-2 bridge uppercase payload fingerprint', () => {
      database.prepare(`
        INSERT INTO idempotency_records(
          tenant_id, actor_id, idempotency_key, command_name, request_digest, payload_fingerprint,
          result_code, created_at_ms, expires_at_ms
        ) VALUES (?, ?, ?, ?, NULL, ?, ?, ?, ?)
      `).run('tenant_01JPAS2', 'actor_01JPAS2', 'idem_05JPAS2', 'profile_generation.verify', 'A'.repeat(64), 'verified', 20, 120);
    });
    expectSqlFailure('PAS-2 bridge request_digest mutation', () => {
      database.prepare(`UPDATE idempotency_records SET request_digest = ? WHERE idempotency_key = ?`)
        .run('changed-legacy-browser-digest', 'idem_01JPAS2');
    });
    expectSqlFailure('PAS-2 bridge payload_fingerprint mutation', () => {
      database.prepare(`UPDATE idempotency_records SET payload_fingerprint = ? WHERE idempotency_key = ?`)
        .run('b'.repeat(64), 'idem_02JPAS2');
    });
    assertDatabaseIntegrity(database, 'PAS-2 expand');

    for (const name of names.slice(expandIndex + 1, contractIndex)) {
      database.exec(await readFile(path.join(MIGRATIONS_DIR, name), 'utf8'));
    }
    const preContractLegacy = database.prepare(
      "SELECT request_digest, payload_fingerprint FROM idempotency_records WHERE idempotency_key = 'idem_01JPAS2'",
    ).get();
    const preContractSuccessor = database.prepare(
      "SELECT request_digest, payload_fingerprint FROM idempotency_records WHERE idempotency_key = 'idem_02JPAS2'",
    ).get();
    if (!preContractLegacy || preContractLegacy.payload_fingerprint !== null) {
      fail('ordinary migrations changed legacy bridge tombstone semantics');
    }
    if (!preContractSuccessor || preContractSuccessor.request_digest !== null || preContractSuccessor.payload_fingerprint !== validFingerprint) {
      fail('ordinary migrations changed successor fingerprint semantics');
    }
    assertBridgeShape(database, 'outbound_mail_intents', 'pre-contract outbound-mail bridge');
    assertDatabaseIntegrity(database, 'PAS-2 pre-contract');

    database.exec(await readFile(path.join(MIGRATIONS_DIR, PAS2_CONTRACT_MIGRATION), 'utf8'));
    assertContractShape(database, 'idempotency_records', 'PAS-2 idempotency contract');
    assertContractShape(database, 'outbound_mail_intents', 'PAS-2 outbound-mail contract');
    assertNoPas2TemporaryTables(database);

    const durableLegacy = database.prepare(`
      SELECT command_name, payload_fingerprint, result_code, result_reference,
             created_at_ms, expires_at_ms
      FROM idempotency_records
      WHERE tenant_id = 'tenant_01JPAS2'
        AND actor_id = 'actor_01JPAS2'
        AND idempotency_key = 'idem_01JPAS2'
    `).get();
    if (!durableLegacy || durableLegacy.payload_fingerprint !== null) {
      fail('PAS-2 contract failed to preserve the legacy row as an untrusted NULL tombstone');
    }
    const durableSuccessor = database.prepare(`
      SELECT payload_fingerprint FROM idempotency_records
      WHERE tenant_id = 'tenant_01JPAS2'
        AND actor_id = 'actor_01JPAS2'
        AND idempotency_key = 'idem_02JPAS2'
    `).get();
    if (!durableSuccessor || durableSuccessor.payload_fingerprint !== validFingerprint) {
      fail('PAS-2 contract changed the trusted successor payload fingerprint');
    }

    expectSqlFailure('post-contract insert without payload fingerprint', () => {
      database.prepare(`
        INSERT INTO idempotency_records(
          tenant_id, actor_id, idempotency_key, command_name, payload_fingerprint,
          result_code, created_at_ms, expires_at_ms
        ) VALUES (?, ?, ?, ?, NULL, ?, ?, ?)
      `).run('tenant_01JPAS2', 'actor_01JPAS2', 'idem_06JPAS2', 'profile_generation.verify', 'verified', 20, 120);
    });
    expectSqlFailure('post-contract legacy tombstone upgrade', () => {
      database.prepare(`UPDATE idempotency_records SET payload_fingerprint = ? WHERE idempotency_key = ?`)
        .run('c'.repeat(64), 'idem_01JPAS2');
    });
    expectSqlFailure('post-contract trusted fingerprint mutation', () => {
      database.prepare(`UPDATE idempotency_records SET payload_fingerprint = ? WHERE idempotency_key = ?`)
        .run('d'.repeat(64), 'idem_02JPAS2');
    });

    for (const trigger of [
      'idempotency_payload_fingerprint_required',
      'idempotency_payload_fingerprint_immutable',
      'outbound_mail_payload_fingerprint_required',
      'outbound_mail_payload_fingerprint_immutable',
      'outbound_mail_intent_validate_access',
      'outbound_mail_dispatch_claim_validate',
      'outbound_mail_dispatch_claim_apply',
      'outbound_mail_dispatch_completion_validate',
      'outbound_mail_dispatch_completion_apply',
      'outbound_mail_ambiguity_mark_validate',
      'outbound_mail_ambiguity_mark_apply',
    ]) {
      assertTrigger(database, trigger);
    }
    if (columnNames(database, 'outbound_mail_intents').includes('request_digest')) {
      fail('PAS-2 contract retained outbound-mail request_digest');
    }
    assertDatabaseIntegrity(database, 'PAS-2 contract');
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

  expectValidationFailure('detached typed D1 schema window', {
    ...inputs,
    releaseD1: inputs.releaseD1.replace(
      'let projection = release_contract(root, component)?;',
      'let projection = serde_json::json!({});',
    ),
  });

  console.log('D1 release schema-identity ownership, phased PAS-2 expand/contract semantics, and negative fixtures passed.');
}

async function main() {
  if (process.argv.includes('--self-test')) {
    await selfTest();
    return;
  }
  if (process.argv.length > 2) fail(`unknown arguments: ${process.argv.slice(2).join(' ')}`);
  await validate();
  console.log(
    'Catalog and Resolver identities consume the canonical D1 projection; Release Set v3 consumes the typed schema window; PAS-2 proves data-preserving expand followed by an explicit destructive contract without reclassifying legacy digest bytes.',
  );
}

main().catch((error) => {
  console.error(`D1 release schema identity error: ${error instanceof Error ? error.message : error}`);
  process.exitCode = 1;
});
