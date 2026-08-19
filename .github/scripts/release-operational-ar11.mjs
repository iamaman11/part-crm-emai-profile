#!/usr/bin/env node

import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';

const ROOT = process.cwd();
const BUILD = '.github/workflows/release-set-build.yml';
const PROMOTION = '.github/workflows/release-set-promotion.yml';
const REGISTRY = 'architecture/github-actions-registry.json';
const AUTHORITY = 'architecture/release-architecture-ar11.json';
const LEGACY_FILES = [
  '.github/workflows/mailbox-secret-resolver-promotion.yml',
  'scripts/mailbox-secret-resolver-promotion.py',
  'scripts/_mailbox_secret_resolver_promotion_core.py',
];

function read(relative) {
  return readFileSync(path.join(ROOT, relative), 'utf8').replace(/\r\n?/g, '\n');
}
function requireMarkers(text, markers, label) {
  return markers.filter((marker) => !text.includes(marker)).map((marker) => `${label} is missing ${JSON.stringify(marker)}`);
}
function forbidMarkers(text, markers, label) {
  const lower = text.toLowerCase();
  return markers.filter((marker) => lower.includes(marker.toLowerCase())).map((marker) => `${label} contains forbidden authority ${JSON.stringify(marker)}`);
}
function operationalErrors({ requireCutover = true } = {}) {
  const errors = [];
  for (const relative of [BUILD, PROMOTION, REGISTRY, AUTHORITY]) {
    if (!existsSync(path.join(ROOT, relative))) errors.push(`missing AR-11 operational authority: ${relative}`);
  }
  if (errors.length > 0) return errors;
  const build = read(BUILD);
  const promotion = read(PROMOTION);
  errors.push(...requireMarkers(build, [
    'branches:\n      - main',
    'Build immutable cloud components once',
    'Build immutable Windows Profile Bridge component',
    'release-set-ar11.py build',
    'release verify',
    'gh release create',
    'gh release upload',
    'gh release download',
    'cmp --silent',
    'byte-identical durable assets',
  ], 'Release Set build'));
  errors.push(...forbidMarkers(build, [
    'CLOUDFLARE_API_TOKEN',
    'CLOUDFLARE_DEPLOY_MANIFEST_JSON',
    'wrangler deploy --env production',
    'terraform',
  ], 'Release Set build'));
  errors.push(...requireMarkers(promotion, [
    'workflow_dispatch:',
    'release_set_id:',
    'expected_current_release_set_id:',
    'confirmation:',
    'environment: staging',
    'TARGET_PROFILE: rehearsal-core-v1',
    'gh release download "$RELEASE_SET_ID"',
    'release verify',
    'd1 compatibility',
    'release compatibility',
    'promotion plan',
    'promotion preflight',
    '--expected-current "$EXPECTED_CURRENT"',
    '--dry-run',
    '--message "release_set=$RELEASE_SET_ID profile=$TARGET_PROFILE"',
    'promotion verify',
    '.verified == true',
  ], 'Release Set promotion'));
  errors.push(...forbidMarkers(promotion, [
    'environment: production',
    'TARGET_PROFILE: production-',
    'mailbox-secret-resolver-promotion.py',
    '_mailbox_secret_resolver_promotion_core.py',
    'worker-build --release',
    'cargo build',
    'run: npm run build',
    'wrangler d1 create',
    'wrangler r2 bucket create',
    'wrangler queues create',
    'CLOUDFLARE_RESOLVER_SECRETS_JSON',
    'CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON',
    'terraform',
  ], 'Release Set promotion'));

  const verifyIndex = promotion.indexOf('release verify');
  const d1Index = promotion.indexOf('d1 compatibility');
  const preflightIndex = promotion.indexOf('promotion preflight');
  const deployIndex = promotion.indexOf('Deploy exact Release Set bits only after READY preflight');
  const postVerifyIndex = promotion.lastIndexOf('promotion verify');
  if (!(0 <= verifyIndex && verifyIndex < d1Index && d1Index < preflightIndex && preflightIndex < deployIndex && deployIndex < postVerifyIndex)) {
    errors.push('promotion ordering must be release-verify -> D1 -> preflight -> exact deploy -> post-verify');
  }
  if ((promotion.match(/workflow_dispatch:/g) ?? []).length !== 1) {
    errors.push('Release Set promotion must expose exactly one manual dispatch surface');
  }

  const registry = JSON.parse(read(REGISTRY));
  const workflows = registry.active_registrations ?? [];
  const buildRows = workflows.filter((row) => row.path === BUILD);
  const promotionRows = workflows.filter((row) => row.path === PROMOTION);
  if (buildRows.length !== 1 || buildRows[0].category !== 'PERMANENT_REQUIRED') {
    errors.push('Release Set build must appear exactly once as PERMANENT_REQUIRED');
  }
  if (promotionRows.length !== 1 || promotionRows[0].category !== 'CURRENT_MANUAL_OPERATION') {
    errors.push('Release Set promotion must be the single canonical CURRENT_MANUAL_OPERATION');
  }
  const manualRows = workflows.filter((row) => row.category === 'CURRENT_MANUAL_OPERATION');
  if (manualRows.length !== 1) errors.push(`canonical Actions registry must have exactly one manual operation, observed=${manualRows.length}`);

  const authority = JSON.parse(read(AUTHORITY));
  if (authority.production_mutation !== false || authority.production_ready !== false || authority.production_core_gate !== 'BLOCKED' || authority.architecture_complete !== false) {
    errors.push('AR-11 operational workflow may not change canonical production authorization state');
  }
  const policy = authority.promotion_policy ?? {};
  if (policy.build_once !== true || policy.promotion_rebuild !== false || policy.opsctl_network !== false || policy.opsctl_credentials !== false || policy.opsctl_provider_mutation !== false || policy.production_execution_before_ar17 !== false) {
    errors.push('AR-11 promotion policy invariants drifted');
  }
  if (requireCutover) {
    for (const relative of LEGACY_FILES) {
      if (existsSync(path.join(ROOT, relative))) errors.push(`legacy D3 operational authority must be retired after Rust cutover: ${relative}`);
    }
  }
  return errors;
}
function selfTest() {
  const promotion = read(PROMOTION);
  const negative = forbidMarkers(`${promotion}\n  environment: production\n  run: cargo build --release\n`, ['environment: production', 'cargo build'], 'negative fixture');
  if (negative.length !== 2) throw new Error('AR-11 operational negative fixture unexpectedly passed');
  console.log('AR-11 operational release/promotion negative self-test passed.');
}
if (process.argv.includes('--self-test')) {
  selfTest();
  process.exit(0);
}
const errors = operationalErrors({ requireCutover: !process.argv.includes('--pre-cutover') });
if (errors.length > 0) {
  console.error('AR-11 operational policy failed:\n' + errors.map((error) => `- ${error}`).join('\n'));
  process.exit(1);
}
console.log('AR-11 durable Release Set and Rust-authoritative promotion policy passed.');
