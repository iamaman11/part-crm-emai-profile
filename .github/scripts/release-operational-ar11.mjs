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
  return markers
    .filter((marker) => !text.includes(marker))
    .map((marker) => `${label} is missing ${JSON.stringify(marker)}`);
}

function forbidMarkers(text, markers, label) {
  const lower = text.toLowerCase();
  return markers
    .filter((marker) => lower.includes(marker.toLowerCase()))
    .map((marker) => `${label} contains forbidden authority ${JSON.stringify(marker)}`);
}

function count(text, marker) {
  return text.split(marker).length - 1;
}

function jobBlock(workflow, jobName) {
  const lines = workflow.split('\n');
  const start = lines.findIndex((line) => line === `  ${jobName}:`);
  if (start < 0) return '';
  let end = lines.length;
  for (let index = start + 1; index < lines.length; index += 1) {
    if (/^  [A-Za-z0-9_-]+:\s*$/.test(lines[index])) {
      end = index;
      break;
    }
  }
  return lines.slice(start, end).join('\n');
}

function buildErrors(build) {
  const errors = [];
  errors.push(...requireMarkers(build, [
    'branches:\n      - main',
    'Build immutable cloud components once',
    'Build immutable Windows Profile Bridge component',
    'Create deterministic self-describing Profile Bridge v2 package',
    'release-set-ar11.py build',
    'accepted-source-evidence-ar11.py',
    'repos/$GITHUB_REPOSITORY/branches/main',
    'compare/$SOURCE_SHA...$main_sha',
    'release verify',
    'gh release create',
    'gh release upload',
    'gh release download',
    'cmp --silent',
    'Publish once or prove byte-identical replay',
    'cp "$RELEASE_DIR/components/"* "$asset_dir/"',
    'test "$(find "$asset_dir" -maxdepth 1 -type f | wc -l)" -eq 5',
    'test "$(find "$existing" -maxdepth 1 -type f | wc -l)" -eq 5',
  ], 'Release Set build'));
  errors.push(...forbidMarkers(build, [
    'CLOUDFLARE_API_TOKEN',
    'CLOUDFLARE_DEPLOY_MANIFEST_JSON',
    'wrangler deploy --env production',
    'environment: production',
    'terraform',
  ], 'Release Set build'));
  return errors;
}

function promotionErrors(promotion) {
  const errors = [];
  errors.push(...requireMarkers(promotion, [
    'workflow_dispatch:',
    'release_set_id:',
    'expected_current_release_set_id:',
    'confirmation:',
    'concurrency:\n  group: release-set-promotion-staging',
  ], 'Release Set promotion'));
  if (count(promotion, 'workflow_dispatch:') !== 1) {
    errors.push('Release Set promotion must expose exactly one manual dispatch surface');
  }
  if (count(promotion, 'secrets.CLOUDFLARE_API_TOKEN') !== 1) {
    errors.push('deploy-capable Cloudflare token must be referenced exactly once, inside mutation executor');
  }

  const resolve = jobBlock(promotion, 'resolve-verify');
  const observe = jobBlock(promotion, 'observe-preflight');
  const mutate = jobBlock(promotion, 'mutate');
  const post = jobBlock(promotion, 'post-verify');
  for (const [name, block] of Object.entries({ 'resolve-verify': resolve, 'observe-preflight': observe, mutate, 'post-verify': post })) {
    if (!block) errors.push(`Release Set promotion is missing structural job ${name}`);
  }
  if (errors.some((error) => error.includes('missing structural job'))) return errors;

  errors.push(...requireMarkers(resolve, [
    'Prove target source was accepted protected main',
    "test \"$(jq -r '.protected' \"$RUNNER_TEMP/protected-main.json\")\" = true",
    'compare/$source_sha...$main_sha',
    'Checkout current protected-main policy authority',
    'Checkout exact target source as read-only provenance input',
    'path: target-source',
    'accepted-source-evidence-ar11.py',
    'gh release download "$RELEASE_SET_ID"',
    'for name in release-set.json control-plane.tar secret-resolver.tar runtime-bundle.tar profile-bridge.zip',
    'release verify',
    '--source-root "$GITHUB_WORKSPACE/target-source"',
    'test "$(find "$asset_root" -maxdepth 1 -type f | wc -l)" -eq 5',
    '.source_accepted == true',
  ], 'promotion phase 1 resolve+verify'));
  errors.push(...forbidMarkers(resolve, [
    'secrets.CLOUDFLARE_',
    'environment: staging',
    'wrangler deploy',
    'wrangler d1 execute',
    'curl --silent --show-error --output "$RUNNER_TEMP/deployments-api.json"',
  ], 'promotion phase 1 resolve+verify'));

  errors.push(...requireMarkers(observe, [
    'needs: resolve-verify',
    'environment: staging',
    'TARGET_PROFILE: rehearsal-core-v1',
    'TARGET_ENVIRONMENT: staging',
    'secrets.CLOUDFLARE_OBSERVE_API_TOKEN',
    'Observe current provider state without mutation',
    'Verify current/known-good immutable Release Set before rollback evaluation',
    'Build metadata-only DeploymentSnapshot v2',
    'deployment-snapshot-ar11.py',
    'release compatibility',
    'promotion plan',
    'promotion preflight',
    '--expected-current "$EXPECTED_CURRENT"',
    'mutation-fence.json',
  ], 'promotion phase 2 observe+preflight'));
  errors.push(...forbidMarkers(observe, [
    'secrets.CLOUDFLARE_API_TOKEN',
    'wrangler deploy',
    'deployments: write',
  ], 'promotion phase 2 observe+preflight'));

  errors.push(...requireMarkers(mutate, [
    'needs: [resolve-verify, observe-preflight]',
    "if: needs.observe-preflight.outputs.decision == 'PLAN'",
    'environment: staging',
    'deployments: write',
    'CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}',
    'Download and bind exact preflight authority before provider use',
    'mutation-fence.json',
    'gh release download "$RELEASE_SET_ID"',
    '--dry-run',
    'Deploy exact Release Set v2 bits',
    '--message "release_set=$RELEASE_SET_ID profile=rehearsal-core-v1"',
  ], 'promotion phase 3 protected mutation'));
  errors.push(...forbidMarkers(mutate, [
    'worker-build --release',
    'cargo build',
    'npm run build',
    'release-set-ar11.py build',
    'release compatibility',
    'promotion plan',
    'promotion preflight',
  ], 'promotion phase 3 protected mutation'));

  errors.push(...requireMarkers(post, [
    'needs: [resolve-verify, observe-preflight, mutate]',
    "needs.mutate.result == 'success' || needs.mutate.result == 'skipped'",
    'environment: staging',
    'secrets.CLOUDFLARE_OBSERVE_API_TOKEN',
    'deployment-snapshot-ar11.py',
    'promotion verify',
    '.verified == true',
  ], 'promotion phase 4 post-deploy observation'));
  errors.push(...forbidMarkers(post, [
    'secrets.CLOUDFLARE_API_TOKEN',
    'deployments: write',
    'wrangler deploy',
  ], 'promotion phase 4 post-deploy observation'));

  errors.push(...forbidMarkers(promotion, [
    'test "$source_sha" = "$main_sha"',
    '--root "$GITHUB_WORKSPACE/target-source" release verify',
    'environment: production',
    'TARGET_PROFILE: production-',
    'mailbox-secret-resolver-promotion.py',
    '_mailbox_secret_resolver_promotion_core.py',
    'wrangler d1 create',
    'wrangler r2 bucket create',
    'wrangler queues create',
    'CLOUDFLARE_RESOLVER_SECRETS_JSON',
    'CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON',
    'terraform',
  ], 'Release Set promotion'));

  return errors;
}

function operationalErrors({ requireCutover = true } = {}) {
  const errors = [];
  for (const relative of [BUILD, PROMOTION, REGISTRY, AUTHORITY]) {
    if (!existsSync(path.join(ROOT, relative))) errors.push(`missing AR-11 operational authority: ${relative}`);
  }
  if (errors.length > 0) return errors;

  const build = read(BUILD);
  const promotion = read(PROMOTION);
  errors.push(...buildErrors(build));
  errors.push(...promotionErrors(promotion));

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
  if (manualRows.length !== 1) {
    errors.push(`canonical Actions registry must have exactly one manual operation, observed=${manualRows.length}`);
  }

  const authority = JSON.parse(read(AUTHORITY));
  if (
    authority.production_mutation !== false
    || authority.production_ready !== false
    || authority.production_core_gate !== 'BLOCKED'
    || authority.architecture_complete !== false
  ) {
    errors.push('AR-11 operational workflow may not change canonical production authorization state');
  }
  const policy = authority.promotion_policy ?? {};
  if (
    policy.build_once !== true
    || policy.promotion_rebuild !== false
    || policy.opsctl_network !== false
    || policy.opsctl_credentials !== false
    || policy.opsctl_provider_mutation !== false
    || policy.production_execution_before_ar17 !== false
  ) {
    errors.push('AR-11 promotion policy invariants drifted');
  }
  if (requireCutover) {
    for (const relative of LEGACY_FILES) {
      if (existsSync(path.join(ROOT, relative))) {
        errors.push(`legacy D3 operational authority must be retired after Rust cutover: ${relative}`);
      }
    }
  }
  return errors;
}

function selfTest() {
  const promotion = read(PROMOTION);
  const build = read(BUILD);
  if (buildErrors(build).length !== 0 || promotionErrors(promotion).length !== 0) {
    throw new Error('canonical AR-11 operational workflow does not satisfy its own structural validator');
  }

  const leaked = promotion.replace(
    'permissions:\n      contents: read\n    outputs:',
    'permissions:\n      contents: read\n    env:\n      LEAKED_DEPLOY: ${{ secrets.CLOUDFLARE_API_TOKEN }}\n    outputs:',
  );
  if (!promotionErrors(leaked).some((error) => error.includes('referenced exactly once'))) {
    throw new Error('deploy-token leakage fixture unexpectedly passed');
  }

  const rebuild = promotion.replace(
    'Deploy exact Release Set v2 bits',
    'run: cargo build --release\n      - name: Deploy exact Release Set v2 bits',
  );
  if (!promotionErrors(rebuild).some((error) => error.includes('cargo build'))) {
    throw new Error('promotion rebuild fixture unexpectedly passed');
  }

  const acceptedSourceProof = 'test "$GITHUB_SHA" = "$main_sha"';
  if (!promotion.includes(acceptedSourceProof)) {
    throw new Error('historical Release Set fixture anchor is missing from canonical workflow');
  }
  const staleHeadEquality = promotion.replace(
    acceptedSourceProof,
    `${acceptedSourceProof}\n          test "$source_sha" = "$main_sha"`,
  );
  if (!promotionErrors(staleHeadEquality).some((error) => error.includes('test "$source_sha" = "$main_sha"'))) {
    throw new Error('historical Release Set current-head equality fixture unexpectedly passed');
  }

  const production = promotion.replace('environment: staging', 'environment: production');
  if (!promotionErrors(production).some((error) => error.includes('environment: production'))) {
    throw new Error('production activation fixture unexpectedly passed');
  }

  console.log('AR-11 structural release/promotion negative self-test passed.');
}

if (process.argv.includes('--self-test')) {
  selfTest();
  process.exit(0);
}

const errors = operationalErrors({ requireCutover: !process.argv.includes('--pre-cutover') });
if (errors.length > 0) {
  console.error(`AR-11 operational policy failed:\n${errors.map((error) => `- ${error}`).join('\n')}`);
  process.exit(1);
}
console.log('AR-11 durable Release Set and structural promotion policy passed.');
