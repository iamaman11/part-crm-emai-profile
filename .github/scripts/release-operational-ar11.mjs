#!/usr/bin/env node

import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';

const ROOT = process.cwd();
const BUILD = '.github/workflows/release-set-build.yml';
const PROMOTION = '.github/workflows/release-set-promotion.yml';
const TRANSPORT = '.github/workflows/ar11-fc6-operator-transport.yml';
const OPERATOR = '.github/scripts/ar11-fc6-operator.mjs';
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

function count(text, marker) { return text.split(marker).length - 1; }

function jobBlock(workflow, jobName) {
  const lines = workflow.split('\n');
  const start = lines.findIndex((line) => line === `  ${jobName}:`);
  if (start < 0) return '';
  let end = lines.length;
  for (let index = start + 1; index < lines.length; index += 1) {
    if (/^  [A-Za-z0-9_-]+:\s*$/.test(lines[index])) { end = index; break; }
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
    'CLOUDFLARE_API_TOKEN', 'CLOUDFLARE_DEPLOY_MANIFEST_JSON', 'wrangler deploy --env production',
    'environment: production', 'terraform',
  ], 'Release Set build'));
  return errors;
}

function operatorScriptErrors(operator) {
  const errors = [];
  errors.push(...requireMarkers(operator, [
    "const REPOSITORY = 'iamaman11/part-crm-emai-profile'",
    'const ISSUE_NUMBER = 399',
    "const WORKFLOW = 'release-set-promotion.yml'",
    "const TRANSPORT_WORKFLOW = 'AR-11 FC-6 Operator Transport'",
    "const PR_TITLE = 'AR-11 FC-6 ephemeral operator request'",
    "const PR_BRANCH_PREFIX = 'codex/ar11-fc6-request-'",
    "const PR_TRIGGER_PATH = 'docs/evidence/ar11-fc6-operator-request.json'",
    "const TRIGGER_WORKFLOW_NAME = 'Release Architecture Gate'",
    "const TRIGGER_WORKFLOW_PATH = '.github/workflows/release-architecture-gate.yml'",
    "run.status !== 'completed' || run.conclusion !== 'success'",
    'operator workflow_run must not bind multiple pull requests',
    'workflow_run fallback must resolve exactly one open exact-head pull request',
    "pull?.user?.login !== OWNER || pull?.author_association !== 'OWNER'",
    "pull?.draft !== true",
    'pull?.changed_files !== 1',
    "pull?.base?.ref !== 'main'",
    "pull?.head?.repo?.full_name !== REPOSITORY",
    'operator PR base must equal the currently protected main SHA',
    'operator PR must change exactly one data-only file',
    "kind !== 'AR11_FC6_EPHEMERAL_OPERATOR_TRIGGER'",
    "authority !== 'NONE'",
    'merge_authorized !== false',
    'confirmation does not bind target and expected current',
    'confirmation does not bind Release Set and source run',
    '/actions/workflows/${WORKFLOW}/dispatches',
    "ref: 'main'",
    "operation: 'rollback-negative'",
    'issue comments must not be an operator authority',
    'ordinary workflow_run unexpectedly became operator input',
    'operator trigger gate must complete successfully',
    'exactly one data-only file',
    'AR11_FC6_OPERATOR_AUDIT',
    'VALIDATED / DISPATCH PENDING',
    'FAILED / NO NEW AUTHORITY',
    'Reused existing canonical run',
    'operator must execute only inside',
    "run?.path === '.github/workflows/release-set-promotion.yml'",
  ], 'FC-6 bounded operator adapter'));
  errors.push(...forbidMarkers(operator, [
    'CLOUDFLARE_API_TOKEN', 'CLOUDFLARE_DEPLOY_MANIFEST_JSON', 'wrangler', 'deployments: write',
    'environment: production', 'terraform', 'pull.body',
  ], 'FC-6 bounded operator adapter'));
  return errors;
}

function transportErrors(transport) {
  const errors = [];
  errors.push(...requireMarkers(transport, [
    'name: AR-11 FC-6 Operator Transport',
    'workflow_run:\n    workflows: [Release Architecture Gate]\n    types: [completed]',
    "if: github.event.workflow_run.event == 'pull_request' && startsWith(github.event.workflow_run.head_branch, 'codex/ar11-fc6-request-')",
    'actions: write',
    'issues: write',
    'pull-requests: read',
    'ref: main',
    'fetch-depth: 1',
    'persist-credentials: false',
    'node .github/scripts/ar11-fc6-operator.mjs',
  ], 'FC-6 operator transport'));
  if (count(transport, 'workflow_run:') !== 1) errors.push('FC-6 operator transport must expose exactly one workflow_run trigger');
  if (count(transport, 'operator-entrypoint:') !== 1) errors.push('FC-6 operator transport must contain exactly one operator job');
  errors.push(...forbidMarkers(transport, [
    'workflow_dispatch:', 'issue_comment:', 'pull_request_target:', 'pull_request:\n',
    'secrets.CLOUDFLARE_', 'CLOUDFLARE_API_TOKEN', 'CLOUDFLARE_DEPLOY_MANIFEST_JSON',
    'environment: staging', 'environment: production', 'deployments: write', 'wrangler', 'terraform',
    'ref: ${{ github.event.workflow_run.head_sha }}',
  ], 'FC-6 operator transport'));
  return errors;
}

function promotionErrors(promotion) {
  const errors = [];
  errors.push(...requireMarkers(promotion, [
    'workflow_dispatch:', 'operation:', 'release_set_id:', 'expected_current_release_set_id:', 'source_run_id:',
    'request_id:', 'confirmation:', 'concurrency:\n  group: release-set-promotion-staging',
    "'{schema_version:1,release_set_id:$release_set_id,expected_current:$expected_current,promotion_id:$promotion_id,decision:$decision,preflight_sha256:$preflight_sha256,plan_sha256:$plan_sha256}'",
  ], 'Release Set promotion'));
  if (count(promotion, 'workflow_dispatch:') !== 1) errors.push('Release Set promotion must expose exactly one manual dispatch surface');
  if (count(promotion, 'secrets.CLOUDFLARE_API_TOKEN') !== 1) errors.push('deploy-capable Cloudflare token must be referenced exactly once, inside mutation executor');
  errors.push(...forbidMarkers(promotion, [
    'workflow_run:', 'issue_comment:', 'pull_request_target:', 'pull_request:\n', 'operator-entrypoint:',
    'test "$source_sha" = "$main_sha"',
    'preflight_sha256256', 'environment: production', 'TARGET_PROFILE: production-',
    'mailbox-secret-resolver-promotion.py', '_mailbox_secret_resolver_promotion_core.py',
    'wrangler d1 create', 'wrangler r2 bucket create', 'wrangler queues create',
    'CLOUDFLARE_RESOLVER_SECRETS_JSON', 'CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON', 'terraform',
  ], 'Release Set promotion'));

  const resolve = jobBlock(promotion, 'resolve-verify');
  const observe = jobBlock(promotion, 'observe-preflight');
  const mutate = jobBlock(promotion, 'mutate');
  const post = jobBlock(promotion, 'post-verify');
  const rollbackNegative = jobBlock(promotion, 'rollback-negative-evidence');
  for (const [name, block] of Object.entries({ 'resolve-verify': resolve, 'observe-preflight': observe, mutate, 'post-verify': post, 'rollback-negative-evidence': rollbackNegative })) {
    if (!block) errors.push(`Release Set promotion is missing structural job ${name}`);
  }
  if (errors.some((error) => error.includes('missing structural job'))) return errors;

  errors.push(...requireMarkers(resolve, [
    "if: github.event_name == 'workflow_dispatch' && inputs.operation == 'promote'",
    'test "${{ inputs.operation }}" = promote', 'test -z "${{ inputs.source_run_id }}"',
    'Prove target source was accepted protected main', "test \"$(jq -r '.protected' \"$RUNNER_TEMP/protected-main.json\")\" = true",
    'compare/$source_sha...$main_sha', 'Checkout current protected-main policy authority',
    'Checkout exact target source as read-only provenance input', 'path: target-source', 'accepted-source-evidence-ar11.py',
    'gh release download "$RELEASE_SET_ID"', 'for name in release-set.json control-plane.tar secret-resolver.tar runtime-bundle.tar profile-bridge.zip',
    'release verify', '--source-root "$GITHUB_WORKSPACE/target-source"',
    'test "$(find "$asset_root" -maxdepth 1 -type f | wc -l)" -eq 5', '.source_accepted == true',
  ], 'promotion phase 1 resolve+verify'));
  errors.push(...forbidMarkers(resolve, ['secrets.CLOUDFLARE_', 'environment: staging', 'wrangler deploy', 'wrangler d1 execute', 'curl --silent --show-error --output "$RUNNER_TEMP/deployments-api.json"'], 'promotion phase 1 resolve+verify'));

  errors.push(...requireMarkers(observe, [
    'needs: resolve-verify', 'environment: staging', 'TARGET_PROFILE: rehearsal-core-v1', 'TARGET_ENVIRONMENT: staging',
    'secrets.CLOUDFLARE_OBSERVE_API_TOKEN', 'Observe current provider state without mutation',
    'Verify current/known-good immutable Release Set before rollback evaluation', 'Build metadata-only DeploymentSnapshot v2',
    'deployment-snapshot-ar11.py', 'release compatibility', 'promotion plan', 'promotion preflight', '--expected-current "$EXPECTED_CURRENT"',
    'mutation-fence.json', 'Materialize flat metadata-only preflight artifact contract',
    'cp "$RUNNER_TEMP/release-policy-input/release-set.json" "$RUNNER_TEMP/release-set.json"',
    'cp "$RUNNER_TEMP/release-policy-input/accepted-source-evidence.json" "$RUNNER_TEMP/accepted-source-evidence.json"',
    '${{ runner.temp }}/release-set.json', '${{ runner.temp }}/accepted-source-evidence.json',
  ], 'promotion phase 2 observe+preflight'));
  errors.push(...forbidMarkers(observe, ['secrets.CLOUDFLARE_API_TOKEN', 'wrangler deploy', 'deployments: write', '${{ runner.temp }}/release-policy-input/release-set.json\n', '${{ runner.temp }}/release-policy-input/accepted-source-evidence.json\n'], 'promotion phase 2 observe+preflight'));

  errors.push(...requireMarkers(mutate, [
    'needs: [resolve-verify, observe-preflight]', "if: needs.observe-preflight.outputs.decision == 'PLAN'", 'environment: staging', 'deployments: write',
    'Checkout exact target source before mutation verification', 'Download and bind exact preflight authority before provider use',
    'Re-verify fence and exact immutable Release Set before credentials', 'mutation-fence.json', "'.preflight_sha256'", "'.plan_sha256'",
    'cmp --silent "$asset_root/release-set.json" "$preflight_root/release-set.json"', 'release verify',
    'Activate deploy credential after READY and exact-byte verification', 'DEPLOY_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}',
    'Re-observe expected-current fence and deploy exact Release Set v2 bits', 'deployment-identity-ar11.py',
    'test "$current_id" = "$EXPECTED_CURRENT"', 'gh release download "$RELEASE_SET_ID"', '--dry-run',
    '--message "release_set=$RELEASE_SET_ID profile=rehearsal-core-v1"',
  ], 'promotion phase 3 protected mutation'));
  errors.push(...forbidMarkers(mutate, ['worker-build --release', 'cargo build', 'npm run build', 'release-set-ar11.py build', 'release compatibility', 'promotion plan', 'promotion preflight'], 'promotion phase 3 protected mutation'));
  const nativeVerify = mutate.indexOf('Re-verify fence and exact immutable Release Set before credentials');
  const deployCredential = mutate.indexOf('DEPLOY_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}');
  const reobserveFence = mutate.indexOf('Re-observe expected-current fence and deploy exact Release Set v2 bits');
  const currentFence = mutate.indexOf('test "$current_id" = "$EXPECTED_CURRENT"', reobserveFence);
  const actualDeploy = mutate.indexOf('--message "release_set=$RELEASE_SET_ID profile=rehearsal-core-v1"', reobserveFence);
  if (!(nativeVerify >= 0 && deployCredential > nativeVerify && reobserveFence > deployCredential && currentFence > reobserveFence && actualDeploy > currentFence)) {
    errors.push('mutation credential/fence ordering must be native verify -> credential activation -> expected-current re-observe -> deploy');
  }

  errors.push(...requireMarkers(post, [
    'needs: [resolve-verify, observe-preflight, mutate]', "needs.mutate.result == 'success' || needs.mutate.result == 'skipped'",
    'environment: staging', 'secrets.CLOUDFLARE_OBSERVE_API_TOKEN', 'deployment-snapshot-ar11.py', 'promotion verify', '.verified == true',
  ], 'promotion phase 4 post-deploy observation'));
  errors.push(...forbidMarkers(post, ['secrets.CLOUDFLARE_API_TOKEN', 'deployments: write', 'wrangler deploy'], 'promotion phase 4 post-deploy observation'));

  errors.push(...requireMarkers(rollbackNegative, [
    "if: github.event_name == 'workflow_dispatch' && inputs.operation == 'rollback-negative'", 'actions: read',
    'Validate evidence-only intent and live source run', '.status == "completed"', '.conclusion == "success"',
    '.path == ".github/workflows/release-set-promotion.yml"', 'gh run download "$SOURCE_RUN_ID"',
    'Download live A-to-A preflight evidence without provider credentials', '.decision == "NO_CHANGE"', '.rollback_compatibility == "COMPATIBLE"',
    "jq '.catalog_schema_revision = null'", 'Native rollback preflight must block UNKNOWN before credentials',
    '--known-good-release-set "$live/release-set.json"', '.decision == "BLOCKED"', '.rollback_compatibility == "UNKNOWN"',
    'ROLLBACK_COMPATIBILITY_UNKNOWN', '.credential_values_accessed == false', '.provider_mutation_executed == false', '.mutation_executed == false',
    'Upload credential-free rollback-negative evidence',
  ], 'promotion rollback-negative evidence'));
  errors.push(...forbidMarkers(rollbackNegative, ['secrets.CLOUDFLARE_', 'CLOUDFLARE_API_TOKEN', 'CLOUDFLARE_DEPLOY_MANIFEST_JSON', 'environment: staging', 'deployments: write', 'wrangler deploy'], 'promotion rollback-negative evidence'));

  return errors;
}

function operationalErrors({ requireCutover = true } = {}) {
  const errors = [];
  for (const relative of [BUILD, PROMOTION, TRANSPORT, OPERATOR, REGISTRY, AUTHORITY]) {
    if (!existsSync(path.join(ROOT, relative))) errors.push(`missing AR-11 operational authority: ${relative}`);
  }
  if (errors.length > 0) return errors;

  const build = read(BUILD);
  const promotion = read(PROMOTION);
  const transport = read(TRANSPORT);
  const operator = read(OPERATOR);
  errors.push(...buildErrors(build));
  errors.push(...promotionErrors(promotion));
  errors.push(...transportErrors(transport));
  errors.push(...operatorScriptErrors(operator));

  const registry = JSON.parse(read(REGISTRY));
  const workflows = registry.active_registrations ?? [];
  const buildRows = workflows.filter((row) => row.path === BUILD);
  const promotionRows = workflows.filter((row) => row.path === PROMOTION);
  const transportRows = workflows.filter((row) => row.path === TRANSPORT);
  if (buildRows.length !== 1 || buildRows[0].category !== 'PERMANENT_REQUIRED') errors.push('Release Set build must appear exactly once as PERMANENT_REQUIRED');
  if (promotionRows.length !== 1 || promotionRows[0].category !== 'CURRENT_MANUAL_OPERATION') errors.push('Release Set promotion must be the single canonical CURRENT_MANUAL_OPERATION');
  if (transportRows.length !== 1 || transportRows[0].category !== 'PERMANENT_REQUIRED') errors.push('FC-6 operator transport must appear exactly once as PERMANENT_REQUIRED and may not be mutation authority');
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
    for (const relative of LEGACY_FILES) if (existsSync(path.join(ROOT, relative))) errors.push(`legacy D3 operational authority must be retired after Rust cutover: ${relative}`);
  }
  return errors;
}

function selfTest() {
  const promotion = read(PROMOTION);
  const transport = read(TRANSPORT);
  const build = read(BUILD);
  const operator = read(OPERATOR);
  if (buildErrors(build).length !== 0 || promotionErrors(promotion).length !== 0 || transportErrors(transport).length !== 0 || operatorScriptErrors(operator).length !== 0) {
    throw new Error('canonical AR-11 operational workflow does not satisfy its own structural validator');
  }
  execFileSync(process.execPath, [path.join(ROOT, OPERATOR), '--self-test'], { cwd: ROOT, stdio: 'inherit' });

  const leaked = promotion.replace('permissions:\n      contents: read\n    outputs:', 'permissions:\n      contents: read\n    env:\n      LEAKED_DEPLOY: ${{ secrets.CLOUDFLARE_API_TOKEN }}\n    outputs:');
  if (!promotionErrors(leaked).some((error) => error.includes('referenced exactly once'))) throw new Error('deploy-token leakage fixture unexpectedly passed');

  const rebuild = promotion.replace('Re-observe expected-current fence and deploy exact Release Set v2 bits', 'run: cargo build --release\n      - name: Re-observe expected-current fence and deploy exact Release Set v2 bits');
  if (!promotionErrors(rebuild).some((error) => error.includes('cargo build'))) throw new Error('promotion rebuild fixture unexpectedly passed');

  const mutate = jobBlock(promotion, 'mutate');
  const staleMutate = mutate.replace('test "$current_id" = "$EXPECTED_CURRENT"', 'test -n "$current_id"');
  if (!promotionErrors(promotion.replace(mutate, staleMutate)).some((error) => error.includes('credential/fence ordering'))) throw new Error('mutation stale-fence bypass fixture unexpectedly passed');

  const earlyCredential = promotion.replace('DEPLOY_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}', 'DEPLOY_TOKEN: EARLY_FIXTURE_REMOVED').replace('Re-verify fence and exact immutable Release Set before credentials', 'DEPLOY_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}\n      - name: Re-verify fence and exact immutable Release Set before credentials');
  if (!promotionErrors(earlyCredential).some((error) => error.includes('credential/fence ordering'))) throw new Error('early deploy credential fixture unexpectedly passed');

  const acceptedSourceProof = 'test "$GITHUB_SHA" = "$main_sha"';
  if (!promotion.includes(acceptedSourceProof)) throw new Error('historical Release Set fixture anchor is missing from canonical workflow');
  const staleHeadEquality = promotion.replace(acceptedSourceProof, `${acceptedSourceProof}\n          test "$source_sha" = "$main_sha"`);
  if (promotionErrors(staleHeadEquality).length === 0) throw new Error('historical Release Set current-head equality fixture unexpectedly passed');

  const production = promotion.replace('environment: staging', 'environment: production');
  if (!promotionErrors(production).some((error) => error.includes('environment: production'))) throw new Error('production activation fixture unexpectedly passed');

  const broadOperation = promotion.replace("if: github.event_name == 'workflow_dispatch' && inputs.operation == 'promote'", "if: github.event_name == 'workflow_dispatch'");
  if (!promotionErrors(broadOperation).some((error) => error.includes('promotion phase 1'))) throw new Error('operation isolation fixture unexpectedly passed');

  const reintroducedListener = promotion.replace('  workflow_dispatch:', '  issue_comment:\n    types: [created]\n  workflow_run:\n    workflows: [Release Architecture Gate]\n    types: [completed]\n  workflow_dispatch:');
  if (!promotionErrors(reintroducedListener).some((error) => error.includes('workflow_run') || error.includes('issue_comment'))) throw new Error('promotion event-listener reintroduction fixture unexpectedly passed');

  const fenceTypo = promotion.replace('preflight_sha256:$preflight_sha256', 'preflight_sha256256:$preflight_sha256');
  if (!promotionErrors(fenceTypo).some((error) => error.includes('preflight_sha256256') || error.includes('mutation-fence'))) throw new Error('mutation fence key typo fixture unexpectedly passed');

  const transportSecretLeak = transport.replace('GITHUB_TOKEN: ${{ github.token }}', 'LEAKED_DEPLOY: ${{ secrets.CLOUDFLARE_API_TOKEN }}\n      GITHUB_TOKEN: ${{ github.token }}');
  if (!transportErrors(transportSecretLeak).some((error) => error.includes('CLOUDFLARE_API_TOKEN'))) throw new Error('operator transport credential leakage fixture unexpectedly passed');

  const directPrTransport = transport.replace('workflow_run:\n    workflows: [Release Architecture Gate]\n    types: [completed]', 'pull_request_target:\n    types: [opened]');
  if (!transportErrors(directPrTransport).some((error) => error.includes('pull_request_target') || error.includes('workflow_run'))) throw new Error('direct PR write-token transport fixture unexpectedly passed');

  const untrustedCheckout = transport.replace('ref: main', 'ref: ${{ github.event.workflow_run.head_sha }}');
  if (!transportErrors(untrustedCheckout).some((error) => error.includes('workflow_run.head_sha') || error.includes('ref: main'))) throw new Error('operator untrusted-head checkout fixture unexpectedly passed');

  const negativeBlock = jobBlock(promotion, 'rollback-negative-evidence');
  const uncontrolledNegative = promotion.replace(negativeBlock, negativeBlock.replace("jq '.catalog_schema_revision = null'", "jq '.catalog_schema_revision = .catalog_schema_revision'"));
  if (!promotionErrors(uncontrolledNegative).some((error) => error.includes('rollback-negative evidence'))) throw new Error('rollback-negative controlled-condition bypass unexpectedly passed');

  const nestedArtifact = promotion.replace('${{ runner.temp }}/release-set.json\n            ${{ runner.temp }}/accepted-source-evidence.json', '${{ runner.temp }}/release-policy-input/release-set.json\n            ${{ runner.temp }}/release-policy-input/accepted-source-evidence.json');
  if (!promotionErrors(nestedArtifact).some((error) => error.includes('phase 2 observe+preflight'))) throw new Error('nested preflight artifact regression unexpectedly passed');

  console.log('AR-11 isolated transport and structural release/promotion negative self-test passed.');
}

if (process.argv.includes('--self-test')) { selfTest(); process.exit(0); }
const errors = operationalErrors({ requireCutover: !process.argv.includes('--pre-cutover') });
if (errors.length > 0) {
  console.error(`AR-11 operational policy failed:\n${errors.map((error) => `- ${error}`).join('\n')}`);
  process.exit(1);
}
console.log('AR-11 durable Release Set, isolated operator transport, and structural promotion policy passed.');
