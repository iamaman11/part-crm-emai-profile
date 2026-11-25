#!/usr/bin/env node

import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { spawnSync } from 'node:child_process';

const ROOT = process.cwd();
const BUILD = '.github/workflows/release-set-build.yml';
const PROMOTION = '.github/workflows/release-set-promotion.yml';
const CAMOUFOX = '.github/workflows/ci.yml';
const CAMOUFOX_SCOPE = 'scripts/classify-camoufox-runtime-scope.py';
const AR11_RUNTIME_DOWNLOADER = 'scripts/download-release-set-ar11.py';
const AR11_RUNTIME_OVERLAY = 'scripts/release-core-overlay-ar11.py';
const STANDING_RUN_PROOF = 'scripts/ar11-standing-run-proof.py';
const AUTHORITY = 'architecture/release-architecture-ar11.json';
const ASSET_MATERIALIZER = 'scripts/release-set-assets-ar11.sh';
const EXPECTED_PROMOTION_RUN_NAME = "run-name: AR11 Release Set Promotion — ${{ github.event_name == 'workflow_run' && 'STAGING READY/AUTO PROMOTION' || 'INTERNAL ROLLBACK-NEGATIVE EVIDENCE' }}";
const READ_ONLY_TERMINAL_DISPOSITION = '(.status == "READ_ONLY_READY" or .status == "NOOP" or .status == "BLOCKED")';
const CAMOUFOX_ACTIVE_PHYSICAL_COMMANDS = [
  ['runtime-package-windows', 'Materialize official browser and verified offline fpgen model', 'python scripts/camouhost-runtime-build-windows.py build'],
  ['runtime-package-windows', 'Materialize official browser and verified offline fpgen model', 'python scripts/github-release-publication.py probe'],
  ['runtime-package-windows', 'Materialize official browser and verified offline fpgen model', 'gh release download'],
  ['runtime-package-windows', 'Prove official Firefox native restored tabs and POST safety on exact runtime', 'python scripts/test-ar10-real-camoufox-cleanup.py'],
  ['runtime-package-windows', 'Package exact compressed Release Set runtime input', 'python scripts/camouhost-runtime-package.py package --runtime-root'],
  ['runtime-package-windows', 'Extract and verify exact official runtime', 'tar.exe -xzf $archive -C $root'],
  ['runtime-package-windows', 'Run physical shipping reader and native Bridge Open on exact packaged runtime', '& "$env:WINDIR\\System32\\cmd.exe" /d /s /c $testCommand'],
];
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

function requireActiveShellCommand(step, command, label) {
  // A marker inside a YAML/PowerShell comment or a Write-Host string is not
  // physical execution. Require the real command at the start of a shell line.
  const active = step.split('\n').some((raw) => {
    const line = raw.trim();
    return !line.startsWith('#') && (line === command || line.startsWith(`${command} `));
  });
  return active ? [] : [`${label} is not executing active shell command ${JSON.stringify(command)}`];
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

function runtimeSidecarErrors(promotion, downloader, overlay) {
  const errors = [];
  if ((promotion.match(/--runtime-manifest-release-id/g) ?? []).length !== 3
      || (promotion.match(/--runtime-manifest "\$TARGET_RUNTIME_MANIFEST"/g) ?? []).length !== 3) {
    errors.push('AR11 sidecar must have exactly three bounded download sites and three bound overlay consumers');
  }
  const ready = jobBlock(promotion, 'preflight-ready');
  const manual = jobBlock(promotion, 'promote');
  const targetStep = stepBlock(ready, 'Verify immutable target before any provider credential');
  const knownGoodStep = stepBlock(ready, 'Verify current immutable Release Set and build metadata snapshot');
  const manualStep = stepBlock(manual, 'Re-verify exact immutable Release Set before credentials');
  const downloads = [
    ['automatic target', targetStep, '$runtime_id'],
    ['automatic known-good', knownGoodStep, '$previous_runtime_id'],
    ['manual target', manualStep, '$runtime_id'],
  ];
  for (const [label, step, releaseId] of downloads) {
    const lines = logicalShellLines(step).filter((line) => !line.startsWith('#'));
    const destination = releaseId === '$runtime_id' ? '$runtime_sidecar_dir' : '$previous_sidecar_dir';
    const verify = lines.findIndex((line) => line.startsWith('cargo run --locked --quiet --manifest-path tools/opsctl/Cargo.toml -- ')
      && line.includes('--root . release verify') && line.includes('--artifact-root "$release_root"'));
    const verdict = lines.findIndex((line) => line.includes('jq -e') && line.includes('.decision == "VALID"'));
    const download = lines.findIndex((line) => line.startsWith(`python ${AR11_RUNTIME_DOWNLOADER} `)
      && line.includes(`--runtime-manifest-release-id "${releaseId}"`)
      && line.includes(`--destination "${destination}"`));
    if (verify < 0 || verdict < verify || download <= verdict || !lines.some((line) => line === 'set -euo pipefail')) {
      errors.push(`${label} sidecar requires an earlier active full opsctl release verify and VALID verdict`);
    }
    if (step.includes('continue-on-error: true') || [lines[verify], lines[verdict], lines[download]].some((line) => line && /\|\|\s*true/.test(line))) {
      errors.push(`${label} sidecar verification or download cannot ignore failures`);
    }
  }
  const initialOverlay = stepBlock(ready, 'Materialize exact target and rendered staging overlay');
  const manualOverlay = stepBlock(manual, 'Render mutation overlay and prove exact bits dry-run without deploy credential');
  for (const [label, step] of [['automatic initial', initialOverlay], ['automatic dual', knownGoodStep], ['manual', manualOverlay]]) {
    const command = logicalShellLines(step).find((line) => line.startsWith('python scripts/release-core-overlay-ar11.py '));
    if (!command?.includes('--runtime-manifest "$TARGET_RUNTIME_MANIFEST"')) {
      errors.push(`${label} overlay bypasses the SHA-bound target runtime sidecar`);
    }
  }
  const targetFence = [
    '[[ "$runtime_id" =~ ^runtime-bundle-v3-sha256-[0-9a-f]{64}$ ]]',
    'echo "TARGET_RUNTIME_MANIFEST=$runtime_sidecar_dir/runtime-manifest.json" >> "$GITHUB_ENV"',
  ];
  for (const [label, step, sourcePath] of [
    ['automatic', targetStep, '$policy_dir/release-set.json'],
    ['manual', manualStep, '$release_root/release-set.json'],
  ]) {
    errors.push(...requireMarkers(step, [
      `runtime_id="$(jq -er '.components.runtime_bundle.release_id' "${sourcePath}")"`,
      ...targetFence,
    ], `${label} sidecar identity/source binding`));
  }
  errors.push(...requireMarkers(knownGoodStep, [
    'previous_runtime_id="$(jq -er \'.components.runtime_bundle.release_id\' "$current_release_manifest")"',
    'previous_manifest_sha="$(jq -er \'.components.runtime_bundle.component_manifest_sha256\' "$current_release_manifest")"',
    'target_runtime_id="$(jq -er \'.components.runtime_bundle.release_id\' "$TARGET_RELEASE_MANIFEST")"',
    'target_manifest_sha="$(jq -er \'.components.runtime_bundle.component_manifest_sha256\' "$TARGET_RELEASE_MANIFEST")"',
    'if [ "$previous_runtime_id" = "$target_runtime_id" ]; then',
    'test "$previous_manifest_sha" = "$target_manifest_sha"',
    'previous_runtime_manifest="$TARGET_RUNTIME_MANIFEST"',
    'previous_runtime_manifest_args=(--previous-runtime-manifest "$previous_runtime_manifest")',
    '"${previous_runtime_manifest_args[@]}"',
  ], 'known-good sidecar exact identity and digest reuse'));
  const previousCommand = logicalShellLines(knownGoodStep).find((line) => line.startsWith('python scripts/release-core-overlay-ar11.py '));
  if (!previousCommand?.includes('"${previous_runtime_manifest_args[@]}"') || !previousCommand.includes('--previous-runtime-release-set')) {
    errors.push('known-good overlay lacks its independently bound runtime sidecar');
  }
  const activePython = (source) => source.split('\n').map((line) => line.trim()).filter((line) => line && !line.startsWith('#'));
  const overlayLines = activePython(overlay);
  for (const marker of [
    'if manifest_sidecar is None:',
    'if runtime["artifact_path"] == "components/runtime-bundle.tar.gz":',
    'if manifest_sidecar.is_symlink() or not manifest_sidecar.is_file():',
    'if not (1 <= manifest_sidecar.stat().st_size <= 2_000_000):',
    'if hashlib.sha256(raw).hexdigest() != runtime["component_manifest_sha256"]:',
    'or document.get("release_id") != runtime["release_id"]',
    'or document.get("schema_version") != 3',
    'or document.get("kind") != "CAMOUFOX_WINDOWS_RUNTIME_COMPONENT"',
    'document = json.loads(raw, object_pairs_hook=unique_json_members)',
  ]) {
    if (!overlayLines.some((line) => line.includes(marker))) errors.push(`runtime sidecar source lost active fail-closed check ${marker}`);
  }
  const downloaderLines = activePython(downloader);
  for (const marker of [
    'RUNTIME_COMPONENT_PATTERN = re.compile(',
    'command.extend(["--pattern", RUNTIME_MANIFEST_ASSET])',
    'or members[0].is_symlink() or not members[0].is_file()',
    'or not (1 <= members[0].stat().st_size <= MAX_RUNTIME_MANIFEST_BYTES)',
  ]) {
    if (!downloaderLines.some((line) => line.includes(marker))) errors.push(`runtime sidecar transport lost active bounded download check ${marker}`);
  }
  return errors;
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
    'sub_fences:{main_read:$main_read,main_match:$main_match,deployment_read:$deployment_read,release_set_match:$release_set_match,capability_profile_match:$capability_profile_match,worker_identity_match:$worker_identity_match,d1_read:$d1_read,d1_match:$d1_match}',
    'observed_worker_identity_sha256:',
    'expected_worker_identity_sha256:$expected_worker_identity_sha256',
    'expected_worker_identity_sha256="$(jq -er \'.worker_identity_sha256\' "$RUNNER_TEMP/ready/ready-to-mutate.json")"',
    'jq -S --slurpfile observed "$RUNNER_TEMP/mutation-current-deployment.json"',
    'script_etag:$observed[0].current_version.resources.script.etag',
    '> "$RUNNER_TEMP/mutation-worker-binding.json"',
    'observed_worker_identity_sha256="$(sha256sum "$RUNNER_TEMP/mutation-worker-binding.json" | cut -d\' \' -f1)"',
    'if [ "$observed_worker_identity_sha256" = "$expected_worker_identity_sha256" ]; then',
    'worker_identity_match=success',
    'worker_identity_match=failure',
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
  if (!(readyIndex >= 0 && dryRunIndex > readyIndex && fenceIndex > dryRunIndex
      && deployIndex > fenceIndex)) {
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
  const runtime = jobBlock(build, 'runtime-bundle');
  errors.push(...requireMarkers(runtime, [
    'Require exact PR-tested runtime bytes before Release publication',
    'python scripts/accepted-main-pr-reuse.py --release-component runtime',
    '--runtime-release-id "$env:RUNTIME_RELEASE_ID"',
    '--runtime-publication-state "$($probe.state)"',
    'Verify exact PR-tested runtime component on cache miss',
    'runtime-physical-proof.json',
    'published runtime bytes differ from exactly physically tested PR bytes',
    'runtime-bundle.tar.gz', 'runtime-manifest.json',
  ], 'SHA-bound main runtime publication'));
  errors.push(...forbidMarkers(runtime, [
    'camouhost-runtime-build-windows.py build',
    'camouhost-runtime-package.py package --runtime-root',
  ], 'main must never rebuild an already physically tested runtime'));
  errors.push(...requireActiveShellCommand(
    stepBlock(runtime, 'Require exact PR-tested runtime bytes before Release publication'),
    'python scripts/accepted-main-pr-reuse.py --release-component runtime',
    'main Release Set physical archive custody',
  ));
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
    'standing_staging_only_after_same_run_read_only_ready',
    'ready_to_mutate_evidence_required_for_standing_promotion',
    'standing_intent_binds_same_run_ready_evidence',
    'owner_manual_dispatch_retired',
    'staging_environment_main_only',
    'expected_current_refence_immediately_before_mutation',
    'd1_refence_immediately_before_mutation',
    'accepted_main_heavy_runtime_replay',
    'accepted_main_exact_tree_pr_evidence',
  ];
  for (const key of requiredTrue) {
    if (policy[key] !== true) errors.push(`AR-11 promotion policy requires ${key}=true`);
  }
  if (policy.standing_staging_authorization !== 'AR11_STANDING_STAGING_READY_V1') {
    errors.push('AR-11 staging policy must bind exact standing owner authorization');
  }
  for (const obsolete of [
    'read_only_ready_before_manual_dispatch', 'ready_to_mutate_evidence_required_for_dispatch',
    'dispatch_intent_binds_ready_evidence', 'workflow_dispatch_is_staging_authorization',
    'owner_only_manual_dispatch', 'read_only_ready_before_mutation_authorization',
    'ready_to_mutate_evidence_required_for_authorization', 'authorization_binds_ready_evidence',
  ]) {
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

function standingPredecessorProofErrors(source) {
  // Exact pre-effect recovery is admitted only after a canonical, completed
  // failed run whose complete job/step topology proves credentials and deploy
  // were never reached. Keep the source contract under the negative matrix.
  return requireMarkers(source, [
    'type(total) is int and total >= 2 and isinstance(runs, list)',
    'len(runs) == min(total, 100)',
    'run.get("event") == "workflow_run"',
    'run.get("head_branch") == "main"',
    'len({run["id"] for run in runs}) == len(runs)',
    'len({run["run_number"] for run in runs}) == len(runs)',
    'current[0].get("run_attempt") == 1',
    'current[0].get("status") == "in_progress"',
    'earlier = [run for run in runs if run["run_number"] < current[0]["run_number"]]',
    'previous = max(earlier, key=lambda run: run["run_number"])',
    'previous.get("status") == "completed"',
    'previous.get("run_attempt") == 1',
    'if previous.get("conclusion") == "success":',
    'require(previous.get("conclusion") == "failure", "unknown previous outcome")',
    'count == len(EXPECTED_JOBS)',
    'and {job.get("name") for job in jobs} == EXPECTED_JOBS',
    'and job.get("run_id") == previous["id"]',
    'exactly_one(jobs, ROUTE).get("conclusion") == "success"',
    'exactly_one(jobs, PREFLIGHT).get("conclusion") == "success"',
    'exactly_one(jobs, ROLLBACK).get("conclusion") == "skipped"',
    'exactly_one(jobs, TERMINAL).get("conclusion") == "failure"',
    'promote.get("status") == "completed" and promote.get("conclusion") == "failure"',
    'exactly_one(steps, RESOLVE).get("conclusion") == "success"',
    'bind.get("conclusion") == "failure"',
    'exactly_one(steps, name).get("conclusion") == "skipped"',
    'for name in (BEFORE_CREDENTIALS, MUTATION_BOUNDARY, DEPLOY)',
    'bind.get("conclusion") == "success" and pin.get("conclusion") == "failure"',
    '[step["name"] for step in actual_prefix] == list(PIN_SUCCESS_PREFIX)',
    'all(step.get("conclusion") == "success" for step in actual_prefix)',
    'for name in PIN_SKIPPED',
    'and exactly_one(steps, name)["number"] > pin_number',
    'len({step["number"] for step in steps}) == len(steps)',
    'step["number"] < failure_number',
    'step["name"] in prefix',
    'step.get("conclusion") == "skipped"',
    '(step["name"] in SAFE_POST_FAILURE and step.get("conclusion") == "success")',
    'raise EvidenceRejected(message)',
  ], 'AR14 exact pre-effect predecessor proof');
}

function promotionErrors(promotion) {
  const errors = [];
  const runNameLine = promotion.split('\n').find((line) => line.startsWith('run-name: '));
  if (runNameLine !== EXPECTED_PROMOTION_RUN_NAME) {
    errors.push('promotion run-name must distinguish standing staging auto-promotion from internal rollback-negative evidence');
  }
  errors.push(...requireMarkers(promotion, [
    'workflow_run:',
    '- Release Set Build',
    'workflow_call:',
    'concurrency:',
    'group: release-set-promotion-staging',
    'cancel-in-progress: false',
  ], 'Release Set promotion'));
  if (promotion.includes('workflow_dispatch:') || promotion.includes('GITHUB_WORKFLOW_DISPATCH')
      || promotion.includes('STAGING_PROMOTION_DISPATCH_INTENT')) {
    errors.push('standing auto-promotion cannot retain manual workflow-dispatch authority');
  }

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
  const stagingOutcome = jobBlock(promotion, 'promotion-outcome');
  const rollback = jobBlock(promotion, 'rollback-negative-evidence');
  for (const [name, block] of Object.entries({ route, 'preflight-ready': ready, promote, 'promotion-outcome': stagingOutcome, 'rollback-negative-evidence': rollback })) {
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
    "echo 'mode=rollback-negative'",
  ], 'promotion invocation router'));
  errors.push(...forbidMarkers(route, ["echo 'mode=promote'", 'workflow_dispatch', 'secrets.', 'wrangler deploy', 'environment: staging'], 'promotion invocation router'));

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
    'Re-observe live Worker identity and D1 ledger before READY',
    "steps.current_readback.outcome == 'success'",
    'provider_observation:{method:"INDEPENDENT_LIVE_READBACK",worker_identity_stable:true,d1_ledger_stable:true}',
    '.provider_observation.method == "INDEPENDENT_LIVE_READBACK"',
    '.provider_observation.worker_identity_stable == true',
    '.provider_observation.d1_ledger_stable == true',
    'cmp --silent "$RUNNER_TEMP/worker-binding-before.json" "$RUNNER_TEMP/worker-binding-after.json"',
    'cmp --silent "$RUNNER_TEMP/d1-names-before.json" "$RUNNER_TEMP/d1-names-after.json"',
    'kind:"AR11_READY_TO_MUTATE"',
    'target_capability_profile_id:$target_profile',
    'expected_current_capability_profile_id:$expected_current_profile',
    'admission_mode:$admission_mode',
    '--arg target_profile "${{ steps.policy.outputs.target_profile }}"',
    '--arg admission_mode "${{ steps.policy.outputs.admission_mode }}"',
    'secret_bindings_verified:true',
    'worker_identity_sha256:$worker_identity_sha256',
    'worker_identity_sha="$(sha256sum "$RUNNER_TEMP/worker-binding-after.json" | cut -d\' \' -f1)"',
    'provider_mutation:false',
    'production_mutation:false',
    'ar11-ready-to-mutate-$RELEASE_SET_ID-$SOURCE_SHA',
    'Terminalize one lossless AR11 OperationalOutcome',
    'promotion-operational-outcome-ar11.py',
    '--profile-id "$effective_target_profile"',
    '--output "$RUNNER_TEMP/promotion-operational-outcome.json"',
    'Upload terminal AR11 OperationalOutcome evidence',
    'evidence_artifact="ar11-operational-outcome-$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT"',
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
  const baselineStep = stepBlock(ready, 'Observe existing Worker, D1 head, resources and secret bindings');
  const secondStep = stepBlock(ready, 'Re-observe live Worker identity and D1 ledger before READY');
  const afterWorker = 'http_code="$(curl --silent --show-error --output "$RUNNER_TEMP/deployments-after-api.json"';
  const afterVersion = 'python scripts/deployment-identity-ar11.py --observe-version';
  const afterD1 = 'npx --yes wrangler@4.94.0 d1 execute CATALOG_DB --remote';
  const workerCmp = 'cmp --silent "$RUNNER_TEMP/worker-binding-before.json" "$RUNNER_TEMP/worker-binding-after.json"';
  const d1Cmp = 'cmp --silent "$RUNNER_TEMP/d1-names-before.json" "$RUNNER_TEMP/d1-names-after.json"';
  if (!baselineStep.includes('> "$RUNNER_TEMP/worker-binding-before.json"')
      || !baselineStep.includes('> "$RUNNER_TEMP/catalog-ledger-before.json"')
      || !baselineStep.includes('python - "$RUNNER_TEMP/catalog-ledger-before.json" "$RUNNER_TEMP/d1-names-before.json"')
      || !baselineStep.includes('d1_head="$(jq -er')) {
    errors.push('native Worker/D1 readback is missing its independent baseline observations');
  }
  if (!secondStep.includes(afterWorker) || !secondStep.includes(afterVersion)
      || !secondStep.includes(afterD1) || !secondStep.includes(workerCmp)
      || !secondStep.includes(d1Cmp) || !secondStep.includes('test "$http_code" = 200')
      || secondStep.indexOf(afterWorker) >= secondStep.indexOf(afterVersion)
      || secondStep.indexOf(afterVersion) >= secondStep.indexOf(workerCmp)
      || secondStep.indexOf(afterD1) >= secondStep.indexOf(d1Cmp)) {
    errors.push('native Worker/D1 readback must independently observe, verify and compare both providers');
  }
  if (ready.indexOf('Capture native compatibility, plan and rollback-aware preflight') < 0
      || ready.indexOf('Capture native compatibility, plan and rollback-aware preflight') >= ready.indexOf('Re-observe live Worker identity and D1 ledger before READY')) {
    errors.push('native Worker/D1 readback must follow actual typed policy preflight');
  }
  if (/\b(?:sleep|usleep|timeout)\s+\d+\b/.test(ready) || ready.includes('worker-d1-baseline-clock')) {
    errors.push('read-only Worker/D1 proof must not use an artificial timing barrier');
  }
  if (promotion.includes('fetch-depth: 0')) {
    errors.push('AR11 exact-commit checkouts must not fetch unused full history');
  }
  if (!jobBlock(promotion, 'promote').includes('.provider_observation.method == "INDEPENDENT_LIVE_READBACK"')
      || !jobBlock(promotion, 'promote').includes('.provider_observation.worker_identity_stable == true')
      || !jobBlock(promotion, 'promote').includes('.provider_observation.d1_ledger_stable == true')) {
    errors.push('staging READY binder must require independently verified native readback');
  }
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
    'needs: [route, preflight-ready]',
    "needs.preflight-ready.result == 'success'",
    "needs.preflight-ready.outputs.terminal_status == 'READ_ONLY_READY'",
    'github.run_attempt == 1',
    'github.event.workflow_run.run_attempt == 1',
    "vars.PROFILEBRIDGE_AUTO_STAGING_ENABLED == 'true'",
    'Bind standing staging authority to exact same-run READY evidence',
    'SELECTED_MAIN_SHA:',
    'SELECTED_RELEASE_SET_ID:',
    'READY_ARTIFACT_ID: ${{ needs.preflight-ready.outputs.ready_artifact_id }}',
    'READY_ARTIFACT_DIGEST: ${{ needs.preflight-ready.outputs.ready_artifact_digest }}',
    'READY_EVIDENCE_SHA256_EXPECTED: ${{ needs.preflight-ready.outputs.ready_evidence_sha256 }}',
    'test "$GITHUB_EVENT_NAME" = workflow_run',
    'test "${{ needs.preflight-ready.result }}" = success',
    'test "${{ needs.preflight-ready.outputs.terminal_status }}" = READ_ONLY_READY',
    'test "$main_sha" = "$READY_SOURCE_SHA"',
    'test "$release_set_id" = "$READY_RELEASE_SET_ID"',
    'actions/workflows/release-set-promotion.yml/runs?head_sha=$main_sha&event=workflow_run',
    '.total_count == 1 and (.workflow_runs | length) == 1',
    '.workflow_runs[0].id == $run and .workflow_runs[0].run_attempt == 1',
    '.workflow_runs[0].event == "workflow_run"',
    '.workflow_runs[0].head_sha == $sha',
    '.workflow_runs[0].status == "in_progress"',
    'actions/workflows/release-set-promotion.yml/runs?event=workflow_run&branch=main&per_page=100',
    '(.workflow_runs | map(select(.id == $current)) | .[0].run_number)',
    '[.workflow_runs[] | select(.run_number < $number)]',
    'max_by(.run_number).id',
    'actions/runs/$predecessor_run_id/jobs?per_page=100',
    'python scripts/ar11-standing-run-proof.py',
    '--history "$RUNNER_TEMP/prior-standing-promotion-runs.json"',
    '--jobs "$RUNNER_TEMP/prior-staging-run-jobs.json"',
    '--current-run-id "$GITHUB_RUN_ID"',
    'prior-standing-proof.json',
    'test "$(git rev-parse HEAD)" = "$main_sha"',
    '[[ "$READY_ARTIFACT_DIGEST" =~ ^[0-9a-f]{64}$ ]]',
    'ready_artifact_api_digest="sha256:$READY_ARTIFACT_DIGEST"',
    '--arg digest "$ready_artifact_api_digest"',
    'actions/artifacts/$READY_ARTIFACT_ID',
    '.id == $artifact_id and .name == $name and .expired == false',
    '.workflow_run.id == $run and .workflow_run.head_sha == $sha',
    'ready_run_attempt',
    'ready_artifact_id',
    'ready_artifact_digest',
    'GITHUB_RUN_ATTEMPT',
    'GITHUB_RUN_ID',
    'READY evidence does not match the exact accepted main and Release Set',
    'STAGING_PROMOTION_STANDING_INTENT',
    "'github.workflow_run.read_only_ready'",
    "'AR11_STANDING_STAGING_READY_V1'",
    'READY_RUN_ID',
    'READY_ARTIFACT_NAME',
    'READY_EVIDENCE_SHA256',
    'intent_digest',
    'Verify exact downloaded READY_TO_MUTATE evidence',
    'native_recovery="$(jq -er',
    '.recovery_strategy == $recovery',
    "jq -er '.promotion_preflight_sha256'",
    'test "$(sha256sum "$ready_root/promotion-plan.json" | cut -d\' \' -f1)" = "$(jq -er \'.promotion_plan_sha256\' "$ready_root/ready-to-mutate.json")"',
    'test "$(sha256sum "$ready_root/release-verify.json" | cut -d\' \' -f1)" = "$(jq -er \'.release_verify_sha256\' "$ready_root/ready-to-mutate.json")"',
    'gh run download "$GITHUB_RUN_ID"',
    '(.path == ".github/workflows/release-set-promotion.yml@main" or .path == ".github/workflows/release-set-promotion.yml")',
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
    'Pin exact standing staging account and Worker before deploy credentials',
    'EXPECTED_STAGING_ACCOUNT_ID: ${{ vars.AR11_STAGING_ACCOUNT_ID }}',
    'docs/evidence/ar11-fc6-cloudflare-observe-token-policy.json',
    '(.control_plane.account_id // .account_id) == $expected',
    '"browser-profile-control-plane-staging"',
    '"staging.alegria.by"',
  ], 'standing READY binder'));
  const readyBindStep = stepBlock(promote, 'Bind standing staging authority to exact same-run READY evidence');
  const readyDownloadStep = stepBlock(promote, 'Verify exact downloaded READY_TO_MUTATE evidence');
  errors.push(...forbidMarkers(`${readyBindStep}\n${readyDownloadStep}`, [
    'secrets.CLOUDFLARE_', 'wrangler deploy', 'promotion plan', 'promotion preflight',
    'SCOPED_EXCEPTION', 'AUTHORITY_COMMENT', 'WORKER_PROMOTION_AUTHORIZATION_V2',
    'authorization_comment_id', 'issues/comments/', 'branches/main',
    'STAGING_CANDIDATE', 'issues/870', 'issues: read',
  ], 'standing READY binder'));
  const stagingResolve = stepBlock(promote, 'Capture staging resolve phase state');
  errors.push(...requireMarkers(stagingResolve, [
    'steps.resolve.outcome',
    'failed_boundary="ACCEPTED_MAIN_RELEASE_RESOLUTION"',
    'step_outcomes:{resolve:$resolve',
  ], 'staging resolution failure localization'));
  for (const [name, label] of [
    ['Capture staging mutation phase state', 'mutation'],
    ['Capture staging post-verify phase state', 'post-verify'],
  ]) {
    errors.push(...requireMarkers(stepBlock(promote, name), [
      'test -n "$source_sha" || source_sha="$SOURCE_SHA"',
      '--arg source_sha "$source_sha"',
      '--arg intent_digest "${{ steps.intent.outputs.intent_digest }}"',
      'release_set_id:(if $release_set_id=="" then null else $release_set_id end)',
      'target_capability_profile_id:(if $target_profile=="" then null else $target_profile end)',
      'intent_digest:(if $intent_digest=="" then null else $intent_digest end)',
      'authorization_kind:"STANDING_AR11_READY"',
    ], `staging ${label} early-failure transport`));
  }

  errors.push(...requireMarkers(promote, [
    'Execute exact staging candidate after same-run READY evidence',
    'environment: staging',
    'TARGET_PROFILE',
    'EXPECTED_CURRENT_PROFILE',
    'INTENT_DIGEST',
    '[[ "$INTENT_DIGEST" =~ ^[0-9a-f]{64}$ ]]',
    'intent_digest:$intent_digest',
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
  const deployCommands = logicalShellLines(promotion).filter((line) => /\bwrangler@4\.94\.0 deploy\b/.test(line));
  const realDeployCommands = deployCommands.filter((line) => !/\s--dry-run\b/.test(line));
  const deployStep = stepBlock(promote, 'Deploy exact Release Set v3 bits after all fences');
  if (deployCommands.length !== 2 || realDeployCommands.length !== 1
      || !logicalShellLines(deployStep).includes(realDeployCommands[0] ?? '__MISSING_REAL_DEPLOY__')) {
    errors.push(`one-runner promotion must execute exactly one real Worker deploy, scoped to the protected deploy step, plus one dry-run; total=${deployCommands.length} real=${realDeployCommands.length}`);
  }
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
  const stagingTargetPin = promote.indexOf('Pin exact standing staging account and Worker before deploy credentials');
  const deployCredential = promote.indexOf('CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}');
  const refence = promote.indexOf('Re-fence Worker identity and D1 head immediately before mutation');
  const actualDeploy = promote.indexOf('Deploy exact Release Set v3 bits after all fences');
  if (!(readyBind >= 0 && stagingTargetPin > readyBind && refence > stagingTargetPin
      && actualDeploy > refence && deployCredential > actualDeploy)) {
    errors.push('mutation order must be READY binding -> staging target pin -> exact-current re-fence -> scoped deploy credential');
  }
  errors.push(...forbidMarkers(promote, [
    'AR11_STAGING_VARIABLES_READ_TOKEN',
    'actions/variables/PROFILEBRIDGE_AUTO_STAGING_ENABLED',
    'Recheck live operator kill switch immediately before mutation',
  ], 'no extra runtime variable-read credential'));

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

  errors.push(...requireMarkers(stagingOutcome, [
    "needs: [route, promote]",
    "if: always() && needs.route.outputs.mode == 'preflight' && needs.promote.result != 'skipped'",
    'TARGET_PROFILE:',
    'Terminalize one lossless staging AR11 OperationalOutcome',
    '--mode staging',
    '--profile-id "$profile_id"',
    'Upload terminal staging AR11 OperationalOutcome evidence',
    'evidence_artifact="ar11-staging-operational-outcome-$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT"',
    'name: ar11-staging-operational-outcome-${{ github.run_id }}-${{ github.run_attempt }}',
    'Enforce terminal staging AR11 disposition after evidence publication',
    '.contract == "PROMOTION_OPERATOR_OUTCOME_V1"',
    '.procedure == "AR11_RELEASE_SET_PROMOTION"',
    '.status == "COMPLETED"',
    '.provider_mutation_started == true',
    '.provider_mutation_executed == true',
    '.production_mutation_executed == false',
    '.effect_state == "EFFECT_VERIFIED"',
    '.authorization_state == "AR11_STANDING_STAGING_READY"',
    'INTENT_DIGEST:',
  ], 'staging terminal outcome'));
  const readOnlyTerminalArtifact = stepBlock(ready, 'Upload terminal AR11 OperationalOutcome evidence')
    .match(/^\s*name: (ar11-[^\n]+)$/m)?.[1];
  const stagingTerminalArtifact = stepBlock(stagingOutcome, 'Upload terminal staging AR11 OperationalOutcome evidence')
    .match(/^\s*name: (ar11-[^\n]+)$/m)?.[1];
  if (!readOnlyTerminalArtifact || !stagingTerminalArtifact || readOnlyTerminalArtifact === stagingTerminalArtifact) {
    errors.push('read-only and staging terminal evidence must use distinct artifact names in the same workflow run');
  }
  errors.push(...forbidMarkers(stagingOutcome, ['secrets.CLOUDFLARE_', 'wrangler deploy --', 'environment: production', '--profile-id rehearsal-core-v2'], 'staging terminal outcome'));
  const stagingTerminalize = stagingOutcome.indexOf('Terminalize one lossless staging AR11 OperationalOutcome');
  const stagingUpload = stagingOutcome.indexOf('Upload terminal staging AR11 OperationalOutcome evidence');
  const stagingEnforce = stagingOutcome.indexOf('Enforce terminal staging AR11 disposition after evidence publication');
  if (!(stagingTerminalize >= 0 && stagingUpload > stagingTerminalize && stagingEnforce > stagingUpload)) {
    errors.push('staging AR11 must terminalize -> publish evidence -> enforce final disposition');
  }

  errors.push(...requireMarkers(rollback, [
    "if: needs.route.outputs.mode == 'rollback-negative'",
    'event == "workflow_run"',
    '.run_attempt == 1',
    'ar11-ready-to-mutate-$RELEASE_SET_ID-${{ steps.bind.outputs.main_sha }}-$SOURCE_RUN_ID-1',
    '.source_sha == $sha and .ready_run_id == $run and .ready_run_attempt == 1',
    '(.path == ".github/workflows/release-set-promotion.yml@main" or .path == ".github/workflows/release-set-promotion.yml")',
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
    'path.startswith("frontend/src/") and path.endswith((".ts", ".tsx", ".css"))',
    '"frontend-only"',
    '"frontend-package-is-heavy"',
    '"frontend-generated-is-heavy"',
    '"frontend-cargo-is-heavy"',
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
    'architecture/ar11-behavioural-certification.json',
    'scripts/download-release-set-ar11.py',
    'scripts/promotion-operational-outcome-ar11.py',
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
    'architecture/release-architecture-ar11.json',
    'scripts/classify-camoufox-runtime-scope.py',
    '.github/workflows/ci.yml',
  ], 'Camoufox proven-unrelated classifier sets'));
  errors.push(...requireMarkers(classifier, [
    'runtime/camouhost/launcher.py',
    'apps/profile-bridge/src/main.rs',
    'Cargo.lock',
    'architecture/release-architecture-ar11.json',
    'scripts/classify-camoufox-runtime-scope.py',
    '.github/workflows/ci.yml',
    'scripts/test-browser-mail-execution-d1.py',
    'new-subsystem/file.txt',
  ], 'Camoufox fail-closed heavy fixtures'));
  return errors;
}

function camoufoxErrors(workflow, classifier) {
  const errors = [];
  const scope = jobBlock(workflow, 'camoufox-scope');
  const linux = jobBlock(workflow, 'camoufox-real-runtime-linux');
  const windows = jobBlock(workflow, 'camoufox-runtime-package-windows');
  const proof = jobBlock(workflow, 'camoufox-camoufox-required-proof');
  if (!scope || !linux || !windows || !proof) {
    return ['Camoufox single Windows physical proof topology is incomplete'];
  }
  const physicalOwner = [scope, linux, windows, proof].join('\n');
  errors.push(...forbidMarkers(workflow, [
    '  bridge-regression-windows:',
    'needs.bridge-regression-windows',
    '  shipping-archive-reader-windows:',
    'needs.shipping-archive-reader-windows',
    '  camoufox-bridge-regression-windows:',
    '  camoufox-shipping-archive-reader-windows:',
  ], 'Camoufox single Windows proof (obsolete extra Windows job)'));
  const uploadCount = (physicalOwner.match(/uses:\s+actions\/upload-artifact@/g) ?? []).length;
  const downloadCount = (physicalOwner.match(/uses:\s+actions\/download-artifact@/g) ?? []).length;
  // The physical tests stay on one Windows runner. A tiny receipt is always
  // exported after PASS; actual 1.5 GiB bytes cross to main only on a new
  // runtime input (never on a published-byte reuse hit).
  if (uploadCount !== 3 || downloadCount !== 0) {
    errors.push(`Camoufox exact physical custody must upload receipt and miss-only raw assets, got ${uploadCount} uploads/${downloadCount} downloads`);
  }
  errors.push(...requireMarkers(scope, [
    'Classify accepted-main or mechanically proven runtime-unrelated PR',
    'fetch-depth: 0',
    'filter: blob:none',
    'persist-credentials: false',
    'if [ "$GITHUB_EVENT_NAME" = push ]; then',
    "echo 'heavy=true'",
    "echo 'heavy=true' >> \"$GITHUB_OUTPUT\"\n              echo 'No exact verified PR proof: full main Camoufox replay required.'",
    'python3.12 scripts/accepted-main-pr-reuse.py --self-test',
    'python3.12 scripts/accepted-main-pr-reuse.py --gate camoufox',
    "grep -qx 'reuse=true'",
    "echo 'heavy=false'",
    "echo 'heavy=true'",
    'python3.12 -m py_compile scripts/classify-camoufox-runtime-scope.py',
    'python3.12 scripts/classify-camoufox-runtime-scope.py --self-test',
    '--paths-file "$RUNNER_TEMP/changed-paths.txt"',
    '--github-output "$RUNNER_TEMP/camoufox-classified.txt"',
    "grep -qx 'heavy=false' \"$RUNNER_TEMP/camoufox-classified.txt\"",
    'python3.12 scripts/accepted-main-pr-reuse.py --pr-camoufox-byte-proof',
    "echo 'heavy=true' >> \"$GITHUB_OUTPUT\"\n              echo 'No published-byte physical receipt: execute complete Gate.'",
    "echo 'heavy=true' >> \"$GITHUB_OUTPUT\"",
  ], 'Camoufox scope routing'));
  errors.push(...camoufoxClassifierErrors(classifier));
  errors.push(...requireMarkers(linux, [
    'needs: camoufox-scope',
    'if: always()',
    'Require successful scope classification',
    'needs.camoufox-scope.outputs.heavy',
  ], 'Linux required context'));
  const protectedRunnerSelection = "runs-on: ${{ needs.camoufox-scope.outputs.heavy == 'true' && 'windows-2025' || 'ubuntu-24.04' }}";
  errors.push(...requireMarkers(windows, [
    // Preserve the protected branch-status context while one physical Windows
    // owner tests published bytes. An independently SHA-attested LIGHT can
    // resolve on Ubuntu without occupying the Windows runner queue.
    'name: Profile Bridge Windows regression',
    'needs: camoufox-scope',
    'if: always()',
    protectedRunnerSelection,
    'timeout-minutes: 20',
    'Require successful scope classification',
    "-notin @('true','false')",
  ], 'protected Windows runtime build, shipping reader and native browser proof'));
  const scopeGuard = stepBlock(windows, 'Require successful scope classification');
  if (windows.split('\n').filter((line) => /^    runs-on:/.test(line)).join('') !== '    ' + protectedRunnerSelection) {
    errors.push('protected Windows runtime build must use exact Windows-on-HEAVY and Ubuntu-on-attested-LIGHT runner selection');
  }
  if (windows.split('\n').filter((line) => /^    timeout-minutes:/.test(line)).join('') !== '    timeout-minutes: 20') {
    errors.push('Windows physical proof must enforce a 20-minute job deadline');
  }
  const activeScopeGuard = scopeGuard.split('\n').map((line) => line.trim()).filter(Boolean);
  if (activeScopeGuard.filter((line) => line === "if ('${{ needs.camoufox-scope.result }}' -ne 'success') { throw 'Camoufox scope classification failed' }").length !== 1
      || activeScopeGuard.filter((line) => line === "if ('${{ needs.camoufox-scope.outputs.heavy }}' -notin @('true','false')) { throw 'invalid Camoufox proof scope' }").length !== 1
      || activeScopeGuard.some((line) => line.startsWith('if: '))) {
    errors.push('Windows scope prerequisite must fail closed for invalid or failed scope');
  }

  const jobsSection = workflow.split('\njobs:\n')[1] ?? '';
  const jobIds = [...jobsSection.matchAll(/^  (camoufox-[A-Za-z0-9_-]+):\s*$/gm)].map((match) => match[1]);
  const expectedJobIds = ['camoufox-scope', 'camoufox-real-runtime-linux',
    'camoufox-runtime-package-windows', 'camoufox-camoufox-required-proof'];
  if (jobIds.length !== expectedJobIds.length || expectedJobIds.some((id) => jobIds.filter((actual) => actual === id).length !== 1)) {
    errors.push('Camoufox proof must contain exactly four unique scope/Linux/Windows/final jobs');
  }

  // A step title is not proof. Require ordered executed commands on one
  // protected runner, plus exact source, archive and physical PASS checks.
  const windowsSteps = [
    ['Checkout exact candidate', ['actions/checkout@f548e57e544e1ff5a4c46bf1e1b8685f8e4a348a', 'persist-credentials: false']],
    ['Verify exact source checkout', ['EXPECTED_SOURCE_SHA:', "['git', 'rev-parse', 'HEAD']"]],
    ['Set up exact build Python', ["python-version: '3.12.10'"]],
    ['Restore versioned Rust dependencies', [
      'Swatinem/rust-cache@6323deb102c322ba6fcbdcafc7e3dddab59af2b6',
      'shared-key: profile-bridge-ci-v1',
    ]],
    ['Validate existing runtime owners', [
      'python scripts/test-camouhost-native-process.py',
      'python scripts/test-camouhost-native-ipc.py',
      'python scripts/test-camouhost-execution-modes.py',
      'python scripts/camouhost-runtime-package.py self-test',
    ]],
    ['Materialize official browser and verified offline fpgen model', [
      'python scripts/camouhost-runtime-build-windows.py build',
      '--runtime-lock runtime/camouhost/runtime-lock.json',
      'python scripts/camouhost-runtime-package.py identity',
      'python scripts/github-release-publication.py probe',
      'gh release download',
      'Get-FileHash -Algorithm SHA256',
    ]],
    ['Package exact compressed Release Set runtime input', [
      'python scripts/camouhost-runtime-package.py package --runtime-root',
      'runtime-bundle.tar.gz',
      'runtime-manifest.json',
      'python scripts/camouhost-runtime-package.py verify-manifest',
      "Get-FileHash -Algorithm SHA256 -LiteralPath $archive",
      "Join-Path $env:RUNNER_TEMP 'runtime-bundle.sha256'",
      "Set-Content -LiteralPath (Join-Path $env:RUNNER_TEMP 'runtime-bundle.sha256')",
    ]],
    ['Extract and verify exact official runtime', [
      "Join-Path $env:RUNNER_TEMP 'runtime-bundle.tar.gz'",
      "Join-Path $env:RUNNER_TEMP 'runtime-manifest.json'",
      'camouhost-delivered-runtime',
      "if (Test-Path -LiteralPath $root) { throw 'shipping extraction must use a clean directory' }",
      'tar.exe -xzf $archive -C $root',
      'Get-FileHash -Algorithm SHA256',
      "Get-Content -LiteralPath (Join-Path $env:RUNNER_TEMP 'runtime-bundle.sha256')",
      'if ($actualArchiveSha -cne $expectedArchiveSha)',
      'python scripts/camouhost-runtime-package.py verify-manifest',
      'from camoufox.fpgen_model import is_pinned',
    ]],
    ['Prove official Firefox native restored tabs and POST safety on exact runtime', [
      'camouhost-delivered-runtime',
      'python scripts/test-ar10-real-camoufox-cleanup.py',
      '--native-restore-only',
      'native_restore_proof stage=passed safe_get=PASS post_replay=0 user_post_after_admission=PASS',
      'AR10_NATIVE_FIREFOX_SESSIONSTORE=PHYSICAL_PASS',
    ]],
    ['Run physical shipping reader and native Bridge Open on exact packaged runtime', [
      'camouhost-bridge-clean-runtime',
      "if (Test-Path -LiteralPath $root) { throw 'Bridge runtime extraction must use a clean directory' }",
      "if ($actualArchiveSha -cne $expectedArchiveSha) { throw 'physical runtime archive changed after SessionStore proof' }",
      'tar.exe -xzf $archive -C $root',
      'clean Bridge runtime embedded manifest mismatch',
      '$env:AR10_PACKAGED_RUNTIME_ARCHIVE = $archive',
      '$env:AR10_PACKAGED_RUNTIME_MANIFEST = $manifest',
      "$env:AR10_REAL_CAMOUFOX = '1'",
      "$env:AR10_REAL_CAMOUFOX_HEADLESS = 'false'",
      'cargo test --locked -p profile-bridge --test real_camoufox_runtime',
      '--test-threads=2',
      'running 2 tests',
      '2 passed; 0 failed',
      'shipping_windows_runtime_archive_is_readable_by_production_delivery_reader',
      'bridge_preflight_launches_real_camoufox_and_acquires_native_profile_lock',
      'test shipping_windows_runtime_archive_is_readable_by_production_delivery_reader \\.\\.\\. ok',
      'test bridge_preflight_launches_real_camoufox_and_acquires_native_profile_lock \\.\\.\\. ok',
      'REAL_SHIPPING_ARCHIVE_READER=PASS;FILES=[0-9]+;COMPRESSED_BYTES=[0-9]+',
      'AR10_NATIVE_OPEN_LOCK=PASS;elapsed_ms=[0-9]+',
      '& "$env:WINDIR\\System32\\cmd.exe" /d /s /c $testCommand',
      '$cargoExit = $LASTEXITCODE',
      'if ($cargoExit -ne 0)',
    ]],
    ['Bind exact physically tested runtime archive and manifest SHA', [
      'Get-FileHash -Algorithm SHA256 -LiteralPath $archive',
      'Get-FileHash -Algorithm SHA256 -LiteralPath $manifest',
      'CAMOUFOX_PHYSICAL_RUNTIME_SHA256=',
    ]],
  ];
  for (const [jobName, job, steps] of [['camoufox-runtime-package-windows', windows, windowsSteps]]) {
    let previousOffset = -1;
    for (const [name, markers] of steps) {
      const stepNameMarker = `      - name: ${name}`;
      const offset = job.indexOf(stepNameMarker);
      const count = job.split(stepNameMarker).length - 1;
      if (count !== 1 || offset <= previousOffset) {
        errors.push(`Windows ${jobName} physical proof step is missing, duplicated or out of order: ${name}`);
      }
      previousOffset = offset;
      const step = stepBlock(job, name);
      const stepConditions = step.split('\n').filter((line) => /^        if: /.test(line));
      const expectedConditions = ["        if: needs.camoufox-scope.outputs.heavy == 'true'"];
      if (JSON.stringify(stepConditions) !== JSON.stringify(expectedConditions)) {
        errors.push(`Windows ${jobName} physical proof step can bypass exact HEAVY classification: ${name}`);
      }
      errors.push(...requireMarkers(step, markers, `Windows ${jobName} physical proof step ${name}`));
      if (name === 'Extract and verify exact official runtime'
          && (step.match(/Get-FileHash -Algorithm SHA256/g) ?? []).length !== 3) {
        errors.push('Windows extraction must SHA-256 verify compressed archive and both embedded/detached manifests');
      }
    }
  }
  const publishedAcquisition = stepBlock(windows, 'Materialize official browser and verified offline fpgen model');
  errors.push(...requireMarkers(publishedAcquisition, [
    '$assetCandidates = @($assets | Where-Object',
    '$assetCandidates.Count -ne 1',
    '$assetCandidates[0].digest -notmatch',
    '(Get-Item -LiteralPath $path).Length -ne $assetCandidates[0].size',
    '$assetCandidates[0].digest -cne "sha256:$actual"',
  ], 'Camoufox published-runtime candidate SHA and size verification'));
  // PowerShell variables are case-insensitive: $matches would alias the
  // automatic $Matches dictionary mutated by -match/-notmatch on digest.
  if (/\$matches\s*=/i.test(publishedAcquisition)) {
    errors.push('Camoufox published-runtime proof shadows PowerShell automatic $Matches');
  }
  for (const [jobName, name, command] of CAMOUFOX_ACTIVE_PHYSICAL_COMMANDS) {
    errors.push(...requireActiveShellCommand(
      stepBlock(windows, name), command, `Windows physical proof step ${name}`,
    ));
  }
  const custodySteps = [
    ['Upload exact PR-tested runtime archive only on publication miss',
      "needs.camoufox-scope.outputs.heavy == 'true' && github.event_name == 'pull_request' && steps.custody.outputs.origin == 'built'",
      'camoufox-runtime-bundle-', 'runtime-bundle.tar.gz'],
    ['Upload exact PR-tested runtime manifest only on publication miss',
      "needs.camoufox-scope.outputs.heavy == 'true' && github.event_name == 'pull_request' && steps.custody.outputs.origin == 'built'",
      'camoufox-runtime-manifest-', 'runtime-manifest.json'],
    ['Upload SHA-bound physical runtime proof receipt',
      "needs.camoufox-scope.outputs.heavy == 'true' && github.event_name == 'pull_request'",
      'camoufox-runtime-proof-', 'camoufox-runtime-proof.json'],
  ];
  for (const [name, expectedCondition, identity, file] of custodySteps) {
    const step = stepBlock(windows, name);
    const conditions = step.split('\n').filter((line) => /^        if: /.test(line));
    if (conditions.length !== 1 || conditions[0] !== '        if: ' + expectedCondition) {
      errors.push('Camoufox physical runtime custody upload has unsafe condition: ' + name);
    }
    errors.push(...requireMarkers(step, [
      'actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a',
      'archive: false', 'if-no-files-found: error',
      name.includes('proof receipt') ? 'retention-days: 30' : 'retention-days: 7',
      identity, file,
    ], 'Camoufox raw physical runtime custody ' + name));
  }
  for (const [name, markers] of [
    ['Verify physical PR runtime uploads match exact tested SHA',
      ['ARCHIVE_UPLOAD_SHA:', 'MANIFEST_UPLOAD_SHA:', 'archive_sha256', 'manifest_sha256',
       "if ($env:ARCHIVE_UPLOAD_SHA -cne $receipt.archive_sha256 -or $env:MANIFEST_UPLOAD_SHA -cne $receipt.manifest_sha256)"]],
    ['Verify uploaded physical runtime proof receipt digest',
      ['RECEIPT_UPLOAD_SHA:', 'Get-FileHash -Algorithm SHA256',
       "if ($env:RECEIPT_UPLOAD_SHA -cne $actual)"]],
  ]) {
    errors.push(...requireMarkers(stepBlock(windows, name), markers,
      'Camoufox exact physical runtime custody ' + name));
  }
  const custodyOrder = [
    'Run physical shipping reader and native Bridge Open on exact packaged runtime',
    'Bind exact physically tested runtime archive and manifest SHA',
    'Upload exact PR-tested runtime archive only on publication miss',
    'Upload exact PR-tested runtime manifest only on publication miss',
    'Verify physical PR runtime uploads match exact tested SHA',
    'Upload SHA-bound physical runtime proof receipt',
    'Verify uploaded physical runtime proof receipt digest',
  ].map((name) => windows.indexOf('      - name: ' + name));
  if (custodyOrder.some((offset, index) => offset < 0 ||
      (index > 0 && offset <= custodyOrder[index - 1]))) {
    errors.push('Camoufox raw runtime custody steps must follow both completed physical tests');
  }
  for (const name of [
    'Verify physical PR runtime uploads match exact tested SHA',
    'Verify uploaded physical runtime proof receipt digest',
  ]) {
    const expected = "        if: needs.camoufox-scope.outputs.heavy == 'true' && github.event_name == 'pull_request'";
    if (!stepBlock(windows, name).split('\n').includes(expected)) {
      errors.push('Camoufox custody digest verdict can skip a physical PR: ' + name);
    }
  }
  // Both SHA-256 observations and the comparison must be *executed* commands,
  // not marker text in a disabled condition, comment or forged log. The local
  // receipt binds the exact compressed archive tested by the shipping reader.
  const archiveIdentityCommands = [
    ['Package exact compressed Release Set runtime input', "$archiveSha = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant()"],
    ['Package exact compressed Release Set runtime input', "if ($archiveSha -notmatch '^[0-9a-f]{64}$') { throw 'runtime archive SHA-256 is invalid' }"],
    ['Package exact compressed Release Set runtime input', "Set-Content -LiteralPath (Join-Path $env:RUNNER_TEMP 'runtime-bundle.sha256') -Value $archiveSha -NoNewline -Encoding ascii"],
    ['Extract and verify exact official runtime', "$expectedArchiveSha = (Get-Content -LiteralPath (Join-Path $env:RUNNER_TEMP 'runtime-bundle.sha256') -Raw).Trim()"],
    ['Extract and verify exact official runtime', "$actualArchiveSha = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant()"],
    ['Extract and verify exact official runtime', "if ($actualArchiveSha -cne $expectedArchiveSha) { throw 'runtime archive bytes changed after packaging' }"],
    ['Extract and verify exact official runtime', "if ((Get-FileHash -Algorithm SHA256 $embedded).Hash -ne (Get-FileHash -Algorithm SHA256 $externalManifest).Hash) {"],
  ];
  for (const [name, command] of archiveIdentityCommands) {
    errors.push(...requireActiveShellCommand(
      stepBlock(windows, name), command, `Windows exact runtime SHA-256 proof step ${name}`,
    ));
  }
  const pairedProof = stepBlock(windows, 'Run physical shipping reader and native Bridge Open on exact packaged runtime');
  for (const command of [
    "if (Test-Path -LiteralPath $root) { throw 'Bridge runtime extraction must use a clean directory' }",
    "if ($actualArchiveSha -cne $expectedArchiveSha) { throw 'physical runtime archive changed after SessionStore proof' }",
    'tar.exe -xzf $archive -C $root',
    "if ($LASTEXITCODE -ne 0) { throw 'clean Bridge runtime extraction failed' }",
  ]) {
    errors.push(...requireActiveShellCommand(
      pairedProof, command, 'Windows clean Bridge runtime extraction must be active'));
  }
  if (pairedProof.split('\n').filter((line) => /^        timeout-minutes:/.test(line)).join('') !== '        timeout-minutes: 8') {
    errors.push('Windows physical Cargo proof must enforce an 8-minute step deadline');
  }
  const activeCargoAssignments = pairedProof.split('\n').map((line) => line.trim())
    .filter((line) => line.startsWith("$testCommand = 'cargo test --locked -p profile-bridge --test real_camoufox_runtime"));
  const canonicalCargoAssignment = "$testCommand = 'cargo test --locked -p profile-bridge --test real_camoufox_runtime -- --skip published_windows_runtime_is_consumed_by_shipping_selector --test-threads=2 --show-output > \"' + $log + '\" 2>&1'";
  const cargoCommands = (pairedProof.match(/cargo test --locked -p profile-bridge --test real_camoufox_runtime/g) ?? []).length;
  if (activeCargoAssignments.length !== 1 || cargoCommands !== 1
      || activeCargoAssignments[0] !== canonicalCargoAssignment) {
    errors.push('Windows protected consumer must execute both real Rust tests concurrently in one exact cargo invocation');
  }
  const requiredActiveChecks = [
    "if ($text -notmatch 'running 2 tests' -or",
    "$text -notmatch 'test result: ok\\. 2 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out;' -or",
    "$text -notmatch '(?m)^test shipping_windows_runtime_archive_is_readable_by_production_delivery_reader \\.\\.\\. ok\\r?$' -or",
    "$text -notmatch '(?m)^test bridge_preflight_launches_real_camoufox_and_acquires_native_profile_lock \\.\\.\\. ok\\r?$' -or",
    "$text -notmatch 'REAL_SHIPPING_ARCHIVE_READER=PASS;FILES=[0-9]+;COMPRESSED_BYTES=[0-9]+' -or",
    "$text -notmatch 'AR10_NATIVE_OPEN_LOCK=PASS;elapsed_ms=[0-9]+'",
  ];
  const activeProofLines = pairedProof.split('\n').map((line) => line.trim()).filter((line) => !line.startsWith('#'));
  for (const check of requiredActiveChecks) {
    if (activeProofLines.filter((line) => line.startsWith(check)).length !== 1) {
      errors.push(`Windows protected consumer must actively enforce both test results and PASS evidence: ${check}`);
    }
  }
  for (const [job, name, quietCommand] of [
    [windows, 'Package exact compressed Release Set runtime input', 'verify-manifest --manifest "$manifest" > $null'],
    [windows, 'Extract and verify exact official runtime', 'verify-manifest --manifest $externalManifest > $null'],
  ]) {
    if (!stepBlock(job, name).includes(quietCommand)) {
      errors.push(`Windows ${name} must retain fail-closed quiet runtime-manifest verification`);
    }
  }
  errors.push(...requireMarkers(proof, [
    'needs: [camoufox-scope, camoufox-runtime-package-windows]',
    'if: always()',
    'Require exact official Camoufox and Bridge Windows proof',
    'test "${{ needs.camoufox-scope.result }}" = success',
    'test "${{ needs.camoufox-runtime-package-windows.result }}" = success',
  ], 'Camoufox required-proof aggregator'));
  if (!proof.split('\n').includes('    needs: [camoufox-scope, camoufox-runtime-package-windows]')) {
    errors.push('Camoufox required-proof aggregator lost its protected Windows dependency');
  }
  const finalStep = stepBlock(proof, 'Require exact official Camoufox and Bridge Windows proof');
  if (!/run: \|\n\s*test "\$\{\{ needs\.camoufox-scope\.result \}\}" = success\n\s*test "\$\{\{ needs\.camoufox-runtime-package-windows\.result \}\}" = success(?:\n|$)/.test(finalStep)) {
    errors.push('Camoufox required-proof aggregator must unconditionally require both scope and Windows success');
  }
  if (!windows.split('\n').includes('    if: always()')) {
    errors.push('Camoufox protected Windows status must run even for LIGHT or failed scope');
  }
  if (!linux.includes("if: needs.camoufox-scope.outputs.heavy == 'true'")) errors.push('Linux heavy proof is not gated by exact scope output');
  return errors;
}

function validateAll({ build, promotion, camoufox, camoufoxClassifier, downloader, overlay, standingProof, authority }) {
  return [
    ...buildErrors(build),
    ...promotionErrors(promotion),
    ...standingPredecessorProofErrors(standingProof),
    ...runtimeSidecarErrors(promotion, downloader, overlay),
    ...camoufoxErrors(camoufox, camoufoxClassifier),
    ...authorityErrors(authority),
    ...legacyAuthorityErrors(),
    ...(existsSync(path.join(ROOT, ASSET_MATERIALIZER)) ? [] : [`missing Release Set asset materializer: ${ASSET_MATERIALIZER}`]),
  ];
}

function selfTest(files) {
  for (const [label, original, replacement] of [
    ['missing second Worker GET', 'http_code="$(curl --silent --show-error --output "$RUNNER_TEMP/deployments-after-api.json"', 'http_code="$(echo 200'],
    ['missing second Worker version verification', 'python scripts/deployment-identity-ar11.py --observe-version', 'echo skipped-version-identity'],
    ['missing D1 live readback', 'npx --yes wrangler@4.94.0 d1 execute CATALOG_DB --remote', 'echo stale-d1-ledger'],
    ['missing second Worker comparison', 'cmp --silent "$RUNNER_TEMP/worker-binding-before.json" "$RUNNER_TEMP/worker-binding-after.json"', 'echo worker-comparison-bypassed'],
    ['missing second D1 comparison', 'cmp --silent "$RUNNER_TEMP/d1-names-before.json" "$RUNNER_TEMP/d1-names-after.json"', 'echo d1-comparison-bypassed'],
    ['forged READY readback proof', 'provider_observation:{method:"INDEPENDENT_LIVE_READBACK",worker_identity_stable:true,d1_ledger_stable:true}', 'provider_observation:{method:"INDEPENDENT_LIVE_READBACK",worker_identity_stable:true,d1_ledger_stable:false}'],
    ['non-readback staging READY accepted', '.provider_observation.method == "INDEPENDENT_LIVE_READBACK"', 'true'],
    ['serial sleep reintroduced', 'Re-observe live Worker identity and D1 ledger before READY', 'Re-observe live Worker identity and D1 ledger before READY\n          sleep 30'],
    ['unneeded deep checkout restored', 'fetch-depth: 1', 'fetch-depth: 0'],
  ]) {
    const secondReadbackFixture = label === 'missing second Worker GET'
      || label === 'missing second Worker version verification'
      || label === 'missing D1 live readback';
    const secondReadbackStep = stepBlock(jobBlock(files.promotion, 'preflight-ready'), 'Re-observe live Worker identity and D1 ledger before READY');
    const altered = secondReadbackFixture
      ? files.promotion.replace(secondReadbackStep, secondReadbackStep.replace(original, replacement))
      : files.promotion.replace(original, replacement);
    if (altered === files.promotion || !promotionErrors(altered).length) {
      throw new Error(`unsafe ${label} native readback/depth fixture passed`);
    }
  }
  for (const [label, original, replacement] of [
    ['missing main PR physical byte proof', 'python scripts/accepted-main-pr-reuse.py --release-component runtime', 'Write-Host "pretend physical bytes existed"'],
    ['missing main runtime published byte equality', 'published runtime bytes differ from exactly physically tested PR bytes', 'runtime bytes assumed equal'],
    ['restored main runtime rebuild', 'Verify exact PR-tested runtime component on cache miss', 'camouhost-runtime-build-windows.py build'],
  ]) {
    const altered = files.build.replace(original, replacement);
    if (altered === files.build || !buildErrors(altered).length) {
      throw new Error(label + ' custody negative fixture unexpectedly passed');
    }
  }
  const positiveSidecarErrors = runtimeSidecarErrors(files.promotion, files.downloader, files.overlay);
  if (positiveSidecarErrors.length) throw new Error(`positive runtime sidecar fixture failed: ${positiveSidecarErrors.join('; ')}`);
  for (const [label, original, replacement] of [
    ['disabled full verify', '--root . release verify', '--root . release inspect'],
    ['disabled full verify verdict', '.decision == "VALID"', '.decision != "INVALID"'],
    ['disabled sidecar download', '--runtime-manifest-release-id "$runtime_id"', '--release-set-id "$runtime_id"'],
    ['ignored sidecar download failure', '--destination "$runtime_sidecar_dir"', '--destination "$runtime_sidecar_dir" || true'],
    ['missing target sidecar binding', '--runtime-manifest "$TARGET_RUNTIME_MANIFEST"', '--runtime-manifest "$FORGED_RUNTIME_MANIFEST"'],
    ['missing known-good digest comparison', 'test "$previous_manifest_sha" = "$target_manifest_sha"', 'test -n "$target_manifest_sha"'],
    ['known-good identity substitution', 'if [ "$previous_runtime_id" = "$target_runtime_id" ]; then', 'if [ "$previous_runtime_id" != "$target_runtime_id" ]; then'],
    ['known-good source substitution', 'previous_runtime_id="$(jq -er \'.components.runtime_bundle.release_id\' "$current_release_manifest")"', 'previous_runtime_id="$(jq -er \'.components.runtime_bundle.release_id\' "$TARGET_RELEASE_MANIFEST")"'],
    ['known-good sidecar route disabled', 'previous_runtime_manifest_args=(--previous-runtime-manifest "$previous_runtime_manifest")', 'previous_runtime_manifest_args=()'],
  ]) {
    const altered = files.promotion.replace(original, replacement);
    if (altered === files.promotion || !runtimeSidecarErrors(altered, files.downloader, files.overlay).length) {
      throw new Error(`${label} runtime sidecar negative fixture unexpectedly passed`);
    }
  }
  for (const [label, file, oldText, newText] of [
    ['sidecar SHA validation disabled', 'overlay', 'hashlib.sha256(raw).hexdigest() != runtime["component_manifest_sha256"]', 'False'],
    ['sidecar release identity disabled', 'overlay', 'document.get("release_id") != runtime["release_id"]', 'False'],
    ['unbounded sidecar download', 'downloader', 'members[0].stat().st_size <= MAX_RUNTIME_MANIFEST_BYTES', 'True'],
  ]) {
    const changed = files[file].replace(oldText, newText);
    if (changed === files[file] || !runtimeSidecarErrors(files.promotion, file === 'downloader' ? changed : files.downloader, file === 'overlay' ? changed : files.overlay).length) {
      throw new Error(`${label} runtime sidecar source fixture unexpectedly passed`);
    }
  }
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
  const stagingResolve = stepBlock(jobBlock(files.promotion, 'promote'), 'Capture staging resolve phase state');
  for (const [marker, name] of [
    ['failed_boundary="ACCEPTED_MAIN_RELEASE_RESOLUTION"', 'main resolution'],
    ['step_outcomes:{resolve:$resolve', 'resolve evidence'],
  ]) {
    if (!stagingResolve.includes(marker)) throw new Error(`staging ${name} diagnostic fixture target is missing`);
    const withoutDiagnostic = files.promotion.replace(stagingResolve, stagingResolve.replace(marker, 'diagnostic_was_removed'));
    if (!promotionErrors(withoutDiagnostic).some((error) => error.includes('staging resolution failure localization'))) {
      throw new Error(`staging ${name} error localization removal unexpectedly passed`);
    }
  }
  const missingRecovery = files.promotion.replace('.recovery_strategy == $recovery', 'true');
  if (!promotionErrors(missingRecovery).some((error) => error.includes('standing READY binder'))) {
    throw new Error('missing native recovery binding fixture passed');
  }
  const missingRecoveryProjection = files.promotion.replace('recovery_strategy:$recovery_strategy', 'recovery_strategy:null');
  if (!promotionErrors(missingRecoveryProjection).some((error) => error.includes('pre-authorization READY proof'))) {
    throw new Error('missing native recovery projection fixture passed');
  }
  const missingWorkerIdentityProof = files.promotion.replace('worker_identity_sha256:$worker_identity_sha256', 'worker_identity_sha256:null');
  if (!promotionErrors(missingWorkerIdentityProof).some((error) => error.includes('pre-authorization READY proof'))) {
    throw new Error('missing READY-bound full Worker identity fingerprint fixture passed');
  }
  const badSecret = files.promotion.replace('secret list --format json', 'secret list --name "$worker_name" --format json');
  if (secretObservationErrors(jobBlock(badSecret, 'preflight-ready'), 'fixture').length === 0) {
    throw new Error('redundant Worker-name secret observation fixture passed');
  }
  const ambiguousRunName = files.promotion.replace(EXPECTED_PROMOTION_RUN_NAME, 'run-name: AR11 Release Set Promotion');
  if (!promotionErrors(ambiguousRunName).some((error) => error.includes('run-name must distinguish'))) {
    throw new Error('ambiguous promotion run-name fixture unexpectedly passed');
  }
  const manualTrigger = files.promotion.replace('  workflow_call:\n', '  workflow_dispatch:\n  workflow_call:\n');
  if (!promotionErrors(manualTrigger).some((error) => error.includes('manual workflow-dispatch authority'))) throw new Error('retired manual dispatch fixture passed');
  const bypassedReadyGate = files.promotion.replace("needs.preflight-ready.outputs.terminal_status == 'READ_ONLY_READY'", "needs.preflight-ready.outputs.terminal_status != 'BLOCKED'");
  if (!promotionErrors(bypassedReadyGate).some((error) => error.includes('standing READY binder'))) throw new Error('NOOP/BLOCKED READY-gate bypass fixture passed');
  const removedAutoKillSwitch = files.promotion.replace("vars.PROFILEBRIDGE_AUTO_STAGING_ENABLED == 'true'", 'true');
  if (!promotionErrors(removedAutoKillSwitch).some((error) => error.includes('standing READY binder'))) throw new Error('automatic staging kill-switch bypass fixture passed');
  const rerunBypass = files.promotion.replace('github.event.workflow_run.run_attempt == 1', 'github.event.workflow_run.run_attempt >= 1');
  if (!promotionErrors(rerunBypass).some((error) => error.includes('standing READY binder'))) throw new Error('workflow re-run staging bypass fixture passed');
  const sameSourceReplay = files.promotion.replace('.total_count == 1 and (.workflow_runs | length) == 1', '.total_count >= 1');
  if (!promotionErrors(sameSourceReplay).some((error) => error.includes('standing READY binder'))) throw new Error('same-source replay staging bypass fixture passed');
  // Preserve the cross-source replay and failed-run exact-effect negative gates.
  const missingHistoryProof = files.promotion.replace(
    'python scripts/ar11-standing-run-proof.py',
    'echo "unverified predecessor accepted"',
  );
  if (!promotionErrors(missingHistoryProof).some((error) => error.includes('standing READY binder'))) {
    throw new Error('unverified cross-source predecessor bypass fixture passed');
  }
  const priorRunNumberGuardRemoved = files.promotion.replace(
    'max_by(.run_number).id',
    'min_by(.run_number).id',
  );
  if (priorRunNumberGuardRemoved === files.promotion
      || !promotionErrors(priorRunNumberGuardRemoved).some((error) => error.includes('standing READY binder'))) {
    throw new Error('cross-source immediate predecessor selector bypass fixture passed');
  }
  const removedHistoryBound = files.standingProof.replace(
    'len(runs) == min(total, 100)', 'len(runs) <= 100',
  );
  if (!standingPredecessorProofErrors(removedHistoryBound).some((error) => error.includes('AR14 exact pre-effect'))) {
    throw new Error('unbounded/truncated canonical predecessor history fixture passed');
  }
  const acceptedUnknownEffect = files.standingProof.replace(
    'require(previous.get("conclusion") == "failure", "unknown previous outcome")',
    'pass  # unknown predecessor outcome is accepted',
  );
  if (!standingPredecessorProofErrors(acceptedUnknownEffect).some((error) => error.includes('AR14 exact pre-effect'))) {
    throw new Error('unknown previous effect bypass fixture passed');
  }
  const bypassedDeployFence = files.standingProof.replace(
    'for name in (BEFORE_CREDENTIALS, MUTATION_BOUNDARY, DEPLOY)',
    'for name in ()',
  );
  if (!standingPredecessorProofErrors(bypassedDeployFence).some((error) => error.includes('AR14 exact pre-effect'))) {
    throw new Error('provider deployment boundary bypass fixture passed');
  }
  const bypassedPinSkippedFence = files.standingProof.replace(
    'for name in PIN_SKIPPED',
    'for name in ()',
  );
  if (!standingPredecessorProofErrors(bypassedPinSkippedFence).some((error) => error.includes('AR14 exact pre-effect'))) {
    throw new Error('staging pin mutation/deploy skipped-state bypass fixture passed');
  }
  const bypassedPinSuccessPrefix = files.standingProof.replace(
    '[step["name"] for step in actual_prefix] == list(PIN_SUCCESS_PREFIX)',
    'true',
  );
  if (!standingPredecessorProofErrors(bypassedPinSuccessPrefix).some((error) => error.includes('AR14 exact pre-effect'))) {
    throw new Error('staging pin safe-prefix bypass fixture passed');
  }
  const standingSelfTest = spawnSync('python3', [STANDING_RUN_PROOF, '--self-test'], {
    cwd: ROOT, encoding: 'utf8',
  });
  if (standingSelfTest.status !== 0) {
    throw new Error('AR14 exact pre-effect proof negative tests failed: ' + standingSelfTest.stderr);
  }
  const allowedWorkflowPath = '(.path == ".github/workflows/release-set-promotion.yml@main" or .path == ".github/workflows/release-set-promotion.yml")';
  for (const [pathValue, accepted] of [
    ['.github/workflows/release-set-promotion.yml@main', true],
    ['.github/workflows/release-set-promotion.yml', true],
    ['.github/workflows/release-set-promotion.yml@evil', false],
    ['.github/workflows/other.yml@main', false],
  ]) {
    const verdict = spawnSync('jq', ['-e', allowedWorkflowPath], {
      input: JSON.stringify({ path: pathValue }), encoding: 'utf8',
    });
    if ((verdict.status === 0) !== accepted) throw new Error(`canonical workflow path guard incorrectly accepted/rejected ${pathValue}`);
  }
  const hostileWorkflowPath = files.promotion.replaceAll(allowedWorkflowPath, '(.path == ".github/workflows/release-set-promotion.yml@evil")');
  if (hostileWorkflowPath === files.promotion
      || !promotionErrors(hostileWorkflowPath).some((error) => error.includes('standing READY binder'))
      || !promotionErrors(hostileWorkflowPath).some((error) => error.includes('rollback-negative evidence'))) {
    throw new Error('hostile workflow path delegation fixture unexpectedly passed');
  }
  const targetPinBypass = files.promotion.replace('(.control_plane.account_id // .account_id) == $expected', 'true');
  if (!promotionErrors(targetPinBypass).some((error) => error.includes('standing READY binder'))) throw new Error('staging target-account pin bypass fixture passed');
  const promoteJob = jobBlock(files.promotion, 'promote');
  for (const [label, digestField, evidenceFile] of [
    ['promotion plan', 'promotion_plan_sha256', 'promotion-plan.json'],
    ['immutable release verify', 'release_verify_sha256', 'release-verify.json'],
  ]) {
    const check = `test "$(sha256sum "$ready_root/${evidenceFile}" | cut -d' ' -f1)" = "$(jq -er '.${digestField}' "$ready_root/ready-to-mutate.json")"`;
    if (!promoteJob.includes(check)) throw new Error(`${label} READY sidecar digest fixture target is missing`);
    const forgedDigest = files.promotion.replace(check, 'test true');
    if (!promotionErrors(forgedDigest).some((error) => error.includes('standing READY binder'))) {
      throw new Error(`${label} READY sidecar digest bypass fixture passed`);
    }
  }
  if (!promoteJob.includes('environment: staging')) throw new Error('staging promotion fixture cannot locate its Environment');
  const missingStagingEnvironment = files.promotion.replace(promoteJob, promoteJob.replace('    environment: staging\n', ''));
  if (!promotionErrors(missingStagingEnvironment).some((error) => error.includes('standing READY binder'))) throw new Error('missing staging resolver Environment guard fixture passed');
  const missingArtifactIdentity = files.promotion.replace('.id == $artifact_id and .name == $name and .expired == false', 'true');
  if (!promotionErrors(missingArtifactIdentity).some((error) => error.includes('standing READY binder'))) throw new Error('unbound READY artifact ID/name fixture passed');
  const missingArtifactRun = files.promotion.replace('.workflow_run.id == $run and .workflow_run.head_sha == $sha', 'true');
  if (!promotionErrors(missingArtifactRun).some((error) => error.includes('standing READY binder'))) throw new Error('foreign READY artifact run fixture passed');
  const readyBinder = stepBlock(promoteJob, 'Bind standing staging authority to exact same-run READY evidence');
  const pythonBlock = readyBinder.match(/python - "\$candidate_root\/ready-to-mutate\.json"[^\n]*<<'PY'\n([\s\S]*?)\n          PY/);
  if (!pythonBlock) throw new Error('standing READY intent Python executable is missing');
  const pythonSource = pythonBlock[1].split('\n').map((line) => {
    if (!line.startsWith('          ')) throw new Error('standing READY intent Python indentation is invalid');
    return line.slice(10);
  }).join('\n');
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), 'ar11-ready-binder-'));
  try {
    const mainSha = 'a'.repeat(40);
    const releaseId = 'release-set-v3-sha256-' + 'b'.repeat(64);
    const currentId = 'release-set-v3-sha256-' + 'c'.repeat(64);
    const readyRunId = 98765;
    const readyAttempt = 2;
    const artifactId = 54321;
    const artifactDigest = 'sha256:' + 'f'.repeat(64);
    const readyArtifact = 'ar11-ready-to-mutate-' + releaseId + '-' + mainSha + '-' + readyRunId + '-' + readyAttempt;
    const readyPath = path.join(fixtureRoot, 'ready.json');
    const intentPath = path.join(fixtureRoot, 'intent.json');
    const ready = {
      schema_version: 1, kind: 'AR11_READY_TO_MUTATE', ready: true, environment: 'staging',
      ready_run_id: readyRunId, ready_run_attempt: readyAttempt,
      source_sha: mainSha, release_set_id: releaseId, expected_current_release_set_id: currentId,
      target_capability_profile_id: 'rehearsal-core-v2',
      expected_current_capability_profile_id: 'rehearsal-core-v1',
      credential_values_accessed: false, provider_mutation: false, production_mutation: false,
    };
    const runBinder = (candidate) => {
      writeFileSync(readyPath, JSON.stringify(candidate));
      return spawnSync('python', ['-c', pythonSource, readyPath, mainSha, releaseId,
        String(readyRunId), String(readyAttempt), String(artifactId), readyArtifact, artifactDigest,
        'd'.repeat(64), intentPath], {
        encoding: 'utf8',
        env: { ...process.env, GITHUB_ACTOR: 'owner', GITHUB_RUN_ID: String(readyRunId), GITHUB_RUN_ATTEMPT: String(readyAttempt), GITHUB_REF: 'refs/heads/main' },
      });
    };
    const valid = runBinder(ready);
    if (valid.status !== 0) throw new Error('valid standing READY fixture failed: ' + valid.stderr);
    const intent = JSON.parse(readFileSync(intentPath, 'utf8'));
    if (intent.kind !== 'STAGING_PROMOTION_STANDING_INTENT' || intent.release_set_id !== releaseId
        || intent.source_sha !== mainSha || intent.expected_current_release_set_id !== currentId
        || intent.ready_run_id !== readyRunId || intent.ready_run_attempt !== readyAttempt
        || intent.ready_artifact_name !== readyArtifact
        || intent.ready_evidence_sha256 !== 'd'.repeat(64)
        || intent.authorization.mechanism !== 'github.workflow_run.read_only_ready'
        || intent.authorization.policy !== 'AR11_STANDING_STAGING_READY_V1'
        || intent.authorization.ready_artifact_id !== artifactId
        || intent.authorization.ready_artifact_digest !== artifactDigest
        || intent.target_capability_profile_id !== ready.target_capability_profile_id
        || intent.expected_current_capability_profile_id !== ready.expected_current_capability_profile_id) {
      throw new Error('valid standing READY fixture failed to bind exact immutable evidence');
    }
    const rejected = [
      ['source SHA drift', { ...ready, source_sha: 'e'.repeat(40) }],
      ['target Release Set drift', { ...ready, release_set_id: 'release-set-v3-sha256-' + 'e'.repeat(64) }],
      ['non-ready evidence', { ...ready, ready: false }],
      ['wrong READY run id', { ...ready, ready_run_id: readyRunId + 1 }],
      ['wrong READY attempt', { ...ready, ready_run_attempt: readyAttempt + 1 }],
      ['provider effect before standing authorization', { ...ready, provider_mutation: true }],
      ['untyped current Release Set', { ...ready, expected_current_release_set_id: null }],
      ['untyped capability profile', { ...ready, target_capability_profile_id: null }],
      ['unapproved staging profile', { ...ready, target_capability_profile_id: 'production-core-v2' }],
    ];
    for (const [label, candidate] of rejected) {
      if (runBinder(candidate).status === 0) throw new Error('unsafe ' + label + ' standing READY fixture passed');
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
  if (!promotionErrors(missingFailureCapture).some((error) => error.includes('staging mutation early-failure transport'))) {
    throw new Error('staging early-failure source identity fixture unexpectedly passed');
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
  const currentFenceStep = stepBlock(mutationBlock, 'Re-fence Worker identity and D1 head immediately before mutation');
  for (const [label, original, weakened] of [
    ['full fingerprint comparison', 'if [ "$observed_worker_identity_sha256" = "$expected_worker_identity_sha256" ]; then', 'if [ "$observed_release_set" = "$EXPECTED_CURRENT" ]; then'],
    ['canonical Worker script etag', 'script_etag:$observed[0].current_version.resources.script.etag', 'script_etag:null'],
  ]) {
    if (!currentFenceStep.includes(original)) throw new Error(`${label} exact-current fixture target is missing`);
    const weakFenceStep = currentFenceStep.replace(original, weakened);
    const weakFence = files.promotion.replace(currentFenceStep, weakFenceStep);
    if (!promotionErrors(weakFence).some((error) => error.includes('read-only exact-current fence'))) {
      throw new Error(`${label} mutation-time Worker drift bypass fixture passed`);
    }
  }
  const missingFenceProjection = files.promotion.replace('exact_current_fence:$exact_current_fence', 'exact_current_fence:null');
  if (!promotionErrors(missingFenceProjection).some((error) => error.includes('lossless exact-current fence projection'))) {
    throw new Error('lossy exact-current fence projection fixture unexpectedly passed');
  }
  const secondDeployAfterEvidence = files.promotion.replace(
    '      - name: Terminalize one lossless staging AR11 OperationalOutcome',
    '      - name: Unsafe post-verify retry fixture\n        run: npx --yes wrangler@4.94.0 deploy --config "$RUNNER_TEMP/wrangler.staging.json" --env staging\n\n      - name: Terminalize one lossless staging AR11 OperationalOutcome',
  );
  if (!promotionErrors(secondDeployAfterEvidence).some((error) => error.includes('exactly one real Worker deploy'))) {
    throw new Error('post-verified duplicate Worker deploy fixture unexpectedly passed');
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
  const missingStagingTerminalization = files.promotion.replace('Terminalize one lossless staging AR11 OperationalOutcome', 'Terminalization fixture removed');
  if (!promotionErrors(missingStagingTerminalization).some((error) => error.includes('staging terminal outcome'))) {
    throw new Error('missing staging terminalization fixture unexpectedly passed');
  }
  const clashingTerminalArtifact = files.promotion.replace(
    'name: ar11-staging-operational-outcome-${{ github.run_id }}-${{ github.run_attempt }}',
    'name: ar11-operational-outcome-${{ github.run_id }}-${{ github.run_attempt }}',
  );
  if (clashingTerminalArtifact === files.promotion
      || !promotionErrors(clashingTerminalArtifact).some((error) => error.includes('distinct artifact names'))) {
    throw new Error('duplicate same-run read-only/staging terminal artifact fixture unexpectedly passed');
  }
  const rollbackOldArtifactName = files.promotion.replace(
    'ar11-ready-to-mutate-$RELEASE_SET_ID-${{ steps.bind.outputs.main_sha }}-$SOURCE_RUN_ID-1',
    'ar11-ready-to-mutate-$RELEASE_SET_ID-${{ steps.bind.outputs.main_sha }}',
  );
  if (rollbackOldArtifactName === files.promotion
      || !promotionErrors(rollbackOldArtifactName).some((error) => error.includes('rollback-negative evidence'))) {
    throw new Error('rollback-negative stale READY artifact-name fixture unexpectedly passed');
  }
  const rollbackUnboundReadyAttempt = files.promotion.replace(
    '.source_sha == $sha and .ready_run_id == $run and .ready_run_attempt == 1',
    '.source_sha == $sha and .ready_run_id == $run',
  );
  if (rollbackUnboundReadyAttempt === files.promotion
      || !promotionErrors(rollbackUnboundReadyAttempt).some((error) => error.includes('rollback-negative evidence'))) {
    throw new Error('rollback-negative unbound READY run-attempt fixture unexpectedly passed');
  }
  const rollbackReRun = files.promotion.replace(
    '.event == "workflow_run" and .run_attempt == 1 and .status == "completed"',
    '.event == "workflow_run" and .run_attempt >= 1 and .status == "completed"',
  );
  if (rollbackReRun === files.promotion
      || !promotionErrors(rollbackReRun).some((error) => error.includes('rollback-negative evidence'))) {
    throw new Error('rollback-negative replayed source run fixture unexpectedly passed');
  }
  const weakStagingSuccess = files.promotion.replace('.status == "COMPLETED"', '.status == "RECOVERY_REQUIRED"');
  if (!promotionErrors(weakStagingSuccess).some((error) => error.includes('staging terminal outcome'))) {
    throw new Error('staging success-without-COMPLETED fixture unexpectedly passed');
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
    "echo 'heavy=true' >> \"$GITHUB_OUTPUT\"\n              echo 'No exact verified PR proof: full main Camoufox replay required.'",
    'echo "skipped fallback"',
  );
  if (!camoufoxErrors(missingHeavyFallback, files.camoufoxClassifier).some(error => error.includes('Camoufox scope routing'))) {
    throw new Error('accepted-main full replay fallback bypass fixture passed');
  }
  const missingLightByteProof = files.camoufox.replace(
    'python3.12 scripts/accepted-main-pr-reuse.py --pr-camoufox-byte-proof',
    'echo "filename-only LIGHT was assumed sufficient"',
  );
  if (!camoufoxErrors(missingLightByteProof, files.camoufoxClassifier)
    .some(error => error.includes('Camoufox scope routing'))) {
    throw new Error('PR LIGHT bypass without published-byte physical receipt fixture passed');
  }
  const assertCamoufoxRejected = (candidate, scenario, expectedError) => {
    if (candidate === files.camoufox) throw new Error(`${scenario} fixture setup failed`);
    if (!camoufoxErrors(candidate, files.camoufoxClassifier)
      .some((error) => error.includes(expectedError))) {
      throw new Error(`${scenario} fixture unexpectedly passed`);
    }
  };
  const windowsProof = jobBlock(files.camoufox, 'camoufox-runtime-package-windows');
  const unsafeAutomaticMatches = windowsProof.replaceAll('$assetCandidates', '$matches');
  assertCamoufoxRejected(
    files.camoufox.replace(windowsProof, () => unsafeAutomaticMatches),
    'PowerShell automatic Matches shadows downloaded asset inventory',
    'Camoufox published-runtime proof shadows PowerShell automatic $Matches',
  );
  const protectedRunnerSelection = "runs-on: ${{ needs.camoufox-scope.outputs.heavy == 'true' && 'windows-2025' || 'ubuntu-24.04' }}";
  for (const [kind, replacement] of [
    ['HEAVY wrongly runs on Ubuntu', "runs-on: ${{ needs.camoufox-scope.outputs.heavy == 'true' && 'ubuntu-24.04' || 'ubuntu-24.04' }}"],
    ['LIGHT selection bypasses validated scope', "runs-on: ${{ github.event_name == 'pull_request' && 'ubuntu-24.04' || 'windows-2025' }}"],
  ]) {
    assertCamoufoxRejected(
      files.camoufox.replace(windowsProof, windowsProof.replace(protectedRunnerSelection, replacement)),
      kind,
      'protected Windows runtime build',
    );
  }
  for (const [jobName, name, realCommand] of CAMOUFOX_ACTIVE_PHYSICAL_COMMANDS) {
    const step = stepBlock(windowsProof, name);
    if (!step) throw new Error(`missing ${name} fixture source`);
    assertCamoufoxRejected(
      files.camoufox.replace(step, step.replace(`      - name: ${name}`, `      - name: fake skipped ${name}`)),
      `missing physical ${name}`,
      'physical proof step',
    );
    assertCamoufoxRejected(
      files.camoufox.replace(step, step.replace(realCommand, 'Write-Host "skipped physical command"')),
      `no-op physical ${name}`,
      `Windows physical proof step ${name}`,
    );
    for (const [kind, passive] of [
      ['comment', `# ${realCommand}`],
      ['forged output', `Write-Host 'fake PASS; original command: ${realCommand}'`],
    ]) {
      assertCamoufoxRejected(
        files.camoufox.replace(step, step.replace(realCommand, passive)),
        `${kind} rather than active physical ${name}`,
        'not executing active shell command',
      );
    }
    const bypass = step.replace("if: needs.camoufox-scope.outputs.heavy == 'true'", 'if: false');
    assertCamoufoxRejected(
      files.camoufox.replace(step, bypass),
      `unexpectedly skipped physical ${name} on HEAVY`,
      'can bypass exact HEAVY classification',
    );
  }
  for (const [name, command] of [
    ['Package exact compressed Release Set runtime input', "$archiveSha = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant()"],
    ['Package exact compressed Release Set runtime input', "if ($archiveSha -notmatch '^[0-9a-f]{64}$') { throw 'runtime archive SHA-256 is invalid' }"],
    ['Package exact compressed Release Set runtime input', "Set-Content -LiteralPath (Join-Path $env:RUNNER_TEMP 'runtime-bundle.sha256') -Value $archiveSha -NoNewline -Encoding ascii"],
    ['Extract and verify exact official runtime', "$expectedArchiveSha = (Get-Content -LiteralPath (Join-Path $env:RUNNER_TEMP 'runtime-bundle.sha256') -Raw).Trim()"],
    ['Extract and verify exact official runtime', "$actualArchiveSha = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant()"],
    ['Extract and verify exact official runtime', "if ($actualArchiveSha -cne $expectedArchiveSha) { throw 'runtime archive bytes changed after packaging' }"],
    ['Extract and verify exact official runtime', "if ((Get-FileHash -Algorithm SHA256 $embedded).Hash -ne (Get-FileHash -Algorithm SHA256 $externalManifest).Hash) {"],
  ]) {
    const step = stepBlock(windowsProof, name);
    for (const [kind, replacement] of [
      ['commented', `# ${command}`],
      ['forged', `Write-Host 'forged SHA-256: ${command}'`],
    ]) {
      assertCamoufoxRejected(
        files.camoufox.replace(step, step.replace(command, replacement)),
        `${kind} exact archive identity command in ${name}`,
        'not executing active shell command',
      );
    }
  }
  for (const [jobName, name, protectedMarker] of [
    ['camoufox-runtime-package-windows', 'Prove official Firefox native restored tabs and POST safety on exact runtime', 'AR10_NATIVE_FIREFOX_SESSIONSTORE=PHYSICAL_PASS'],
    ['camoufox-runtime-package-windows', 'Extract and verify exact official runtime', "if (Test-Path -LiteralPath $root) { throw 'shipping extraction must use a clean directory' }"],
    ['camoufox-runtime-package-windows', 'Extract and verify exact official runtime', 'Get-FileHash -Algorithm SHA256'],
    ['camoufox-runtime-package-windows', 'Run physical shipping reader and native Bridge Open on exact packaged runtime', '$env:AR10_PACKAGED_RUNTIME_ARCHIVE = $archive'],
    ['camoufox-runtime-package-windows', 'Run physical shipping reader and native Bridge Open on exact packaged runtime', '$env:AR10_PACKAGED_RUNTIME_MANIFEST = $manifest'],
    ['camoufox-runtime-package-windows', 'Run physical shipping reader and native Bridge Open on exact packaged runtime', 'running 2 tests'],
    ['camoufox-runtime-package-windows', 'Run physical shipping reader and native Bridge Open on exact packaged runtime', '2 passed; 0 failed'],
    ['camoufox-runtime-package-windows', 'Run physical shipping reader and native Bridge Open on exact packaged runtime', 'REAL_SHIPPING_ARCHIVE_READER=PASS;FILES=[0-9]+;COMPRESSED_BYTES=[0-9]+'],
    ['camoufox-runtime-package-windows', 'Run physical shipping reader and native Bridge Open on exact packaged runtime', 'AR10_NATIVE_OPEN_LOCK=PASS;elapsed_ms=[0-9]+'],
  ]) {
    const step = stepBlock(windowsProof, name);
    assertCamoufoxRejected(
      files.camoufox.replace(step, step.replaceAll(protectedMarker, 'Write-Host "unverified"')),
      `missing ${name} integrity marker ${protectedMarker}`,
      `Windows ${jobName} physical proof step ${name}`,
    );
  }
  const missingProtectedJoin = files.camoufox.replace(
    'needs: [camoufox-scope, camoufox-runtime-package-windows]',
    'needs: camoufox-scope',
  );
  assertCamoufoxRejected(missingProtectedJoin, 'missing protected Windows join', 'Camoufox required-proof aggregator');
  const missingScopeDependency = files.camoufox.replace(
    windowsProof, windowsProof.replace('needs: camoufox-scope', 'needs: []'),
  );
  assertCamoufoxRejected(missingScopeDependency, 'Windows removed scope dependency', 'protected Windows runtime build');
  const missingWindowsVerdict = files.camoufox.replace(
    'test "${{ needs.camoufox-runtime-package-windows.result }}" = success',
    'echo "Windows result ignored"',
  );
  assertCamoufoxRejected(missingWindowsVerdict, 'ignored protected Windows verdict', 'Camoufox required-proof aggregator');
  const conditionalWindowsVerdict = files.camoufox.replace(
    '          test "${{ needs.camoufox-runtime-package-windows.result }}" = success',
    '          if [ "${{ needs.camoufox-scope.outputs.heavy }}" = "true" ]; then\n            test "${{ needs.camoufox-runtime-package-windows.result }}" = success\n          fi',
  );
  assertCamoufoxRejected(conditionalWindowsVerdict, 'conditional protected Windows verdict', 'unconditionally require both scope and Windows success');
  const bypassedScopeGuard = windowsProof.replace(
    "if ('${{ needs.camoufox-scope.result }}' -ne 'success') { throw 'Camoufox scope classification failed' }",
    'Write-Host "scope is assumed successful"',
  );
  assertCamoufoxRejected(files.camoufox.replace(windowsProof, bypassedScopeGuard), 'bypassed Windows scope prerequisite', 'Windows scope prerequisite must fail closed');
  const obsoleteJob = files.camoufox.replace(
    '  camoufox-camoufox-required-proof:',
    '  camoufox-bridge-regression-windows:\n    name: restored redundant runtime job\n  camoufox-camoufox-required-proof:',
  );
  assertCamoufoxRejected(obsoleteJob, 'redundant Windows consumer job', 'obsolete extra Windows job');
  const obsoleteReaderJob = files.camoufox.replace(
    '  camoufox-camoufox-required-proof:',
    '  camoufox-shipping-archive-reader-windows:\n    name: restored obsolete reader job\n  camoufox-camoufox-required-proof:',
  );
  assertCamoufoxRejected(obsoleteReaderJob, 'obsolete cross-runner reader job', 'obsolete extra Windows job');
  for (const [name, action] of [
    ['upload', 'actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a'],
    ['download', 'actions/download-artifact@70fc10c6e5e1ce46ad2ea6f2b72d43f7d47b13c3'],
  ]) {
    assertCamoufoxRejected(
      files.camoufox.replace('  camoufox-camoufox-required-proof:',
        `  camoufox-obsolete-${name}-transport:\n    steps:\n      - name: Forbidden runtime ${name}\n        uses: ${action}\n  camoufox-camoufox-required-proof:`),
      `restored runtime ${name} between runners`,
      'exactly four unique',
    );
  }
  const rawArchiveUpload = stepBlock(windowsProof,
    'Upload exact PR-tested runtime archive only on publication miss');
  assertCamoufoxRejected(
    files.camoufox.replace(rawArchiveUpload,
      rawArchiveUpload.replace('archive: false', 'archive: true')),
    'PR runtime archive unexpectedly rewrapped with different bytes',
    'Camoufox raw physical runtime custody',
  );
  assertCamoufoxRejected(
    files.camoufox.replace(rawArchiveUpload,
      rawArchiveUpload.replace("steps.custody.outputs.origin == 'built'",
        "steps.custody.outputs.origin != 'built'")),
    'PR runtime archive uploaded on reusable published-byte hit',
    'Camoufox physical runtime custody upload has unsafe condition',
  );
  const physicalTests = stepBlock(windowsProof,
    'Run physical shipping reader and native Bridge Open on exact packaged runtime');
  assertCamoufoxRejected(
    files.camoufox.replace(rawArchiveUpload, '')
      .replace(physicalTests, rawArchiveUpload + physicalTests),
    'PR raw runtime bytes uploaded before physical proof',
    'Camoufox raw runtime custody steps must follow both completed physical tests',
  );
  const combinedStep = stepBlock(windowsProof, 'Run physical shipping reader and native Bridge Open on exact packaged runtime');
  for (const guard of [
    "if (Test-Path -LiteralPath $root) { throw 'Bridge runtime extraction must use a clean directory' }",
    "if ($actualArchiveSha -cne $expectedArchiveSha) { throw 'physical runtime archive changed after SessionStore proof' }",
    'tar.exe -xzf $archive -C $root',
    'if ($LASTEXITCODE -ne 0) { throw \'clean Bridge runtime extraction failed\' }',
  ]) {
    for (const passive of [`# ${guard}`, `Write-Host 'forged: ${guard}'`]) {
      assertCamoufoxRejected(
        files.camoufox.replace(combinedStep, combinedStep.replace(guard, passive)),
        'clean Bridge extraction guard replaced by passive text',
        'not executing active shell command',
      );
    }
  }
  for (const [scenario, replacement, expectedError] of [
    ['missing physical Cargo deadline', combinedStep.replace('        timeout-minutes: 8\n', ''), '8-minute step deadline'],
    ['unsafe extended physical Cargo deadline', combinedStep.replace('        timeout-minutes: 8', '        timeout-minutes: 90'), '8-minute step deadline'],
  ]) {
    assertCamoufoxRejected(files.camoufox.replace(combinedStep, replacement), scenario, expectedError);
  }
  const longWindowsJob = files.camoufox.replace(windowsProof, windowsProof.replace('    timeout-minutes: 20', '    timeout-minutes: 90'));
  assertCamoufoxRejected(longWindowsJob, 'unbounded Windows job deadline', '20-minute job deadline');
  for (const mutation of ['--test-threads=1', '--skip shipping_windows_runtime_archive_is_readable_by_production_delivery_reader', '--skip bridge_preflight_launches_real_camoufox_and_acquires_native_profile_lock']) {
    const original = '--test-threads=2';
    assertCamoufoxRejected(
      files.camoufox.replace(combinedStep, combinedStep.replace(original, mutation)),
      `parallel physical Rust test changed to ${mutation}`,
      'both real Rust tests concurrently',
    );
  }
  assertCamoufoxRejected(
    files.camoufox.replace(combinedStep, combinedStep.replace('--show-output', '--nocapture')),
    'unbounded interleaved Rust test output',
    'both real Rust tests concurrently',
  );
  assertCamoufoxRejected(
    files.camoufox.replace(combinedStep, combinedStep.replace(
      '2>&1\'', '2>&1 & echo REAL_SHIPPING_ARCHIVE_READER=PASS & echo AR10_NATIVE_OPEN_LOCK=PASS & exit /b 0\'',
    )),
    'appended forged PASS and forced successful exit after cargo',
    'both real Rust tests concurrently',
  );
  const forgedCargo = combinedStep.replace(
    "$testCommand = 'cargo test --locked -p profile-bridge --test real_camoufox_runtime",
    "$testCommand = 'echo forgery; # cargo test --locked -p profile-bridge --test real_camoufox_runtime",
  );
  assertCamoufoxRejected(files.camoufox.replace(combinedStep, forgedCargo), 'forged paired Rust execution', 'both real Rust tests concurrently');
  const disabledPassVerdict = combinedStep.replace(
    "$text -notmatch 'AR10_NATIVE_OPEN_LOCK=PASS;elapsed_ms=[0-9]+'",
    "Write-Host 'AR10_NATIVE_OPEN_LOCK=PASS;elapsed_ms=[0-9]+'",
  );
  assertCamoufoxRejected(files.camoufox.replace(combinedStep, disabledPassVerdict), 'forged native PASS without inspection', 'actively enforce both test results');
  for (const [step, noisyMarker] of [
    [stepBlock(windowsProof, 'Package exact compressed Release Set runtime input'), 'verify-manifest --manifest "$manifest" > $null'],
    [stepBlock(windowsProof, 'Extract and verify exact official runtime'), 'verify-manifest --manifest $externalManifest > $null'],
  ]) {
    assertCamoufoxRejected(
      files.camoufox.replace(step, step.replace(noisyMarker, noisyMarker.replace(' > $null', ''))),
      'restored huge runtime manifest stdout dump',
      'quiet runtime-manifest verification',
    );
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
  const broadFrontendClassifier = files.camoufoxClassifier.replace(
    'path.startswith("frontend/src/") and path.endswith((".ts", ".tsx", ".css"))',
    'path.startswith("frontend/")',
  );
  if (broadFrontendClassifier === files.camoufoxClassifier ||
      !camoufoxClassifierErrors(broadFrontendClassifier).length) {
    throw new Error('unsafe broad frontend Camoufox LIGHT exemption fixture passed');
  }
  const missingUnknownFixture = files.camoufoxClassifier.replace('"unknown-fails-closed"', '"unknown-fixture-removed"');
  if (camoufoxClassifierErrors(missingUnknownFixture).length === 0) {
    throw new Error('Camoufox unknown fail-closed fixture removal passed');
  }
  const weakAuthority = structuredClone(files.authority);
  weakAuthority.promotion_policy.standing_staging_only_after_same_run_read_only_ready = false;
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
  downloader: read(AR11_RUNTIME_DOWNLOADER),
  overlay: read(AR11_RUNTIME_OVERLAY),
  standingProof: read(STANDING_RUN_PROOF),
  authority: JSON.parse(read(AUTHORITY)),
};
const errors = validateAll(files);
if (errors.length > 0) {
  console.error('AR-11 operational authority failed:\n' + errors.map((error) => `- ${error}`).join('\n'));
  process.exit(1);
}
if (process.argv.includes('--self-test')) selfTest(files);
else console.log('AR-11 release operational authority passed: protected-main read-only READY/BLOCKED terminalizes before same-run standing staging promotion; exact run/attempt/asset/READY SHA bind the immutable Release Set to staging only; no manual workflow_dispatch, exact-current Worker/D1 re-fence, protected deploy credential and lossless effect outcome; Camoufox PR replay is fail-closed.');
