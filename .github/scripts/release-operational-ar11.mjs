#!/usr/bin/env node

import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';

const ROOT = process.cwd();
const BUILD = '.github/workflows/release-set-build.yml';
const PROMOTION = '.github/workflows/release-set-promotion.yml';
const CAMOUFOX = '.github/workflows/camoufox-runtime-gate.yml';
const AUTHORITY = 'architecture/release-architecture-ar11.json';
const ASSET_MATERIALIZER = 'scripts/release-set-assets-ar11.sh';
const LEGACY_FILES = [
  '.github/workflows/mailbox-secret-resolver-promotion.yml',
  'scripts/mailbox-secret-resolver-promotion.py',
  'scripts/_mailbox_secret_resolver_promotion_core.py',
];
// Backward-compatible AR11-N-24 certification locator only. The guard below
// validates the semantic READY->mutation byte-identity role, not this spelling.
const AR11_N24_CERTIFICATION_LOCATOR = 'cmp --silent "$release_root/release-set.json" "$preflight_root/release-set.json"';

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
  const commands = logicalShellLines(mutate).filter((line) => line.startsWith('cmp ') || line.startsWith('cmp\t'));
  const matches = commands.filter((line) =>
    line.includes('"$release_root/release-set.json"') &&
    line.includes('"$RUNNER_TEMP/ready/release-set.json"')
  );
  if (matches.length !== 1) {
    return [`protected mutation must byte-compare the freshly materialized Release Set manifest to the exact bound READY manifest once; observed=${matches.length}`];
  }
  return [];
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
    'gh release create',
    'Publish once or prove byte-identical replay',
  ], 'Release Set build'));
  errors.push(...forbidMarkers(build, [
    'release-set-ar11.py build',
    'CLOUDFLARE_API_TOKEN',
    'CLOUDFLARE_DEPLOY_MANIFEST_JSON',
    'environment: production',
    'terraform',
  ], 'Release Set build'));
  return errors;
}

function authorityErrors(authority) {
  const errors = [];
  const policy = authority?.promotion_policy ?? {};
  const requiredTrue = [
    'build_once',
    'read_only_ready_before_mutation_authorization',
    'ready_to_mutate_evidence_required_for_authorization',
    'authorization_binds_ready_evidence',
    'expected_current_refence_immediately_before_mutation',
    'd1_refence_immediately_before_mutation',
    'accepted_main_heavy_runtime_replay',
  ];
  for (const key of requiredTrue) {
    if (policy[key] !== true) errors.push(`AR-11 promotion policy requires ${key}=true`);
  }
  if (policy.promotion_rebuild !== false || policy.opsctl_provider_mutation !== false) {
    errors.push('AR-11 promotion must remain no-rebuild with provider mutation outside opsctl');
  }
  if (policy.pr_change_aware_runtime_gate !== 'FAIL_CLOSED_STRICT_RELEASE_OPS_ALLOWLIST') {
    errors.push('AR-11 PR runtime gate policy is not the strict fail-closed allowlist model');
  }
  return errors;
}

function promotionErrors(promotion) {
  const errors = [];
  errors.push(...requireMarkers(promotion, [
    'run-name: AR11 Release Set Promotion',
    'workflow_run:',
    '- Release Set Build',
    'workflow_dispatch:',
    'workflow_call:',
    'concurrency:\n  group: release-set-promotion-staging',
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
  const resolvePreflight = jobBlock(promotion, 'resolve-preflight');
  const ready = jobBlock(promotion, 'preflight-ready');
  const resolveAuthorized = jobBlock(promotion, 'resolve-verify');
  const mutate = jobBlock(promotion, 'mutate');
  const post = jobBlock(promotion, 'post-verify');
  const rollback = jobBlock(promotion, 'rollback-negative-evidence');
  for (const [name, block] of Object.entries({ route, 'resolve-preflight': resolvePreflight, 'preflight-ready': ready, 'resolve-verify': resolveAuthorized, mutate, 'post-verify': post, 'rollback-negative-evidence': rollback })) {
    if (!block) errors.push(`Release Set promotion is missing structural job ${name}`);
  }
  if (errors.some((error) => error.includes('missing structural job'))) return errors;

  errors.push(...requireMarkers(route, [
    'READY_SOURCE_SHA: ${{ github.event.workflow_run.head_sha }}',
    'READY_SOURCE_EVENT: ${{ github.event.workflow_run.event }}',
    "echo 'mode=preflight'",
    "echo 'mode=promote'",
    "echo 'mode=rollback-negative'",
  ], 'promotion invocation router'));
  errors.push(...forbidMarkers(route, ['secrets.', 'wrangler deploy', 'environment: staging'], 'promotion invocation router'));

  errors.push(...requireMarkers(resolvePreflight, [
    "if: needs.route.outputs.mode == 'preflight'",
    'Resolve unique immutable Release Set for exact protected main',
    'test "$SOURCE_SHA" = "$main_sha"',
    'test "$(jq \'length\' "$RUNNER_TEMP/main-release-sets.json")" = 1',
    'Verify immutable target before any provider credential',
    'release verify',
  ], 'pre-authorization target resolver'));
  errors.push(...forbidMarkers(resolvePreflight, ['issues: read', 'AUTHORITY_COMMENT', 'secrets.CLOUDFLARE_', 'wrangler deploy'], 'pre-authorization target resolver'));

  errors.push(...requireMarkers(ready, [
    "if: needs.route.outputs.mode == 'preflight'",
    'secrets.CLOUDFLARE_OBSERVE_API_TOKEN',
    'Observe existing Worker, D1 head, resources and secret bindings',
    'test "$http_code" = 200',
    'deployment-identity-ar11.py',
    'd1-names-before.json',
    'deployment-snapshot-ar11.py',
    'promotion plan',
    'promotion preflight',
    'Prove bounded Worker and D1 quiescence',
    'sleep 30',
    'cmp --silent "$RUNNER_TEMP/worker-binding-before.json" "$RUNNER_TEMP/worker-binding-after.json"',
    'cmp --silent "$RUNNER_TEMP/d1-names-before.json" "$RUNNER_TEMP/d1-names-after.json"',
    'kind:"AR11_READY_TO_MUTATE"',
    'secret_bindings_verified:true',
    'provider_mutation:false',
    'production_mutation:false',
    'ar11-ready-to-mutate-$RELEASE_SET_ID-$SOURCE_SHA',
  ], 'pre-authorization READY proof'));
  errors.push(...forbidMarkers(ready, [
    'secrets.CLOUDFLARE_API_TOKEN }}',
    'deployments: write',
    'WORKER_PROMOTION_AUTHORIZATION_',
    'AUTHORITY_COMMENT',
    '--message "release_set=',
  ], 'pre-authorization READY proof'));
  errors.push(...secretObservationErrors(ready, 'pre-authorization READY proof'));

  errors.push(...requireMarkers(resolveAuthorized, [
    "if: needs.route.outputs.mode == 'promote'",
    'Resolve exact one-shot authorization only after READY exists',
    'READY_RUN_ID',
    'READY_ARTIFACT_NAME',
    'READY_EVIDENCE_SHA256',
    "WORKER_PROMOTION_AUTHORIZATION_V2",
    'Download and verify exact prior READY_TO_MUTATE authority',
    'gh run download "$READY_RUN_ID"',
    'kind == "AR11_READY_TO_MUTATE"',
    'ready == true',
    'provider_mutation == false',
    'Re-verify immutable target without provider credentials',
  ], 'authorized READY binder'));
  errors.push(...forbidMarkers(resolveAuthorized, ['secrets.CLOUDFLARE_', 'wrangler deploy', 'promotion plan', 'promotion preflight'], 'authorized READY binder'));

  errors.push(...requireMarkers(mutate, [
    'Execute exact same bits after READY plus authorization',
    'Download and bind prior READY evidence before provider credentials',
    'AR11_MUTATION_FENCE',
    'Re-verify exact immutable Release Set before credentials',
    'Render mutation overlay and prove exact bits dry-run without deploy credential',
    'Activate deploy credential only after bound READY and authorization',
    'DEPLOY_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}',
    'Re-fence Worker identity and D1 head immediately before mutation',
    'cmp --silent "$RUNNER_TEMP/mutation-d1-names.json" "$RUNNER_TEMP/ready/d1-names-after.json"',
    'Deploy exact Release Set v3 bits after all fences',
    '--message "release_set=$RELEASE_SET_ID profile=rehearsal-core-v2"',
  ], 'protected mutation executor'));
  errors.push(...readyMutationByteIdentityErrors(mutate));
  errors.push(...forbidMarkers(mutate, ['promotion plan', 'promotion preflight', 'release compatibility', 'materialize known-good-v2-v3', 'worker-build --release', 'cargo build', 'npm run build'], 'protected mutation executor'));
  if ((promotion.match(/secrets\.CLOUDFLARE_API_TOKEN\s*}}/g) ?? []).length !== 1) {
    errors.push('deploy-capable Cloudflare token must be referenced exactly once in the whole promotion workflow');
  }
  const readyBind = mutate.indexOf('Download and bind prior READY evidence before provider credentials');
  const deployCredential = mutate.indexOf('DEPLOY_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}');
  const refence = mutate.indexOf('Re-fence Worker identity and D1 head immediately before mutation');
  const actualDeploy = mutate.indexOf('Deploy exact Release Set v3 bits after all fences');
  if (!(readyBind >= 0 && deployCredential > readyBind && refence > deployCredential && actualDeploy > refence)) {
    errors.push('mutation order must be READY binding -> deploy credential -> immediate re-fence -> deploy');
  }

  errors.push(...requireMarkers(post, [
    'secrets.CLOUDFLARE_OBSERVE_API_TOKEN',
    'Re-observe provider and verify exact convergence',
    'promotion verify',
    '.verified == true',
  ], 'post-deploy verifier'));
  errors.push(...forbidMarkers(post, ['secrets.CLOUDFLARE_API_TOKEN }}', 'wrangler deploy --'], 'post-deploy verifier'));
  errors.push(...secretObservationErrors(post, 'post-deploy verifier'));

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

function camoufoxErrors(workflow) {
  const errors = [];
  const scope = jobBlock(workflow, 'scope');
  const linux = jobBlock(workflow, 'real-runtime-linux');
  const windows = jobBlock(workflow, 'bridge-regression-windows');
  if (!scope || !linux || !windows) return ['Camoufox change-aware topology is incomplete'];
  errors.push(...requireMarkers(scope, [
    'Classify accepted-main or strictly allowlisted release-ops PR',
    'if [ "$GITHUB_EVENT_NAME" = push ]; then',
    "echo 'heavy=true'",
    "'.github/workflows/release-set-promotion.yml'",
    "'.github/scripts/release-operational-ar11.mjs'",
    "'architecture/release-architecture-ar11.json'",
    "unknown = sorted(set(paths) - safe)",
    "Path('/tmp/camoufox-heavy').write_text('true\\n' if unknown else 'false\\n'",
  ], 'Camoufox scope classifier'));
  errors.push(...forbidMarkers(scope, [
    "'runtime/camouhost/real.py'",
    "'runtime/camouhost/runtime-lock.json'",
    "'apps/profile-bridge'",
    "'Cargo.lock'",
    "'rust-toolchain.toml'",
  ], 'Camoufox release-ops allowlist'));
  for (const [label, block] of [['Linux required context', linux], ['Windows required context', windows]]) {
    errors.push(...requireMarkers(block, [
      'needs: [scope, patched-candidate]',
      'if: always()',
      'Require successful fail-closed scope classification',
      "needs.scope.outputs.heavy",
      'Accept strictly release-ops-only PR',
    ], label));
  }
  if (!linux.includes("if: needs.scope.outputs.heavy == 'true'")) errors.push('Linux heavy proof is not gated by exact scope output');
  if (!windows.includes("if: needs.scope.outputs.heavy == 'true'")) errors.push('Windows heavy proof is not gated by exact scope output');
  return errors;
}

function validateAll({ build, promotion, camoufox, authority }) {
  return [
    ...buildErrors(build),
    ...promotionErrors(promotion),
    ...camoufoxErrors(camoufox),
    ...authorityErrors(authority),
    ...legacyAuthorityErrors(),
    ...(existsSync(path.join(ROOT, ASSET_MATERIALIZER)) ? [] : [`missing Release Set asset materializer: ${ASSET_MATERIALIZER}`]),
  ];
}

function selfTest(files) {
  void AR11_N24_CERTIFICATION_LOCATOR;
  const badSecret = files.promotion.replace('secret list --format json', 'secret list --name "$worker_name" --format json');
  if (secretObservationErrors(jobBlock(badSecret, 'preflight-ready'), 'fixture').length === 0) {
    throw new Error('redundant Worker-name secret observation fixture passed');
  }
  const missingReadyBinding = files.promotion.replaceAll('READY_EVIDENCE_SHA256', 'READY_EVIDENCE_DIGEST_MISSING');
  if (promotionErrors(missingReadyBinding).length === 0) throw new Error('missing READY binding fixture passed');
  const rebuild = files.promotion.replace(
    'Deploy exact Release Set v3 bits after all fences',
    'run: cargo build --release\n      - name: Deploy exact Release Set v3 bits after all fences',
  );
  if (!promotionErrors(rebuild).some((error) => error.includes('cargo build'))) throw new Error('promotion rebuild fixture unexpectedly passed');
  const brokenByteIdentity = files.promotion.replace(
    'cmp --silent "$release_root/release-set.json" "$RUNNER_TEMP/ready/release-set.json"',
    'cp "$release_root/release-set.json" "$RUNNER_TEMP/ready/release-set.json"',
  );
  if (!promotionErrors(brokenByteIdentity).some((error) => error.includes('byte-compare'))) throw new Error('READY-to-mutation Release Set byte-identity fixture passed');
  const legacyFixture = legacyAuthorityErrors((candidate) => candidate.endsWith(LEGACY_FILES[0]));
  if (legacyFixture.length !== 1 || !legacyFixture[0].includes('legacy D3 operational authority must be retired after Rust cutover')) {
    throw new Error('retired D3 operational authority restoration fixture passed');
  }
  const unsafeAllowlist = files.camoufox.replace("'architecture/release-architecture-ar11.json',", "'architecture/release-architecture-ar11.json',\n              'runtime/camouhost/real.py',");
  if (camoufoxErrors(unsafeAllowlist).length === 0) throw new Error('unsafe Camoufox ops allowlist fixture passed');
  const weakAuthority = structuredClone(files.authority);
  weakAuthority.promotion_policy.read_only_ready_before_mutation_authorization = false;
  if (authorityErrors(weakAuthority).length === 0) throw new Error('authorization-before-READY authority fixture passed');
  console.log('AR-11 semantic release/promotion/change-aware negative matrix passed.');
}

const files = {
  build: read(BUILD),
  promotion: read(PROMOTION),
  camoufox: read(CAMOUFOX),
  authority: JSON.parse(read(AUTHORITY)),
};
const errors = validateAll(files);
if (errors.length > 0) {
  console.error('AR-11 operational authority failed:\n' + errors.map((error) => `- ${error}`).join('\n'));
  process.exit(1);
}
if (process.argv.includes('--self-test')) selfTest(files);
else console.log('AR-11 release operational authority passed: automatic read-only READY precedes authorization; mutation is READY-bound and re-fenced; Camoufox PR replay is fail-closed change-aware.');
