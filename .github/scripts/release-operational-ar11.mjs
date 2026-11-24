#!/usr/bin/env node

import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { spawnSync } from 'node:child_process';

const ROOT = process.cwd();
const BUILD = '.github/workflows/release-set-build.yml';
const PROMOTION = '.github/workflows/release-set-promotion.yml';
const CAMOUFOX = '.github/workflows/camoufox-runtime-gate.yml';
const CAMOUFOX_SCOPE = 'scripts/classify-camoufox-runtime-scope.py';
const AUTHORITY = 'architecture/release-architecture-ar11.json';
const ASSET_MATERIALIZER = 'scripts/release-set-assets-ar11.sh';
const EXPECTED_PROMOTION_RUN_NAME = "run-name: AR11 Release Set Promotion — ${{ github.event_name == 'workflow_run' && 'READ-ONLY PREFLIGHT' || github.event_name == 'workflow_dispatch' && format('STAGING PROMOTION by {0}', github.actor) || 'INTERNAL ROLLBACK-NEGATIVE EVIDENCE' }}";
const READ_ONLY_TERMINAL_DISPOSITION = '(.status == "READ_ONLY_READY" or .status == "NOOP" or .status == "BLOCKED")';
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

function stepBlock(job, stepName) {
  const marker = `      - name: ${stepName}`;
  const start = job.indexOf(marker);
  if (start < 0) return '';
  const next = job.indexOf('\n      - name: ', start + marker.length);
  return job.slice(start, next < 0 ? job.length : next);
}

function logicalShellLines(source) {
  return source.replace(/\\\n\s*/g, ' ').split('\n').map((line) => line.trim()).filter(Boolean);
}

function secretObservationErrors(source, label) {
  const commands = logicalShellLines(source).filter((line) => line.includes('wrangler@4.94.0 secret list'));
  const errors = [];
  if (commands.length === 0) errors.push(`${label} has no Worker secret-name observation`);
  for (const command of commands) {
    for (const required of ['--format json', '--config "$WRANGLER_CONFIG"', '--env staging']) {
      if (!command.includes(required)) errors.push(`${label} secret observation lacks ${required}`);
    }
    if (/\s--name(?:\s|=)/.test(command)) {
      errors.push(`${label} secret observation redundantly overrides the env-owned Worker name`);
    }
    if (/\s(?:put|bulk|delete)\b/.test(command)) errors.push(`${label} secret observation contains mutation semantics`);
  }
  return errors;
}

function readyMutationByteIdentityErrors(mutate) {
  const expected = 'cmp --silent "$release_root/release-set.json" "$RUNNER_TEMP/ready/release-set.json"';
  const matches = logicalShellLines(mutate).filter((line) => line === expected);
  if (matches.length !== 1) {
    return [`protected mutation must byte-compare the freshly materialized Release Set manifest to the exact bound READY manifest once; observed=${matches.length}`];
  }
  return [];
}

function immediateMutationFenceErrors(mutate) {
  const lines = logicalShellLines(mutate);
  const workerFence = 'test "$(jq -r \' .release_set_id\' "$RUNNER_TEMP/mutation-current-identity.json")" = "$EXPECTED_CURRENT"'.replace("' .release", "'.release");
  const profileFence = 'test "$(jq -r \' .capability_profile_id\' "$RUNNER_TEMP/mutation-current-identity.json")" = "$EXPECTED_CURRENT_PROFILE"'.replace("' .capability", "'.capability");
  const d1Fence = 'cmp --silent "$RUNNER_TEMP/mutation-d1-names.json" "$RUNNER_TEMP/ready/d1-names-after.json"';
  const workerMatches = lines.filter((line) => line.includes(workerFence));
  const profileMatches = lines.filter((line) => line.includes(profileFence));
  const d1Matches = lines.filter((line) => line.includes(d1Fence));
  const errors = [];
  if (workerMatches.length !== 1) {
    errors.push(`protected mutation expected-current Worker fence must exist exactly once immediately before deploy; observed=${workerMatches.length}`);
  }
  if (profileMatches.length !== 1) {
    errors.push(`protected mutation expected-current capability-profile fence must exist exactly once immediately before deploy; observed=${profileMatches.length}`);
  }
  if (d1Matches.length !== 1) {
    errors.push(`protected mutation READY-bound D1 ledger fence must exist exactly once immediately before deploy; observed=${d1Matches.length}`);
  }

  const fence = stepBlock(mutate, 'Re-fence Worker identity and D1 head immediately before mutation');
  if (!fence) {
    errors.push('protected mutation exact-current fence step is missing');
    return errors;
  }
  errors.push(...requireMarkers(fence, [
    'CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_OBSERVE_API_TOKEN }}',
    'kind:"AR11_EXACT_CURRENT_FENCE"',
    'credential_class:"READ_ONLY_OBSERVE"',
    'provider_mutation:false',
    'production_mutation:false',
    'sub_fences:{deployment_read:$deployment_read,release_set_match:$release_set_match,capability_profile_match:$capability_profile_match,d1_read:$d1_read,d1_match:$d1_match}',
    '> "$RUNNER_TEMP/exact-current-fence.json"',
  ], 'protected mutation read-only exact-current fence'));
  errors.push(...forbidMarkers(fence, [
    'secrets.CLOUDFLARE_API_TOKEN }}',
    'wrangler deploy --',
  ], 'protected mutation read-only exact-current fence'));
  errors.push(...requireMarkers(mutate, [
    'exact_current_fence=\'null\'',
    'exact_current_fence:$exact_current_fence',
    '.kind == "AR11_EXACT_CURRENT_FENCE"',
    '.credential_class == "READ_ONLY_OBSERVE"',
  ], 'protected mutation lossless exact-current fence projection'));
  return errors;
}

function deployCredentialBoundaryErrors(promotion, promote) {
  const token = 'CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}';
  const readyBind = 'Bind previously verified READY evidence before provider credentials';
  const dryRun = 'Render mutation overlay and prove exact bits dry-run without deploy credential';
  const promotionCount = promotion.split(token).length - 1;
  const promoteCount = promote.split(token).length - 1;
  const readyIndex = promote.indexOf(readyBind);
  const dryRunIndex = promote.indexOf(dryRun);
  const fenceIndex = promote.indexOf('Re-fence Worker identity and D1 head immediately before mutation');
  const deployIndex = promote.indexOf('Deploy exact Release Set v3 bits after all fences');
  const deployStep = stepBlock(promote, 'Deploy exact Release Set v3 bits after all fences');
  const errors = [];
  if (promotionCount !== 1 || promoteCount !== 1 || !deployStep.includes(token)) {
    errors.push(`deploy-capable credential must be referenced exactly once and only by the one deploy step; workflow=${promotionCount} promote=${promoteCount}`);
  }
  if (!(readyIndex >= 0 && dryRunIndex > readyIndex && fenceIndex > dryRunIndex && deployIndex > fenceIndex)) {
    errors.push('deploy-capable credential must remain behind bound READY, exact-bits dry-run and immediate predeploy fence');
  }
  const earlierSteps = promote.slice(0, deployIndex >= 0 ? deployIndex : promote.length);
  if (earlierSteps.includes(token) || promote.includes('echo "CLOUDFLARE_API_TOKEN=$DEPLOY_TOKEN" >> "$GITHUB_ENV"')) {
    errors.push('deploy credential must be scoped to deploy step, never persist across read-only observation or post-verification');
  }
  return errors;
}

function legacyAuthorityErrors(exists = existsSync) {
  const errors = [];
  for (const relative of LEGACY_FILES) {
    if (exists(path.join(ROOT, relative))) {
      errors.push(`legacy D3 operational authority must be retired after Rust cutover: ${relative}`);
    }
  }
  return errors;
}

function buildErrors(build) {
  const errors = [];
  errors.push(...requireMarkers(build, [
    'branches:\n      - main',
    'Build immutable cloud components once',
    'Build immutable Windows Profile Bridge component',
    'Finalize one content-addressed Release Set v3 through opsctl',
    'kind: "RELEASE_FINALIZE_REQUEST"',
    'release finalize --request-json',
    'accepted-source-evidence-ar11.py',
    'group: release-set-build-main',
    'python scripts/github-release-publication.py probe',
    'python scripts/github-release-publication.py publish',
    'Publish once or prove byte-identical replay',
  ], 'Release Set build'));
  errors.push(...forbidMarkers(build, [
    'release-set-ar11.py build',
    'CLOUDFLARE_API_TOKEN',
    'CLOUDFLARE_DEPLOY_MANIFEST_JSON',
    'environment: production',
    'terraform',
    'gh release create "$env:RUNTIME_RELEASE_ID"',
    'gh release upload "$env:RUNTIME_RELEASE_ID"',
    'gh release create "$RELEASE_ID"',
    'gh release upload "$RELEASE_ID"',
  ], 'Release Set build'));
  return errors;
}

function authorityErrors(authority) {
  const errors = [];
  const policy = authority?.promotion_policy ?? {};
  const requiredTrue = [
    'build_once',
    'read_only_ready_before_manual_dispatch',
    'ready_to_mutate_evidence_required_for_dispatch',
    'dispatch_intent_binds_ready_evidence',
    'workflow_dispatch_is_staging_authorization',
    'owner_only_manual_dispatch',
    'staging_environment_main_only',
    'expected_current_refence_immediately_before_mutation',
    'd1_refence_immediately_before_mutation',
    'accepted_main_heavy_runtime_replay',
    'accepted_main_exact_tree_pr_evidence',
  ];
  for (const key of requiredTrue) {
    if (policy[key] !== true) errors.push(`AR-11 promotion policy requires ${key}=true`);
  }
  for (const obsolete of ['read_only_ready_before_mutation_authorization', 'ready_to_mutate_evidence_required_for_authorization', 'authorization_binds_ready_evidence']) {
    if (obsolete in policy) errors.push(`AR-11 promotion policy retains obsolete comment-authorization field ${obsolete}`);
  }
  if (policy.promotion_rebuild !== false || policy.opsctl_provider_mutation !== false) {
    errors.push('AR-11 promotion must remain no-rebuild with provider mutation outside opsctl');
  }
  if (policy.pr_change_aware_runtime_gate !== 'FAIL_CLOSED_PROVEN_RUNTIME_UNRELATED') {
    errors.push('AR-11 PR runtime gate policy must be fail-closed with only mechanically proven runtime-unrelated cheap paths');
  }
  if (policy.pr_runtime_scope_classifier !== CAMOUFOX_SCOPE) {
    errors.push('AR-11 PR runtime gate must name the canonical Camoufox scope classifier');
  }
  if (policy.pr_runtime_unknown_path_policy !== 'HEAVY') {
    errors.push('AR-11 PR runtime gate must classify unknown paths as HEAVY');
  }
  if (policy.pr_runtime_accepted_main_policy !== 'EXACT_TREE_PR_PROOF_OR_HEAVY') {
    errors.push('AR-11 accepted-main runtime policy requires proven PR tree or full main replay');
  }
  return errors;
}

function promotionErrors(promotion) {
  const errors = [];
  const runNameLine = promotion.split('\n').find((line) => line.startsWith('run-name: '));
  if (runNameLine !== EXPECTED_PROMOTION_RUN_NAME) {
    errors.push('promotion run-name must distinguish read-only preflight, actor-attributed manual staging dispatch, and internal rollback-negative evidence');
  }
  errors.push(...requireMarkers(promotion, [
    'workflow_run:',
    '- Release Set Build',
    'workflow_dispatch:',
    'workflow_call:',
    'concurrency:',
    'group: release-set-promotion-staging',
    'cancel-in-progress: false',
  ], 'Release Set promotion'));
  if ((promotion.match(/\n  workflow_dispatch:/g) ?? []).length !== 1) errors.push('promotion must expose exactly one zero-field manual dispatch');
  const trigger = promotion.split('\npermissions:', 1)[0];
  const manual = trigger.split('\n  workflow_dispatch:', 2)[1]?.split('\n  workflow_call:', 1)[0] ?? '';
  if (manual.includes('inputs:')) errors.push('manual promotion dispatch must remain zero-input');

  errors.push(...forbidMarkers(promotion, [
    'pull_request_target:',
    'issue_comment:',
    'environment: production',
    'TARGET_PROFILE: production-',
    'profile=rehearsal-core-v1',
    'wrangler d1 create',
    'wrangler r2 bucket create',
    'wrangler queues create',
    'CLOUDFLARE_RESOLVER_SECRETS_JSON',
    'CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON',
    'terraform',
  ], 'Release Set promotion'));

  const route = jobBlock(promotion, 'route');
  const ready = jobBlock(promotion, 'preflight-ready');
  const promote = jobBlock(promotion, 'promote');
  const manualOutcome = jobBlock(promotion, 'manual-outcome');
  const rollback = jobBlock(promotion, 'rollback-negative-evidence');
  for (const [name, block] of Object.entries({ route, 'preflight-ready': ready, promote, 'manual-outcome': manualOutcome, 'rollback-negative-evidence': rollback })) {
    if (!block) errors.push(`Release Set promotion is missing structural job ${name}`);
  }
  for (const retired of ['resolve-preflight', 'resolve-verify', 'mutate', 'post-verify']) {
    if (jobBlock(promotion, retired)) errors.push(`Release Set promotion retains redundant cold-runner job ${retired}`);
  }
  if (errors.some((error) => error.includes('missing structural job'))) return errors;

  errors.push(...requireMarkers(route, [
    'READY_SOURCE_SHA: ${{ github.event.workflow_run.head_sha }}',
    'READY_SOURCE_EVENT: ${{ github.event.workflow_run.event }}',
    "echo 'mode=preflight'",
    "echo 'mode=promote'",
    "echo 'mode=rollback-negative'",
    "if: github.event_name != 'workflow_dispatch' || github.actor == github.repository_owner",
  ], 'promotion invocation router'));
  errors.push(...forbidMarkers(route, ['secrets.', 'wrangler deploy', 'environment: staging'], 'promotion invocation router'));

  errors.push(...requireMarkers(ready, [
    'Resolve unique immutable Release Set for exact protected main',
    'test "$SOURCE_SHA" = "$main_sha"',
    'test "$(jq \'length\' "$RUNNER_TEMP/main-release-sets.json")" = 1',
    'Verify immutable target before any provider credential',
    'release verify',
  ], 'pre-authorization target resolver'));
  errors.push(...forbidMarkers(ready, ['issues: read', 'AUTHORITY_COMMENT', 'secrets.CLOUDFLARE_API_TOKEN', 'wrangler deploy'], 'pre-authorization target resolver'));

  errors.push(...requireMarkers(ready, [
    "if: needs.route.outputs.mode == 'preflight'",
    'secrets.CLOUDFLARE_OBSERVE_API_TOKEN',
    'Observe existing Worker, D1 head, resources and secret bindings',
    'test "$http_code" = 200',
    'deployment-identity-ar11.py',
    'd1-names-before.json',
    'deployment-snapshot-ar11.py',
    'current_profile="$(jq -er \' .capability_profile_id'.replace("' .capability", "'.capability"),
    '--root . promotion plan --release-set "$TARGET_RELEASE_MANIFEST"',
    '--source-root . --profile "$TARGET_PROFILE" --environment "$TARGET_ENVIRONMENT"',
    '.requested_target_capability_profile_id == $requested',
    '.admission_mode == "DIRECT" or .admission_mode == "PREREQUISITE_RELEASE_BRIDGE"',
    'target_profile="$(jq -er \' .target_capability_profile_id'.replace("' .target", "'.target"),
    'admission_mode="$(jq -er \' .admission_mode'.replace("' .admission", "'.admission"),
    '--source-root . --profile "$target_profile" --environment "$TARGET_ENVIRONMENT"',
    '.target_capability_profile_id == $effective',
    '.admission_mode == $mode',
    'promotion plan',
    'promotion preflight',
    '.schema_version == 2 and .command == "promotion.preflight"',
    'recovery_strategy:$recovery_strategy',
    '--arg recovery_strategy "$(jq -er',
    'Prove bounded Worker and D1 quiescence',
    'sleep 30',
    'cmp --silent "$RUNNER_TEMP/worker-binding-before.json" "$RUNNER_TEMP/worker-binding-after.json"',
    'cmp --silent "$RUNNER_TEMP/d1-names-before.json" "$RUNNER_TEMP/d1-names-after.json"',
    'kind:"AR11_READY_TO_MUTATE"',
    'target_capability_profile_id:$target_profile',
    'expected_current_capability_profile_id:$expected_current_profile',
    'admission_mode:$admission_mode',
    '--arg target_profile "${{ steps.policy.outputs.target_profile }}"',
    '--arg admission_mode "${{ steps.policy.outputs.admission_mode }}"',
    'secret_bindings_verified:true',
    'provider_mutation:false',
    'production_mutation:false',
    'ar11-ready-to-mutate-$RELEASE_SET_ID-$SOURCE_SHA',
    'Terminalize one lossless AR11 OperationalOutcome',
    'promotion-operational-outcome-ar11.py',
    '--profile-id "$effective_target_profile"',
    '--output "$RUNNER_TEMP/promotion-operational-outcome.json"',
    'Upload terminal AR11 OperationalOutcome evidence',
    'name: ar11-operational-outcome-${{ github.run_id }}-${{ github.run_attempt }}',
    'Enforce terminal AR11 disposition after evidence publication',
    '.contract == "PROMOTION_OPERATOR_OUTCOME_V1"',
    READ_ONLY_TERMINAL_DISPOSITION,
    '.authorization_state == "NOT_AUTHORIZED_READ_ONLY"',
    '.provider_mutation_started == false',
    '.provider_mutation_executed == false',
    '.production_mutation_executed == false',
    '.effect_state == "EXACT_NO_EFFECT"',
  ], 'pre-authorization READY proof'));
  errors.push(...forbidMarkers(ready, [
    'secrets.CLOUDFLARE_API_TOKEN }}',
    'deployments: write',
    'WORKER_PROMOTION_AUTHORIZATION_',
    'AUTHORITY_COMMENT',
    '--message "release_set=',
    '.status == "INFRASTRUCTURE_FAILURE"',
    'if [ "$current_id" != "$RELEASE_SET_ID" ]; then',
  ], 'pre-authorization READY proof'));
  errors.push(...secretObservationErrors(ready, 'pre-authorization READY proof'));
  const readOnlyTerminalize = ready.indexOf('Terminalize one lossless AR11 OperationalOutcome');
  const readOnlyUpload = ready.indexOf('Upload terminal AR11 OperationalOutcome evidence');
  const readOnlyEnforce = ready.indexOf('Enforce terminal AR11 disposition after evidence publication');
  if (!(readOnlyTerminalize >= 0 && readOnlyUpload > readOnlyTerminalize && readOnlyEnforce > readOnlyUpload)) {
    errors.push('read-only AR11 must terminalize -> publish evidence -> enforce final disposition');
  }
  const preflightTerminal = stepBlock(ready, 'Terminalize one lossless AR11 OperationalOutcome');
  errors.push(...requireMarkers(preflightTerminal, [
    'steps.resolve.outcome',
    'failed_boundary="ACCEPTED_MAIN_RELEASE_RESOLUTION"',
    'steps.target_verify.outcome',
    'failed_boundary="TARGET_RELEASE_VERIFY"',
  ], 'pre-authorization failure localization'));

  errors.push(...requireMarkers(promote, [
    'environment: staging',
    'Bind the explicit GitHub dispatch to unique accepted-main READY evidence',
    'SELECTED_MAIN_SHA:',
    'SELECTED_RELEASE_SET_ID:',
    'actions/artifacts?name=$ready_artifact_name',
    '.total_count == 1 and (.artifacts | length) == 1',
    '.artifacts[0].workflow_run.head_sha == $sha',
    'candidate-ready-run.json',
    'READY evidence does not match the exact accepted main and Release Set',
    'STAGING_PROMOTION_DISPATCH_INTENT',
    "'github.workflow_dispatch'",
    'READY_RUN_ID',
    'READY_ARTIFACT_NAME',
    'READY_EVIDENCE_SHA256',
    'dispatch_digest',
    'Verify exact downloaded READY_TO_MUTATE evidence',
    'native_recovery="$(jq -er',
    '.recovery_strategy == $recovery',
    "jq -er '.promotion_preflight_sha256'",
    'gh run download "$ready_run_id"',
    'target_capability_profile_id',
    'expected_current_capability_profile_id',
    'target_profile="$(jq -er',
    'expected_current_profile="$(jq -er',
    'echo "target_profile=$target_profile"',
    'echo "expected_current_profile=$expected_current_profile"',
    'kind == "AR11_READY_TO_MUTATE"',
    'ready == true',
    'provider_mutation == false',
    'Re-verify exact immutable Release Set before credentials',
  ], 'dispatch READY binder'));
  const readyBindStep = stepBlock(promote, 'Bind the explicit GitHub dispatch to unique accepted-main READY evidence');
  const readyDownloadStep = stepBlock(promote, 'Verify exact downloaded READY_TO_MUTATE evidence');
  errors.push(...forbidMarkers(`${readyBindStep}\n${readyDownloadStep}`, [
    'secrets.CLOUDFLARE_', 'wrangler deploy', 'promotion plan', 'promotion preflight',
    'SCOPED_EXCEPTION', 'AUTHORITY_COMMENT', 'WORKER_PROMOTION_AUTHORIZATION_V2',
    'authorization_comment_id', 'issues/comments/', 'branches/main',
    'STAGING_CANDIDATE', 'issues/870', 'issues: read',
  ], 'dispatch READY binder'));
  const manualResolve = stepBlock(promote, 'Capture manual resolve phase state');
  errors.push(...requireMarkers(manualResolve, [
    'steps.resolve.outcome',
    'failed_boundary="ACCEPTED_MAIN_RELEASE_RESOLUTION"',
    'step_outcomes:{resolve:$resolve',
  ], 'manual resolution failure localization'));
  for (const [name, label] of [
    ['Capture manual mutation phase state', 'mutation'],
    ['Capture manual post-verify phase state', 'post-verify'],
  ]) {
    errors.push(...requireMarkers(stepBlock(promote, name), [
      'test -n "$source_sha" || source_sha="$SOURCE_SHA"',
      '--arg source_sha "$source_sha"',
      '--arg dispatch_digest "${{ steps.intent.outputs.dispatch_digest }}"',
      'release_set_id:(if $release_set_id=="" then null else $release_set_id end)',
      'target_capability_profile_id:(if $target_profile=="" then null else $target_profile end)',
      'dispatch_digest:(if $dispatch_digest=="" then null else $dispatch_digest end)',
    ], `manual ${label} early-failure transport`));
  }

  errors.push(...requireMarkers(promote, [
    'Execute exact candidate after owner dispatch and READY evidence',
    'environment: staging',
    'TARGET_PROFILE',
    'EXPECTED_CURRENT_PROFILE',
    'DISPATCH_DIGEST',
    '[[ "$DISPATCH_DIGEST" =~ ^[0-9a-f]{64}$ ]]',
    'dispatch_digest:$dispatch_digest',
    'Bind previously verified READY evidence before provider credentials',
    'AR11_MUTATION_FENCE',
    'target_capability_profile_id:$target_profile',
    'expected_current_capability_profile_id:$expected_current_profile',
    'Re-verify exact immutable Release Set before credentials',
    'Render mutation overlay and prove exact bits dry-run without deploy credential',
    'CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}',
    'Re-fence Worker identity and D1 head immediately before mutation',
    'cmp --silent "$RUNNER_TEMP/mutation-d1-names.json" "$RUNNER_TEMP/ready/d1-names-after.json"',
    'Deploy exact Release Set v3 bits after all fences',
    '--message "release_set=$RELEASE_SET_ID profile=$TARGET_PROFILE"',
  ], 'protected mutation executor'));
  errors.push(...readyMutationByteIdentityErrors(promote));
  errors.push(...immediateMutationFenceErrors(promote));
  errors.push(...deployCredentialBoundaryErrors(promotion, promote));
  const currentMaterializations = (promote.match(/release-set-assets-ar11\.sh materialize current-v3/g) ?? []).length;
  if (currentMaterializations !== 1) {
    errors.push(`one-runner promotion must materialize the verified immutable target exactly once; observed=${currentMaterializations}`);
  }
  const releaseVerifications = (promote.match(/\brelease verify --release-set\b/g) ?? []).length;
  if (releaseVerifications !== 1) {
    errors.push(`one-runner promotion must verify exact release bits once before mutation; observed=${releaseVerifications}`);
  }
  errors.push(...forbidMarkers(promote, ['promotion plan', 'promotion preflight', 'release compatibility', 'materialize known-good-v2-v3', 'worker-build --release', 'cargo build', 'npm run build', 'profile=rehearsal-core-v2"'], 'protected mutation executor'));
  if ((promotion.match(/secrets\.CLOUDFLARE_API_TOKEN\s*}}/g) ?? []).length !== 1) {
    errors.push('deploy-capable Cloudflare token must be referenced exactly once in the whole promotion workflow');
  }
  const readyBind = promote.indexOf('Bind previously verified READY evidence before provider credentials');
  const deployCredential = promote.indexOf('CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}');
  const refence = promote.indexOf('Re-fence Worker identity and D1 head immediately before mutation');
  const actualDeploy = promote.indexOf('Deploy exact Release Set v3 bits after all fences');
  if (!(readyBind >= 0 && refence > readyBind && actualDeploy > refence && deployCredential > actualDeploy)) {
    errors.push('mutation order must be READY binding -> read-only immediate re-fence -> scoped deploy credential');
  }

  errors.push(...requireMarkers(promote, [
    'TARGET_PROFILE',
    'secrets.CLOUDFLARE_OBSERVE_API_TOKEN',
    "if: always() && steps.deploy.outcome == 'success'",
    "if: always() && steps.deploy.outcome == 'success' && steps.target_prepare.outcome == 'success'",
    'Re-observe provider and capture promotion.verify natural-owner verdict',
    'promotion verify',
    '--profile "$TARGET_PROFILE"',
    '> "$RUNNER_TEMP/promotion-verify.json"',
    'decision="$(jq -er',
    '$RUNNER_TEMP/promotion-verify.json',
    "if: always() && steps.observe_verify.outputs.decision == 'VERIFIED'",
    'Record authenticated human health handoff after promotion owner reports VERIFIED',
    'kind:"AR11_HUMAN_HEALTH_HANDOFF"',
    'decision:"DEFERRED_TO_V2_B7"',
    'reason:"HUMAN_CLOUDFLARE_ACCESS_REQUIRED"',
    'authenticated_health_owner:"V2_B7_REAL_WINDOWS_HUMAN"',
    'human-health-handoff.json',
  ], 'post-deploy verifier'));
  const postObservationStep = stepBlock(promote, 'Re-observe provider and capture promotion.verify natural-owner verdict');
  errors.push(...forbidMarkers(postObservationStep, [
    'secrets.CLOUDFLARE_API_TOKEN }}',
    'wrangler deploy --',
    '--profile rehearsal-core-v2',
    'CLOUDFLARE_ACCESS_CLIENT_ID',
    'CLOUDFLARE_ACCESS_CLIENT_SECRET',
    'CF-Access-Client-Id',
    'CF-Access-Client-Secret',
    'https://$custom_domain/api/v1/health',
  ], 'post-deploy verifier'));
  errors.push(...secretObservationErrors(postObservationStep, 'post-deploy verifier'));

  errors.push(...requireMarkers(manualOutcome, [
    "if: always() && needs.route.outputs.mode == 'promote'",
    'TARGET_PROFILE:',
    'Terminalize one lossless manual AR11 OperationalOutcome',
    '--mode manual',
    '--profile-id "$profile_id"',
    'Upload terminal manual AR11 OperationalOutcome evidence',
    'Enforce terminal manual AR11 disposition after evidence publication',
    '.contract == "PROMOTION_OPERATOR_OUTCOME_V1"',
    '.procedure == "AR11_RELEASE_SET_PROMOTION"',
    '.status == "COMPLETED"',
    '.provider_mutation_started == true',
    '.provider_mutation_executed == true',
    '.production_mutation_executed == false',
    '.effect_state == "EFFECT_VERIFIED"',
    '.authorization_state == "GITHUB_WORKFLOW_DISPATCH"',
    'DISPATCH_AUTHORIZED:',
    'DISPATCH_DIGEST:',
  ], 'manual terminal outcome'));
  errors.push(...forbidMarkers(manualOutcome, ['secrets.CLOUDFLARE_', 'wrangler deploy --', 'environment: production', '--profile-id rehearsal-core-v2'], 'manual terminal outcome'));
  const manualTerminalize = manualOutcome.indexOf('Terminalize one lossless manual AR11 OperationalOutcome');
  const manualUpload = manualOutcome.indexOf('Upload terminal manual AR11 OperationalOutcome evidence');
  const manualEnforce = manualOutcome.indexOf('Enforce terminal manual AR11 disposition after evidence publication');
  if (!(manualTerminalize >= 0 && manualUpload > manualTerminalize && manualEnforce > manualUpload)) {
    errors.push('manual AR11 must terminalize -> publish evidence -> enforce final disposition');
  }

  errors.push(...requireMarkers(rollback, [
    "if: needs.route.outputs.mode == 'rollback-negative'",
    'event == "workflow_run"',
    'ar11-ready-to-mutate-$RELEASE_SET_ID-',
    'ROLLBACK_COMPATIBILITY_UNKNOWN',
    'credential_values_accessed == false',
  ], 'rollback-negative evidence'));
  errors.push(...forbidMarkers(rollback, ['secrets.CLOUDFLARE_', 'wrangler deploy', 'deployments: write'], 'rollback-negative evidence'));
  return errors;
}

function assignmentBlock(source, name, nextMarker) {
  const start = source.indexOf(`${name} =`);
  if (start < 0) return '';
  const end = nextMarker ? source.indexOf(nextMarker, start + name.length) : source.length;
  return source.slice(start, end < 0 ? source.length : end);
}

function camoufoxClassifierErrors(classifier) {
  const errors = [];
  errors.push(...requireMarkers(classifier, [
    'EXACT_PROVEN_UNRELATED = frozenset(',
    'PROVEN_UNRELATED_PREFIXES = (',
    'PROVEN_UNRELATED_GLOBS = (',
    'return bool(heavy), unrelated, heavy',
    '"o0-e3a-d1-only-regression"',
    '"classifier-change-is-heavy"',
    '"gate-change-is-heavy"',
    '"mixed-fails-closed"',
    '"unknown-fails-closed"',
    'classify([])',
  ], 'Camoufox runtime-impact classifier'));

  const exact = assignmentBlock(classifier, 'EXACT_PROVEN_UNRELATED', 'PROVEN_UNRELATED_PREFIXES =');
  const prefixes = assignmentBlock(classifier, 'PROVEN_UNRELATED_PREFIXES', 'PROVEN_UNRELATED_GLOBS =');
  const globs = assignmentBlock(classifier, 'PROVEN_UNRELATED_GLOBS', 'def is_proven_unrelated');
  errors.push(...requireMarkers(exact, [
    '.github/workflows/release-set-promotion.yml',
    '.github/scripts/release-operational-ar11.mjs',
    'architecture/release-architecture-ar11.json',
    '.github/workflows/v2-phase-a-d1-command-router.yml',
  ], 'Camoufox exact proven-unrelated set'));
  errors.push(...requireMarkers(prefixes, [
    'docs/',
    'migrations/d1/',
    'tools/opsctl/',
  ], 'Camoufox proven-unrelated prefixes'));
  errors.push(...requireMarkers(globs, [
    '.github/workflows/d1-*.yml',
    '.github/scripts/d1-*',
    'scripts/check-d1-*',
    'scripts/d1-*',
  ], 'Camoufox proven-unrelated globs'));

  const safeAssignments = `${exact}\n${prefixes}\n${globs}`;
  errors.push(...forbidMarkers(safeAssignments, [
    'runtime/',
    'apps/',
    'crates/',
    'frontend/',
    'Cargo.lock',
    'rust-toolchain.toml',
    'scripts/classify-camoufox-runtime-scope.py',
    '.github/workflows/camoufox-runtime-gate.yml',
  ], 'Camoufox proven-unrelated classifier sets'));
  errors.push(...requireMarkers(classifier, [
    'runtime/camouhost/launcher.py',
    'apps/profile-bridge/src/main.rs',
    'Cargo.lock',
    'scripts/classify-camoufox-runtime-scope.py',
    '.github/workflows/camoufox-runtime-gate.yml',
    'scripts/test-browser-mail-execution-d1.py',
    'new-subsystem/file.txt',
  ], 'Camoufox fail-closed heavy fixtures'));
  return errors;
}

function camoufoxErrors(workflow, classifier) {
  const errors = [];
  const scope = jobBlock(workflow, 'scope');
  const linux = jobBlock(workflow, 'real-runtime-linux');
  const runtimePackage = jobBlock(workflow, 'runtime-package-windows');
  const archiveReader = jobBlock(workflow, 'shipping-archive-reader-windows');
  const windows = jobBlock(workflow, 'bridge-regression-windows');
  const proof = jobBlock(workflow, 'camoufox-required-proof');
  if (!scope || !linux || !runtimePackage || !archiveReader || !windows || !proof) {
    return ['Camoufox bounded Windows proof topology is incomplete'];
  }
  errors.push(...requireMarkers(scope, [
    'Classify accepted-main or mechanically proven runtime-unrelated PR',
    'if [ "$GITHUB_EVENT_NAME" = push ]; then',
    "echo 'heavy=true'",
    'python3.12 scripts/accepted-main-pr-reuse.py --self-test',
    'python3.12 scripts/accepted-main-pr-reuse.py --gate camoufox',
    "grep -qx 'reuse=true'",
    "echo 'heavy=false'",
    "echo 'heavy=true'",
    'python3.12 -m py_compile scripts/classify-camoufox-runtime-scope.py',
    'python3.12 scripts/classify-camoufox-runtime-scope.py --self-test',
    '--paths-file "$RUNNER_TEMP/changed-paths.txt"',
    '--github-output "$GITHUB_OUTPUT"',
  ], 'Camoufox scope routing'));
  errors.push(...camoufoxClassifierErrors(classifier));
  errors.push(...requireMarkers(linux, [
    'needs: scope',
    'if: always()',
    'Require successful scope classification',
    'needs.scope.outputs.heavy',
  ], 'Linux required context'));
  errors.push(...requireMarkers(runtimePackage, [
    'needs: scope',
    "if: needs.scope.outputs.heavy == 'true'",
    'Require successful scope classification',
    'Materialize official browser and verified offline fpgen model',
    'Package exact compressed Release Set runtime input',
    'Upload exact-head official runtime for downstream Windows proofs',
  ], 'Windows runtime package context'));
  errors.push(...requireMarkers(archiveReader, [
    'needs: [scope, runtime-package-windows]',
    "if: needs.scope.outputs.heavy == 'true'",
    'Require exact runtime package',
    'Download exact official runtime artifact',
    'Run Windows Bridge library tests and production archive reader',
  ], 'Windows production archive-reader context'));
  errors.push(...requireMarkers(windows, [
    // The browser Open proof and shipping-reader proof independently consume
    // the same verified package. The protected aggregator MUST join both.
    'needs: [scope, runtime-package-windows]',
    'if: always()',
    'Require successful prerequisites',
    'needs.scope.outputs.heavy',
    "if: needs.scope.outputs.heavy == 'true'",
    'Download exact official runtime artifact',
    'Extract and verify exact official runtime',
    'Run official Camoufox Playwright user Open and native file-lock proof through production Bridge crates',
  ], 'Windows user Open and native file-lock context'));
  errors.push(...requireMarkers(proof, [
    'needs: [scope, runtime-package-windows, shipping-archive-reader-windows, bridge-regression-windows]',
    'if: always()',
    'Require exact official Camoufox and Bridge Windows proof',
  ], 'Camoufox required-proof aggregator'));
  if (!linux.includes("if: needs.scope.outputs.heavy == 'true'")) errors.push('Linux heavy proof is not gated by exact scope output');
  if (!runtimePackage.includes("if: needs.scope.outputs.heavy == 'true'")) errors.push('Windows runtime packaging is not gated by exact scope output');
  if (!archiveReader.includes("if: needs.scope.outputs.heavy == 'true'")) errors.push('Windows archive-reader proof is not gated by exact scope output');
  if (!windows.includes("if: needs.scope.outputs.heavy == 'true'")) errors.push('Windows user Open proof is not gated by exact scope output');
  return errors;
}

function validateAll({ build, promotion, camoufox, camoufoxClassifier, authority }) {
  return [
    ...buildErrors(build),
    ...promotionErrors(promotion),
    ...camoufoxErrors(camoufox, camoufoxClassifier),
    ...authorityErrors(authority),
    ...legacyAuthorityErrors(),
    ...(existsSync(path.join(ROOT, ASSET_MATERIALIZER)) ? [] : [`missing Release Set asset materializer: ${ASSET_MATERIALIZER}`]),
  ];
}

function selfTest(files) {
  const preflightTerminal = stepBlock(jobBlock(files.promotion, 'preflight-ready'), 'Terminalize one lossless AR11 OperationalOutcome');
  for (const [marker, name] of [
    ['failed_boundary="ACCEPTED_MAIN_RELEASE_RESOLUTION"', 'main resolution'],
    ['failed_boundary="TARGET_RELEASE_VERIFY"', 'target immutable verify'],
  ]) {
    if (!preflightTerminal.includes(marker)) throw new Error(`preflight ${name} diagnostic fixture target is missing`);
    const withoutDiagnostic = files.promotion.replace(preflightTerminal, preflightTerminal.replace(marker, 'failed_boundary="UNKNOWN_GATE"'));
    if (!promotionErrors(withoutDiagnostic).some((error) => error.includes('pre-authorization failure localization'))) {
      throw new Error(`read-only ${name} error localization removal unexpectedly passed`);
    }
  }
  const manualResolve = stepBlock(jobBlock(files.promotion, 'promote'), 'Capture manual resolve phase state');
  for (const [marker, name] of [
    ['failed_boundary="ACCEPTED_MAIN_RELEASE_RESOLUTION"', 'main resolution'],
    ['step_outcomes:{resolve:$resolve', 'resolve evidence'],
  ]) {
    if (!manualResolve.includes(marker)) throw new Error(`manual ${name} diagnostic fixture target is missing`);
    const withoutDiagnostic = files.promotion.replace(manualResolve, manualResolve.replace(marker, 'diagnostic_was_removed'));
    if (!promotionErrors(withoutDiagnostic).some((error) => error.includes('manual resolution failure localization'))) {
      throw new Error(`manual ${name} error localization removal unexpectedly passed`);
    }
  }
  const missingRecovery = files.promotion.replace('.recovery_strategy == $recovery', 'true');
  if (!promotionErrors(missingRecovery).some((error) => error.includes('dispatch READY binder'))) {
    throw new Error('missing native recovery binding fixture passed');
  }
  const missingRecoveryProjection = files.promotion.replace('recovery_strategy:$recovery_strategy', 'recovery_strategy:null');
  if (!promotionErrors(missingRecoveryProjection).some((error) => error.includes('pre-authorization READY proof'))) {
    throw new Error('missing native recovery projection fixture passed');
  }
  const badSecret = files.promotion.replace('secret list --format json', 'secret list --name "$worker_name" --format json');
  if (secretObservationErrors(jobBlock(badSecret, 'preflight-ready'), 'fixture').length === 0) {
    throw new Error('redundant Worker-name secret observation fixture passed');
  }
  const ambiguousRunName = files.promotion.replace(EXPECTED_PROMOTION_RUN_NAME, 'run-name: AR11 Release Set Promotion');
  if (!promotionErrors(ambiguousRunName).some((error) => error.includes('run-name must distinguish'))) {
    throw new Error('ambiguous promotion run-name fixture unexpectedly passed');
  }
  const missingOwnerGuard = files.promotion.replace("if: github.event_name != 'workflow_dispatch' || github.actor == github.repository_owner", "if: always()");
  if (!promotionErrors(missingOwnerGuard).some((error) => error.includes('promotion invocation router'))) throw new Error('missing owner access guard fixture passed');
  const promoteJob = jobBlock(files.promotion, 'promote');
  if (!promoteJob.includes('environment: staging')) throw new Error('staging promotion fixture cannot locate its Environment');
  const missingStagingEnvironment = files.promotion.replace(promoteJob, promoteJob.replace('    environment: staging\n', ''));
  if (!promotionErrors(missingStagingEnvironment).some((error) => error.includes('dispatch READY binder'))) throw new Error('missing staging resolver Environment guard fixture passed');
  const missingUniqueReady = files.promotion.replaceAll('.total_count == 1 and (.artifacts | length) == 1', 'true');
  if (!promotionErrors(missingUniqueReady).some((error) => error.includes('dispatch READY binder'))) throw new Error('ambiguous READY artifact fixture passed');
  const staleIssueSelector = files.promotion.replace('candidate-ready-run.json', 'candidate-ready-run.json\\n          gh api "repos/$GITHUB_REPOSITORY/issues/870"');
  if (!promotionErrors(staleIssueSelector).some((error) => error.includes('dispatch READY binder'))) throw new Error('old Issue-body selector fixture passed');
  const readyBinder = stepBlock(promoteJob, 'Bind the explicit GitHub dispatch to unique accepted-main READY evidence');
  const pythonBlock = readyBinder.match(/<<'PY'\n([\s\S]*?)\n          PY/);
  if (!pythonBlock) throw new Error('READY dispatch intent Python executable is missing');
  const pythonSource = pythonBlock[1].split('\n').map((line) => {
    if (!line.startsWith('          ')) throw new Error('READY dispatch intent Python indentation is invalid');
    return line.slice(10);
  }).join('\n');
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), 'ar11-ready-binder-'));
  try {
    const mainSha = 'a'.repeat(40);
    const releaseId = 'release-set-v3-sha256-' + 'b'.repeat(64);
    const currentId = 'release-set-v3-sha256-' + 'c'.repeat(64);
    const readyArtifact = 'ar11-ready-to-mutate-' + releaseId + '-' + mainSha;
    const readyPath = path.join(fixtureRoot, 'ready.json');
    const intentPath = path.join(fixtureRoot, 'intent.json');
    const ready = {
      schema_version: 1, kind: 'AR11_READY_TO_MUTATE', ready: true, environment: 'staging',
      source_sha: mainSha, release_set_id: releaseId, expected_current_release_set_id: currentId,
      target_capability_profile_id: 'rehearsal-core-v2',
      expected_current_capability_profile_id: 'rehearsal-core-v1',
      credential_values_accessed: false, provider_mutation: false, production_mutation: false,
    };
    const runBinder = (candidate) => {
      writeFileSync(readyPath, JSON.stringify(candidate));
      return spawnSync('python', ['-c', pythonSource, readyPath, mainSha, releaseId, '12345', readyArtifact, 'd'.repeat(64), intentPath], {
        encoding: 'utf8',
        env: { ...process.env, GITHUB_ACTOR: 'owner', GITHUB_RUN_ID: '98765', GITHUB_REF: 'refs/heads/main' },
      });
    };
    const valid = runBinder(ready);
    if (valid.status !== 0) throw new Error('valid READY dispatch fixture failed: ' + valid.stderr);
    const intent = JSON.parse(readFileSync(intentPath, 'utf8'));
    if (intent.kind !== 'STAGING_PROMOTION_DISPATCH_INTENT' || intent.release_set_id !== releaseId
        || intent.source_sha !== mainSha || intent.expected_current_release_set_id !== currentId
        || intent.ready_run_id !== 12345 || intent.ready_artifact_name !== readyArtifact
        || intent.ready_evidence_sha256 !== 'd'.repeat(64)
        || intent.authorization.mechanism !== 'github.workflow_dispatch'
        || intent.target_capability_profile_id !== ready.target_capability_profile_id
        || intent.expected_current_capability_profile_id !== ready.expected_current_capability_profile_id) {
      throw new Error('valid READY dispatch fixture failed to bind exact immutable evidence');
    }
    const rejected = [
      ['source SHA drift', { ...ready, source_sha: 'e'.repeat(40) }],
      ['target Release Set drift', { ...ready, release_set_id: 'release-set-v3-sha256-' + 'e'.repeat(64) }],
      ['non-ready evidence', { ...ready, ready: false }],
      ['provider effect before dispatch', { ...ready, provider_mutation: true }],
      ['untyped current Release Set', { ...ready, expected_current_release_set_id: null }],
      ['untyped capability profile', { ...ready, target_capability_profile_id: null }],
    ];
    for (const [label, candidate] of rejected) {
      if (runBinder(candidate).status === 0) throw new Error('unsafe ' + label + ' READY dispatch fixture passed');
    }
  } finally {
    rmSync(fixtureRoot, { recursive: true, force: true });
  }
  const missingReadyBinding = files.promotion.replaceAll('READY_EVIDENCE_SHA256', 'READY_EVIDENCE_DIGEST_MISSING');
  if (promotionErrors(missingReadyBinding).length === 0) throw new Error('missing READY binding fixture passed');
  const missingReadyProfile = files.promotion.replace('target_capability_profile_id:$target_profile', 'target_capability_profile_id:null');
  if (!promotionErrors(missingReadyProfile).some((error) => error.includes('pre-authorization READY proof'))) {
    throw new Error('missing READY target capability-profile binding fixture passed');
  }
  const workflowOwnedBridge = files.promotion.replace(
    '[[ "$current_profile" =~ ^[a-z0-9][a-z0-9-]*$ ]]',
    '[[ "$current_profile" =~ ^[a-z0-9][a-z0-9-]*$ ]]\n          if [ "$current_id" != "$RELEASE_SET_ID" ]; then echo workflow-policy; fi',
  );
  if (!promotionErrors(workflowOwnedBridge).some((error) => error.includes('pre-authorization READY proof'))) {
    throw new Error('workflow-owned admission bridge fixture unexpectedly passed');
  }
  const rebuild = files.promotion.replace(
    'Deploy exact Release Set v3 bits after all fences',
    'run: worker-build --release\n      - name: Deploy exact Release Set v3 bits after all fences',
  );
  if (!promotionErrors(rebuild).some((error) => error.includes('worker-build --release'))) throw new Error('promotion rebuild fixture unexpectedly passed');
  const missingFailureCapture = files.promotion.replace(
    'test -n "$source_sha" || source_sha="$SOURCE_SHA"',
    'test -n "$source_sha" || source_sha=""',
  );
  if (!promotionErrors(missingFailureCapture).some((error) => error.includes('manual mutation early-failure transport'))) {
    throw new Error('manual early-failure source identity fixture unexpectedly passed');
  }
  const cargoRebuild = files.promotion.replace(
    'Deploy exact Release Set v3 bits after all fences',
    'run: cargo build --release\n      - name: Deploy exact Release Set v3 bits after all fences',
  );
  if (!promotionErrors(cargoRebuild).some((error) => error.includes('cargo build'))) throw new Error('promotion cargo-rebuild fixture unexpectedly passed');
  const mutationBlock = jobBlock(files.promotion, 'promote');
  const brokenMutationBlock = mutationBlock.replace(
    'cmp --silent "$release_root/release-set.json" "$RUNNER_TEMP/ready/release-set.json"',
    'cp "$release_root/release-set.json" "$RUNNER_TEMP/ready/release-set.json"',
  );
  if (brokenMutationBlock === mutationBlock) throw new Error('READY-to-mutation Release Set byte-identity fixture setup failed');
  const brokenByteIdentity = files.promotion.replace(mutationBlock, brokenMutationBlock);
  if (brokenByteIdentity === files.promotion) throw new Error('READY-to-mutation Release Set byte-identity fixture setup failed');
  if (!promotionErrors(brokenByteIdentity).some((error) => error.includes('byte-compare'))) throw new Error('READY-to-mutation Release Set byte-identity fixture passed');
  const staleFenceBypass = files.promotion.replace(
    'test "$(jq -r \' .release_set_id\' "$RUNNER_TEMP/mutation-current-identity.json")" = "$EXPECTED_CURRENT"'.replace("' .release", "'.release"),
    'echo "expected-current Worker fence bypassed"',
  );
  if (!promotionErrors(staleFenceBypass).some((error) => error.includes('expected-current Worker fence'))) {
    throw new Error('mutation stale-fence bypass fixture unexpectedly passed');
  }
  const staleProfileFenceBypass = files.promotion.replace(
    'test "$(jq -r \' .capability_profile_id\' "$RUNNER_TEMP/mutation-current-identity.json")" = "$EXPECTED_CURRENT_PROFILE"'.replace("' .capability", "'.capability"),
    'echo "expected-current capability-profile fence bypassed"',
  );
  if (!promotionErrors(staleProfileFenceBypass).some((error) => error.includes('capability-profile fence'))) {
    throw new Error('mutation stale capability-profile fence bypass fixture unexpectedly passed');
  }
  const hardcodedDeployProfile = files.promotion.replace(
    '--message "release_set=$RELEASE_SET_ID profile=$TARGET_PROFILE"',
    '--message "release_set=$RELEASE_SET_ID profile=rehearsal-core-v2"',
  );
  if (!promotionErrors(hardcodedDeployProfile).some((error) => error.includes('protected mutation executor'))) {
    throw new Error('hard-coded mutation target capability-profile fixture unexpectedly passed');
  }
  const deployToken = '          CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}';
  const earlyCredentialAnchor = '      - name: Render mutation overlay and prove exact bits dry-run without deploy credential';
  let earlyDeployCredential = files.promotion.replace(deployToken, '          CLOUDFLARE_API_TOKEN: inherited');
  earlyDeployCredential = earlyDeployCredential.replace(
    earlyCredentialAnchor,
    '      - name: Early deploy credential fixture\n        env:\n          CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}\n        run: echo early\n\n' + earlyCredentialAnchor,
  );
  if (earlyDeployCredential === files.promotion || !earlyDeployCredential.includes('Early deploy credential fixture')) {
    throw new Error('early deploy credential fixture setup failed');
  }
  if (!promotionErrors(earlyDeployCredential).some((error) => error.includes('deploy step'))) {
    throw new Error('early deploy credential fixture unexpectedly passed');
  }
  const mutationWithDeployRead = mutationBlock.replace(
    'CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_OBSERVE_API_TOKEN }}',
    'CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}',
  );
  if (mutationWithDeployRead === mutationBlock) throw new Error('deploy-token exact-current read fence fixture setup failed');
  const deployTokenFence = files.promotion.replace(mutationBlock, mutationWithDeployRead);
  if (!promotionErrors(deployTokenFence).some((error) => error.includes('read-only exact-current fence'))) {
    throw new Error('deploy-token exact-current read fence fixture unexpectedly passed');
  }
  const missingFenceProjection = files.promotion.replace('exact_current_fence:$exact_current_fence', 'exact_current_fence:null');
  if (!promotionErrors(missingFenceProjection).some((error) => error.includes('lossless exact-current fence projection'))) {
    throw new Error('lossy exact-current fence projection fixture unexpectedly passed');
  }
  const missingReadOnlyTerminalization = files.promotion.replace('Terminalize one lossless AR11 OperationalOutcome', 'Read-only terminalization fixture removed');
  if (!promotionErrors(missingReadOnlyTerminalization).some((error) => error.includes('pre-authorization READY proof'))) {
    throw new Error('missing read-only terminalization fixture unexpectedly passed');
  }
  const missingReadOnlyPublication = files.promotion.replace('Upload terminal AR11 OperationalOutcome evidence', 'Read-only terminal publication fixture removed');
  if (!promotionErrors(missingReadOnlyPublication).some((error) => error.includes('pre-authorization READY proof'))) {
    throw new Error('missing read-only terminal publication fixture unexpectedly passed');
  }
  const blockedTerminalDispositionRemoved = files.promotion.replace(' or .status == "BLOCKED"', '');
  if (blockedTerminalDispositionRemoved === files.promotion) throw new Error('BLOCKED terminal disposition fixture setup failed');
  if (!promotionErrors(blockedTerminalDispositionRemoved).some((error) => error.includes('pre-authorization READY proof'))) {
    throw new Error('missing semantic BLOCKED terminal disposition fixture unexpectedly passed');
  }
  const missingManualTerminalization = files.promotion.replace('Terminalize one lossless manual AR11 OperationalOutcome', 'Terminalization fixture removed');
  if (!promotionErrors(missingManualTerminalization).some((error) => error.includes('manual terminal outcome'))) {
    throw new Error('missing manual terminalization fixture unexpectedly passed');
  }
  const weakManualSuccess = files.promotion.replace('.status == "COMPLETED"', '.status == "RECOVERY_REQUIRED"');
  if (!promotionErrors(weakManualSuccess).some((error) => error.includes('manual terminal outcome'))) {
    throw new Error('manual success-without-COMPLETED fixture unexpectedly passed');
  }
  const legacyFixture = legacyAuthorityErrors((candidate) => candidate === path.join(ROOT, LEGACY_FILES[0]));
  if (legacyFixture.length !== 1 || !legacyFixture[0].includes('legacy D3 operational authority must be retired after Rust cutover')) {
    throw new Error('retired D3 operational authority restoration fixture passed');
  }
  const missingClassifierCall = files.camoufox.replace(
    'python3.12 scripts/classify-camoufox-runtime-scope.py --self-test',
    'echo classifier-self-test-bypassed',
  );
  if (camoufoxErrors(missingClassifierCall, files.camoufoxClassifier).length === 0) {
    throw new Error('Camoufox classifier self-test bypass fixture passed');
  }
  const missingAcceptedProof = files.camoufox.replace(
    'python3.12 scripts/accepted-main-pr-reuse.py --gate camoufox',
    'echo bypassed-main-proof',
  );
  if (!camoufoxErrors(missingAcceptedProof, files.camoufoxClassifier).some(error => error.includes('Camoufox scope routing'))) {
    throw new Error('accepted-main PR proof bypass fixture passed');
  }
  const missingHeavyFallback = files.camoufox.replace(
    "echo 'heavy=true' >> \"$GITHUB_OUTPUT\"",
    'echo "skipped fallback"',
  );
  if (!camoufoxErrors(missingHeavyFallback, files.camoufoxClassifier).some(error => error.includes('Camoufox scope routing'))) {
    throw new Error('accepted-main full replay fallback bypass fixture passed');
  }
  const missingArchiveReaderFromFinalProof = files.camoufox.replace(
    'needs: [scope, runtime-package-windows, shipping-archive-reader-windows, bridge-regression-windows]',
    'needs: [scope, runtime-package-windows, bridge-regression-windows]',
  );
  if (missingArchiveReaderFromFinalProof === files.camoufox
      || !camoufoxErrors(missingArchiveReaderFromFinalProof, files.camoufoxClassifier)
        .some(error => error.includes('Camoufox required-proof aggregator'))) {
    throw new Error('missing shipping-reader edge on protected final Camoufox proof fixture passed');
  }
  const browserProof = jobBlock(files.camoufox, 'bridge-regression-windows');
  const missingRuntimePackageFromBrowser = files.camoufox.replace(browserProof, browserProof.replace(
    'needs: [scope, runtime-package-windows]',
    'needs: scope',
  ));
  if (missingRuntimePackageFromBrowser === files.camoufox
      || !camoufoxErrors(missingRuntimePackageFromBrowser, files.camoufoxClassifier)
        .some(error => error.includes('Windows user Open and native file-lock context'))) {
    throw new Error('missing verified runtime package on Windows proof fixture passed');
  }
  const wrongAcceptedMainPolicy = structuredClone(files.authority);
  wrongAcceptedMainPolicy.promotion_policy.pr_runtime_accepted_main_policy = 'ALWAYS_HEAVY';
  if (!authorityErrors(wrongAcceptedMainPolicy).some(error => error.includes('proven PR tree or full main replay'))) {
    throw new Error('incorrect accepted-main runtime policy fixture passed');
  }
  const missingExactPrEvidence = structuredClone(files.authority);
  missingExactPrEvidence.promotion_policy.accepted_main_exact_tree_pr_evidence = false;
  if (!authorityErrors(missingExactPrEvidence).some(error => error.includes('accepted_main_exact_tree_pr_evidence'))) {
    throw new Error('missing exact-tree PR authority fixture passed');
  }
  const unsafePrefixClassifier = files.camoufoxClassifier.replace('    "docs/",', '    "docs/",\n    "runtime/",');
  if (unsafePrefixClassifier === files.camoufoxClassifier) throw new Error('unsafe Camoufox prefix fixture setup failed');
  if (camoufoxClassifierErrors(unsafePrefixClassifier).length === 0) {
    throw new Error('unsafe Camoufox runtime prefix fixture passed');
  }
  const missingUnknownFixture = files.camoufoxClassifier.replace('"unknown-fails-closed"', '"unknown-fixture-removed"');
  if (camoufoxClassifierErrors(missingUnknownFixture).length === 0) {
    throw new Error('Camoufox unknown fail-closed fixture removal passed');
  }
  const weakAuthority = structuredClone(files.authority);
  weakAuthority.promotion_policy.read_only_ready_before_mutation_authorization = false;
  if (authorityErrors(weakAuthority).length === 0) throw new Error('authorization-before-READY authority fixture passed');
  const weakRuntimeAuthority = structuredClone(files.authority);
  weakRuntimeAuthority.promotion_policy.pr_runtime_unknown_path_policy = 'CHEAP';
  if (authorityErrors(weakRuntimeAuthority).length === 0) throw new Error('unknown-path CHEAP authority fixture passed');
  console.log('AR-11 semantic release/promotion/change-aware negative matrix passed.');
}

const files = {
  build: read(BUILD),
  promotion: read(PROMOTION),
  camoufox: read(CAMOUFOX),
  camoufoxClassifier: read(CAMOUFOX_SCOPE),
  authority: JSON.parse(read(AUTHORITY)),
};
const errors = validateAll(files);
if (errors.length > 0) {
  console.error('AR-11 operational authority failed:\n' + errors.map((error) => `- ${error}`).join('\n'));
  process.exit(1);
}
if (process.argv.includes('--self-test')) selfTest(files);
else console.log('AR-11 release operational authority passed: automatic read-only READY/BLOCKED terminalizes before the explicit owner workflow_dispatch; unique accepted-main Release Set and immutable READY evidence bind to that dispatch; GitHub Environment restricts staging to main; exact-current Worker/D1 re-fence prevents replay after success; provider mutation remains behind the exact-bits dry-run and credential boundary; Camoufox PR replay is fail-closed and runtime-impact-aware.');
