#!/usr/bin/env node

import { execFileSync } from 'node:child_process';
import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const EXECUTOR = '.github/workflows/d1-migration-executor.yml';
const V2_ROUTER = '.github/workflows/v2-phase-a-d1-command-router.yml';
const LEGACY_V2_ONE_CLICK = '.github/workflows/v2-d1-one-click-dispatcher.yml';
const ADAPTER = 'scripts/d1-executor-plan.py';
const EXECUTOR_ADMISSION = 'tools/opsctl/src/d1/executor_admission.rs';
const EXECUTION_CONTROL = 'tools/opsctl/src/d1/execution_control.rs';
const EXECUTION_CONTROL_EXAMPLE = 'tools/opsctl/examples/d1-execution-control.rs';
const DIAGNOSTICS_HELPER = 'scripts/d1-gate-diagnostics.py';
const CONTRACT_TRANSITION = 'tools/opsctl/src/d1/contract_transition.rs';
const CONTRACT_EXAMPLE = 'tools/opsctl/examples/d1-contract-transition.rs';
const PROMOTION = '.github/workflows/release-set-promotion.yml';
const WORKFLOWS = '.github/workflows';
const PINNED_WRANGLER = 'wrangler@4.94.0';
const SHARED_MUTATION_GROUP = 'release-set-promotion-staging';
const OBSERVE_REF = 'CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_OBSERVE_API_TOKEN }}';
const DEPLOY_REF = 'CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}';
const ORDINARY_CONFIRMATION = 'test "$CONFIRMATION" = "$SOURCE_SHA:$TARGET_ENVIRONMENT:$COMPONENT:$DATABASE_ID"';
const CONTRACT_CONFIRMATION = 'test "$CONFIRMATION" = "$SOURCE_SHA:$TARGET_ENVIRONMENT:$COMPONENT:$DATABASE_ID:contract:$EXPECTED_RELEASE_SET_ID"';
const POST_CONTRACT_LEDGER = '--post-ledger-json artifacts/d1-migration/ledger-after.json';
const DIAGNOSTICS_PATH = 'artifacts/d1-migration/d1-gate-diagnostics.jsonl';
const SEALED_PLAN_EXTRACTION = "jq '.execution_plan' artifacts/d1-migration/executor-admission.json > artifacts/d1-migration/plan.json";
const AUTH_DIGEST_MARKER = "hashlib.sha256(raw.encode('utf-8')).hexdigest()";
const AUTH_PROVENANCE_STEP = 'Bind ordinary authorization to immutable GitHub provenance';
const REPLAY_PROOF_STEP = 'Prove ordinary transaction authorization has not been consumed';
const CONSUMPTION_STEP = 'Consume ordinary transaction authorization';
const REPLAY_RUN_MARKER = '.run_number < $current_number';
const REPLAY_SUCCESS_MARKER = '.name == $step and .conclusion == "success"';
const CONSUMPTION_ARTIFACT = 'artifacts/d1-migration/authorization-consumption.json';
const TARGET_FENCE_CREATE_STEP = 'Create typed target fence lease';
const TARGET_FENCE_MARKER_STEP = 'Acquire durable target fence';
const TARGET_FENCE_VERIFY_STEP = 'Verify typed target fence immediately before provider mutation';
const TARGET_FENCE_LEASE = 'artifacts/d1-migration/target-fence-lease.json';
const TARGET_FENCE_OBSERVATION = 'artifacts/d1-migration/target-fence-observation.json';
const TARGET_FENCE_VERIFICATION = 'artifacts/d1-migration/target-fence-verification.json';
const TARGET_FENCE_NEWER_RUN_MARKER = '.run_number > $current_number';
const TARGET_FENCE_SUCCESS_MARKER = '.name == $step and .conclusion == "success"';
const RECEIPT = 'artifacts/d1-migration/execution-receipt.json';
const RECEIPT_INIT_STEP = 'Initialize execution receipt after durable target fence';
const RECEIPT_INITIAL_MARKER_STEP = 'Persist initial execution receipt snapshot';
const RECEIPT_PREWRITE_STEP = 'Append PREWRITE_FENCE_PASS to execution receipt';
const PREAPPLY_OBSERVE_STEP = 'Revalidate exact ledger fence with observe credential';
const RECEIPT_MUTATION_STARTED_STEP = 'Append MUTATION_STARTED to execution receipt';
const RECEIPT_MUTATION_MARKER_STEP = 'Persist MUTATION_STARTED execution receipt snapshot';
const APPLY_STEP = 'Apply planned migrations with deploy credential';
const RECEIPT_APPLIED_STEP = 'Append mechanically known MIGRATION_APPLIED events';
const RECEIPT_APPLIED_MARKER_STEP = 'Persist applied migration execution receipt snapshot';
const REREAD_STEP = 'Reread remote ledger with observe credential';
const RECEIPT_POST_OBSERVED_STEP = 'Append POST_OBSERVED to execution receipt';
const RECEIPT_COMPLETE_STEP = 'Complete execution receipt after verified post-state';
const RECEIPT_TERMINALIZE_STEP = 'Terminalize execution receipt fail closed';
const RECEIPT_TERMINAL_MARKER_STEP = 'Persist terminal execution receipt snapshot';
const DIAGNOSTIC_REASON_CODES = [
  'D1_NATIVE_PLAN_COMMAND_FAILED',
  'D1_COMPATIBILITY_COMMAND_FAILED',
  'D1_NATIVE_PLAN_OUTPUT_INVALID',
  'D1_NATIVE_PLAN_DENIED',
  'D1_COMPATIBILITY_OUTPUT_INVALID',
  'D1_COMPATIBILITY_DENIED',
  'D1_ROLLBACK_POLICY_BLOCKED',
  'D1_CONTRACT_PLAN_MIGRATION_MISMATCH',
  'D1_CONTRACT_RECOVERY_STRATEGY_MISMATCH',
];

function fail(message) {
  throw new Error(message);
}

function normalizedShell(text) {
  return text.replace(/\\\s*\n\s*/g, ' ');
}

function occurrenceCount(text, marker) {
  return text.split(marker).length - 1;
}

function replaceFixture(label, text, from, to) {
  const mutated = text.replace(from, to);
  if (mutated === text) fail(`negative protected-executor fixture did not mutate source: ${label}`);
  return mutated;
}

function stepBody(text, stepName) {
  const start = text.indexOf(`\n      - name: ${stepName}`);
  if (start < 0) fail(`protected D1 executor step is missing: ${stepName}`);
  const end = text.indexOf('\n      - name:', start + 1);
  return text.slice(start, end < 0 ? text.length : end);
}

function runDiagnosticsHelperSelfTest() {
  const env = { ...process.env };
  delete env.CLOUDFLARE_API_TOKEN;
  delete env.CLOUDFLARE_ACCOUNT_ID;
  const python = process.platform === 'win32' ? 'python' : 'python3';
  execFileSync(python, [path.join(ROOT, DIAGNOSTICS_HELPER), 'self-test'], {
    cwd: ROOT,
    env,
    stdio: 'inherit',
  });
}

async function workflowPaths(root) {
  const entries = await readdir(path.join(root, WORKFLOWS), { withFileTypes: true });
  return entries
    .filter((entry) => entry.isFile() && /\.ya?ml$/i.test(entry.name))
    .map((entry) => `${WORKFLOWS}/${entry.name}`)
    .sort();
}

function validateSharedMutationGroup(executorText, promotionText) {
  const marker = `group: ${SHARED_MUTATION_GROUP}`;
  if (!executorText.includes(marker) || !promotionText.includes(marker)) {
    fail('Release Set promotion and protected D1 executor must share the exact staging mutation mutex');
  }
  if (!executorText.includes('cancel-in-progress: false') || !promotionText.includes('cancel-in-progress: false')) {
    fail('shared staging provider mutation authority must serialize without cancellation');
  }
}

async function validateExecutor(text, root = ROOT) {
  const promotionText = await readFile(path.join(root, PROMOTION), 'utf8');
  const routerText = await readFile(path.join(root, V2_ROUTER), 'utf8');
  const adapterText = await readFile(path.join(root, ADAPTER), 'utf8');
  const admissionText = await readFile(path.join(root, EXECUTOR_ADMISSION), 'utf8');
  const compactAdmissionText = admissionText.replace(/\s+/g, '');
  const executionControlText = await readFile(path.join(root, EXECUTION_CONTROL), 'utf8');
  const executionControlExampleText = await readFile(path.join(root, EXECUTION_CONTROL_EXAMPLE), 'utf8');
  const diagnosticsText = await readFile(path.join(root, DIAGNOSTICS_HELPER), 'utf8');
  const contractText = await readFile(path.join(root, CONTRACT_TRANSITION), 'utf8');
  const contractExampleText = await readFile(path.join(root, CONTRACT_EXAMPLE), 'utf8');
  const workflows = await workflowPaths(root);
  validateSharedMutationGroup(text, promotionText);

  const requiredMarkers = [
    'workflow_dispatch:', 'run-name: D1 authorization=', 'issues: read', 'environment: staging',
    `group: ${SHARED_MUTATION_GROUP}`, 'cancel-in-progress: false', 'authorize:', 'needs: authorize',
    'authorization_digest:', 'AUTHORIZATION_DIGEST: ${{ inputs.authorization_digest }}',
    'test "$TARGET_ENVIRONMENT" = "staging"', 'test "$GITHUB_REF" = "refs/heads/main"',
    'test "$GITHUB_SHA" = "$SOURCE_SHA"', 'test "$MUTATION_AUTHORIZED" = "true"',
    'test "$GITHUB_EVENT_NAME" = "workflow_dispatch"', 'test "$GITHUB_RUN_ATTEMPT" = "1"',
    '[[ "$AUTHORIZATION_DIGEST" =~ ^[0-9a-f]{64}$ ]]', AUTH_DIGEST_MARKER,
    'D1_TRANSACTION_AUTHORIZATION_V1', 'authorization_reference must identify an issue comment in this repository',
    "comment.get('author_association') != 'OWNER'", "updated_at != created_at",
    'authorization comment must be durably recorded inside its declared validity window',
    'display_title', 'GITHUB_RUN_NUMBER', AUTH_PROVENANCE_STEP, REPLAY_PROOF_STEP, CONSUMPTION_STEP,
    REPLAY_RUN_MARKER, REPLAY_SUCCESS_MARKER,
    'actions/workflows/d1-migration-executor.yml/runs?event=workflow_dispatch&per_page=100',
    'actions/runs/$run_id/jobs?per_page=100', 'test "$consumed" -eq 0',
    CONSUMPTION_ARTIFACT, 'AUTHORIZATION_CONSUMPTION_READY',
    '.authorization_digest == $authorization_digest',
    ORDINARY_CONFIRMATION, CONTRACT_CONFIRMATION,
    'transition_mode:', 'expected_release_set_id:', "'migrations_dir': 'migrations-bounded'", 'd1 repository',
    'd1 info', 'SELECT id, name FROM d1_migrations ORDER BY id', 'd1 status', 'd1 plan',
    '--example d1-contract-transition', 'FAIL_FORWARD_ONLY', 'd1 compatibility',
    'executor-admission.json', '.execution_plan', 'planned_migration_digests', SEALED_PLAN_EXTRACTION,
    'd1-executor-plan.py materialize', '--require-plan-digests', 'expected-pending.json', 'ledger-before-names.json',
    'd1 migrations list', 'd1-executor-plan.py verify-pending', 'd1 time-travel info',
    TARGET_FENCE_CREATE_STEP, TARGET_FENCE_MARKER_STEP, TARGET_FENCE_VERIFY_STEP,
    '--example d1-execution-control', 'd1-execution-control acquire-fence', 'd1-execution-control verify-fence',
    'd1-execution-control initialize-receipt', 'd1-execution-control append-receipt',
    "'fence_epoch': int(os.environ['GITHUB_RUN_NUMBER'])", "'executor_run_id': int(os.environ['GITHUB_RUN_ID'])",
    "'run_attempt': int(os.environ['GITHUB_RUN_ATTEMPT'])", TARGET_FENCE_LEASE,
    'd1-target-fence-${{ github.run_number }}-${{ github.run_id }}-${{ github.run_attempt }}',
    TARGET_FENCE_NEWER_RUN_MARKER, 'actions/runs/$run_id/jobs?filter=all&per_page=100',
    'actions/runs/$run_id/artifacts?per_page=100', TARGET_FENCE_SUCCESS_MARKER,
    "'history_complete': True", "'current_marker_succeeded': True", TARGET_FENCE_OBSERVATION,
    TARGET_FENCE_VERIFICATION, 'TARGET_FENCE_ACQUIRED', 'TARGET_FENCE_VERIFIED',
    RECEIPT_INIT_STEP, RECEIPT_INITIAL_MARKER_STEP, RECEIPT_PREWRITE_STEP, PREAPPLY_OBSERVE_STEP,
    RECEIPT_MUTATION_STARTED_STEP, RECEIPT_MUTATION_MARKER_STEP, APPLY_STEP, RECEIPT_APPLIED_STEP,
    RECEIPT_APPLIED_MARKER_STEP, REREAD_STEP, RECEIPT_POST_OBSERVED_STEP, RECEIPT_COMPLETE_STEP,
    RECEIPT_TERMINALIZE_STEP, RECEIPT_TERMINAL_MARKER_STEP, RECEIPT,
    'PREWRITE_FENCE_PASS', 'PREWRITE_ABORTED', 'MUTATION_STARTED', 'MIGRATION_APPLIED',
    'POST_OBSERVED', 'VERIFIED', 'COMPLETED', 'RECOVERY_REQUIRED', 'FAILED_NO_EFFECT',
    'd1-execution-receipt-${{ github.run_number }}-${{ github.run_id }}-${{ github.run_attempt }}-initial',
    'd1-execution-receipt-${{ github.run_number }}-${{ github.run_id }}-${{ github.run_attempt }}-mutation-started',
    'd1-execution-receipt-${{ github.run_number }}-${{ github.run_id }}-${{ github.run_attempt }}-applied',
    'd1-execution-receipt-${{ github.run_number }}-${{ github.run_id }}-${{ github.run_attempt }}-terminal',
    "hashFiles('artifacts/d1-migration/execution-receipt.json')", "'execution_receipt': receipt",
    "'execution_receipt_id': receipt['receipt_id']",
    'ledger-fence.json', 'status-fence.json',
    'cmp --silent artifacts/d1-migration/status-before.json artifacts/d1-migration/status-fence.json',
    'ledger-fence-names.json',
    'cmp --silent artifacts/d1-migration/ledger-before-names.json artifacts/d1-migration/ledger-fence-names.json',
    'wrangler-pending-fence.txt', 'd1 migrations apply', 'expected-after.json', 'ledger-after-names.json',
    'd1 verify', POST_CONTRACT_LEDGER, 'd1 contract-transition verify', 'predecessor_ledger_state',
    'runtime_target_revision', 'transition_migrations',
    'PRAGMA foreign_key_check', 'PRAGMA integrity_check',
    "'target_fence': load('target-fence-lease.json')", "'target_fence_verification': load('target-fence-verification.json')",
    "provider_mutation_executed': bool(plan.get('planned_migrations'))", "automatic_restore_executed': False",
    "secret_material_recorded': False", 'actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a',
    '--experimental-provision=false', '--experimental-auto-create=false',
    'env -u CLOUDFLARE_API_TOKEN -u CLOUDFLARE_ACCOUNT_ID cargo run', OBSERVE_REF, DEPLOY_REF,
    'python scripts/d1-gate-diagnostics.py self-test', 'python scripts/d1-gate-diagnostics.py record',
    'python scripts/d1-gate-diagnostics.py evaluate', DIAGNOSTICS_PATH,
    'Upload metadata-only D1 gate diagnostics on policy failure',
    "if: failure() && steps.policy.outcome == 'failure'",
    'D1_COMPATIBILITY_COMMAND_FAILED',
  ];
  for (const marker of requiredMarkers) {
    if (!text.includes(marker)) fail(`protected D1 executor lost required contract marker: ${marker}`);
  }
  for (const forbidden of [
    'workflow_call:', 'issues: write',
    'Bind ordinary authorization to immutable GitHub provenance and consume one-shot dispatch identity',
    'test "$prior" -eq 0',
    "'migrations_dir': '../../migrations/d1'", '"migrations_dir": "../../migrations/d1"',
    'migrations_dir = ../../migrations/d1', 'd1 time-travel restore', 'time-travel restore', 'd1 create',
    'database create', 'experimental-provision=true', 'experimental-auto-create=true', 'cancel-in-progress: true',
    '          - production', 'environment: ${{ inputs.environment }}',
  ]) {
    if (text.includes(forbidden)) fail(`protected D1 executor contains forbidden marker: ${forbidden}`);
  }
  if (occurrenceCount(text, 'test "$GITHUB_RUN_ATTEMPT" = "1"') !== 2) {
    fail('ordinary one-shot authorization must reject reruns in exactly the pre-Environment and mutation jobs');
  }
  if (occurrenceCount(text, '[[ "$AUTHORIZATION_DIGEST" =~ ^[0-9a-f]{64}$ ]]') !== 2) {
    fail('authorization digest format must be enforced in exactly the pre-Environment and mutation jobs');
  }
  if (occurrenceCount(text, ORDINARY_CONFIRMATION) !== 2) {
    fail(`ordinary mutation confirmation must be enforced exactly twice; observed=${occurrenceCount(text, ORDINARY_CONFIRMATION)}`);
  }
  if (occurrenceCount(text, CONTRACT_CONFIRMATION) !== 2) {
    fail(`contract mutation confirmation must be enforced exactly twice; observed=${occurrenceCount(text, CONTRACT_CONFIRMATION)}`);
  }
  if (workflows.includes(LEGACY_V2_ONE_CLICK)) {
    fail('superseded V2 one-click executor caller must remain retired');
  }

  for (const marker of [
    AUTH_DIGEST_MARKER, 'AUTHORIZATION_DIGEST', '--arg authorization_digest "$AUTHORIZATION_DIGEST"',
    'authorization_digest:$authorization_digest', 'TRANSACTION_AUTHORIZATION_JSON',
  ]) {
    if (!routerText.includes(marker)) fail(`V2 router lost exact typed authorization transport marker: ${marker}`);
  }
  for (const forbidden of [
    'select(.head_sha == $sha)', 'conservatively prove source has not been dispatched',
  ]) {
    if (routerText.includes(forbidden)) fail(`V2 router revived source-wide replay fence: ${forbidden}`);
  }

  for (const reasonCode of DIAGNOSTIC_REASON_CODES) {
    if (!diagnosticsText.includes(reasonCode)) fail(`D1 diagnostic helper lost stable reason code: ${reasonCode}`);
  }
  for (const marker of [
    'schema_version', 'reason_code', 'exit_code', 'allowed', 'detail',
    '::error title=', 'GITHUB_STEP_SUMMARY', 'self-test',
  ]) {
    if (!diagnosticsText.includes(marker)) fail(`D1 diagnostic helper lost metadata-only contract marker: ${marker}`);
  }

  for (const marker of [
    'native D1 plan', 'typed repository projection', 'remote ledger is not an exact prefix',
    'ordinary d1 plan must never authorize the separate fail-forward CONTRACT',
    'contract-transition must authorize exactly the sole Catalog 0032 CONTRACT',
    'Wrangler pending list differs from native planned_migrations',
    'authorized ordinary execution plan must contain planned_migration_digests',
    'planned_migration_digests cardinality must exactly match planned_migrations',
    'materialized migration content digest differs from authorized execution plan',
    'authorized ordinary execution plan must contain sealed predecessor state',
    'fresh remote ledger differs from the sealed authorized predecessor',
    'self-test accepted fresh ledger drift for a sealed no-op transaction',
  ]) {
    if (!adapterText.includes(marker)) fail(`exact-plan adapter lost fail-closed marker: ${marker}`);
  }
  for (const marker of [
    'pubpredecessor_ledger_sha256:String',
    'pubpredecessor_migrations:Vec<String>',
    'predecessor_ledger_sha256:transaction.transaction_plan.predecessor_ledger_sha256.clone()',
    'predecessor_migrations:transaction.provider_observation.remote_migrations.clone()',
    'pubauthorization_digest:String',
    'authorization_digest:authorization.authorization_digest',
  ]) {
    if (!compactAdmissionText.includes(marker)) fail(`typed executor admission lost sealed transaction/authorization projection: ${marker}`);
  }
  for (const marker of [
    'pub fn acquire_target_fence', 'pub fn verify_target_fence',
    'pub fn initialize_execution_receipt', 'pub fn append_execution_event',
    'target fence history is incomplete or unknown; abort before provider mutation',
    'stale executor fence rejected', 'split-brain target fence rejected',
    'same_target(&observed.target, &lease.target)', 'input.run_attempt != 1',
    '(Authorized, PrewriteFencePass | PrewriteAborted)',
    '(PrewriteAborted, FailedNoEffect)',
    '(PrewriteFencePass, MutationStarted | PostObserved | FailedNoEffect)',
    '(MutationStarted, MigrationApplied | PostObserved | RecoveryRequired)',
    '(PostObserved, Verified | RecoveryRequired)', '(Verified, Completed)',
    'only MIGRATION_APPLIED may carry migration_id',
  ]) {
    if (!executionControlText.includes(marker)) fail(`typed execution-control owner lost fail-closed marker: ${marker}`);
  }
  for (const marker of [
    '"acquire-fence"', '"verify-fence"', '"initialize-receipt"', '"append-receipt"',
    'TargetFenceObservation', 'TargetFenceLease', 'ExecutionReceiptSeed', 'ExecutionEventInput',
  ]) {
    if (!executionControlExampleText.includes(marker)) fail(`headless execution-control adapter lost marker: ${marker}`);
  }
  for (const marker of [
    'verify_post_transition', 'post-contract verification requires exactly one canonical 0031 -> 0032 transition',
    'd1 contract-transition verify', 'EXACT_ONE_STEP_0032_CONTRACT', 'RUNTIME_WINDOW_0031_0032_VERIFIED',
  ]) {
    if (!contractText.includes(marker)) fail(`typed contract-transition lost post-CONTRACT invariant: ${marker}`);
  }
  for (const marker of ['--post-ledger-json', 'contract_transition_verify']) {
    if (!contractExampleText.includes(marker)) fail(`contract-transition entrypoint lost post-CONTRACT route: ${marker}`);
  }

  if (/create\s+table\s+[^\n;]*(?:d1|migration)[^\n;]*lock/is.test(text)) {
    fail('protected D1 executor must not invent a database-resident migration lock');
  }
  if (text.includes('python scripts/d1-prepare.py prepare')) {
    fail('protected ordinary executor must consume the admitted sealed plan and must never re-run prepare after authorization');
  }

  const authorizeIndex = text.indexOf('\n  authorize:');
  const migrateIndex = text.indexOf('\n  migrate:');
  if (authorizeIndex < 0 || migrateIndex < 0 || authorizeIndex >= migrateIndex) {
    fail('input authorization must be a separate job before the protected mutation job');
  }
  const authorizeBody = text.slice(authorizeIndex, migrateIndex);
  if (authorizeBody.includes('environment:')) fail('preflight authorization job must not bind any GitHub Environment');
  if (authorizeBody.includes('CLOUDFLARE_API_TOKEN')) fail('preflight authorization job must not receive provider credentials');
  for (const marker of [
    'test "$TARGET_ENVIRONMENT" = "staging"', 'test "$COMPONENT" = "catalog" || test "$COMPONENT" = "resolver"',
    'test "$MUTATION_AUTHORIZED" = "true"', 'test "$COMPONENT" = "catalog"', 'test -n "$EXPECTED_RELEASE_SET_ID"',
    AUTH_PROVENANCE_STEP, AUTH_DIGEST_MARKER, 'D1_TRANSACTION_AUTHORIZATION_V1',
    "comment.get('author_association') != 'OWNER'", 'display_title',
  ]) {
    if (!authorizeBody.includes(marker)) fail(`preflight authorization lost fail-closed marker: ${marker}`);
  }
  for (const forbidden of [
    REPLAY_PROOF_STEP, CONSUMPTION_STEP, REPLAY_RUN_MARKER,
    'actions/workflows/d1-migration-executor.yml/runs?event=workflow_dispatch&per_page=100',
    CONSUMPTION_ARTIFACT, TARGET_FENCE_CREATE_STEP, TARGET_FENCE_MARKER_STEP, TARGET_FENCE_VERIFY_STEP,
    RECEIPT_INIT_STEP, RECEIPT_MUTATION_STARTED_STEP,
  ]) {
    if (authorizeBody.includes(forbidden)) {
      fail(`preflight authorization must not consume, fence, or journal execution before typed executor admission: ${forbidden}`);
    }
  }

  const migrateStepsIndex = text.indexOf('\n    steps:', migrateIndex);
  if (migrateStepsIndex < 0) fail('protected mutation job steps are missing');
  const migrateHeader = text.slice(migrateIndex, migrateStepsIndex);
  if (!migrateHeader.includes('needs: authorize')) fail('protected mutation job must depend on preflight authorization');
  if (!migrateHeader.includes('environment: staging')) fail('protected mutation job lost literal staging Environment');
  if (!migrateHeader.includes('    env:\n')) fail('protected D1 executor job-level metadata environment is missing');
  if (migrateHeader.includes('CLOUDFLARE_API_TOKEN')) fail('provider API tokens must remain step-scoped');

  const normalized = normalizedShell(text);
  const escapedWrangler = PINNED_WRANGLER.replaceAll('.', '\\.');
  if ((normalized.match(new RegExp(`npx --yes ${escapedWrangler} d1 migrations apply\\b`, 'g')) ?? []).length !== 1) {
    fail('protected D1 executor must contain exactly one pinned Wrangler migrations apply site');
  }
  if ((normalized.match(new RegExp(`npx --yes ${escapedWrangler} d1 migrations apply\\b[^\\n]*?--remote\\b`, 'g')) ?? []).length !== 1) {
    fail('the sole protected D1 migration apply site must explicitly target --remote');
  }
  const pendingListSites = normalized.match(new RegExp(`npx --yes ${escapedWrangler} d1 migrations list\\b`, 'g')) ?? [];
  if (pendingListSites.length !== 2) {
    fail(`protected D1 executor must observe Wrangler pending migrations exactly twice; observed=${pendingListSites.length}`);
  }
  const pendingVerifySites = text.match(/d1-executor-plan\.py verify-pending/g) ?? [];
  if (pendingVerifySites.length !== 2) {
    fail(`protected D1 executor must verify the native/Wrangler pending equality exactly twice; observed=${pendingVerifySites.length}`);
  }
  const providerPattern = new RegExp(
    `npx --yes ${escapedWrangler} d1 (?:info|execute|time-travel info|migrations list|migrations apply)\\b`, 'g',
  );
  const providerSites = normalized.match(providerPattern) ?? [];
  if (providerSites.length === 0) fail('protected D1 executor contains no pinned Wrangler provider operations');
  if ((normalized.match(/--experimental-provision=false/g) ?? []).length < providerSites.length) {
    fail('every protected provider operation must explicitly disable experimental provisioning');
  }
  if ((normalized.match(/--experimental-auto-create=false/g) ?? []).length < providerSites.length) {
    fail('every protected provider operation must explicitly disable experimental auto-create');
  }

  const observeSteps = (text.match(/CLOUDFLARE_API_TOKEN: \$\{\{ secrets\.CLOUDFLARE_OBSERVE_API_TOKEN \}\}/g) ?? []).length;
  if (observeSteps < 7) fail(`provider observations must use the dedicated observe credential, including exact preapply revalidation; observed=${observeSteps}`);
  const deploySteps = (text.match(/CLOUDFLARE_API_TOKEN: \$\{\{ secrets\.CLOUDFLARE_API_TOKEN \}\}/g) ?? []).length;
  if (deploySteps !== 1) fail(`deploy-capable credential must appear exactly once; observed=${deploySteps}`);

  const provenanceIndex = text.indexOf(`\n      - name: ${AUTH_PROVENANCE_STEP}`);
  const admissionIndex = text.indexOf('Load immutable prepared transaction and verify typed executor admission');
  const replayIndex = text.indexOf(`\n      - name: ${REPLAY_PROOF_STEP}`);
  const consumptionIndex = text.indexOf(`\n      - name: ${CONSUMPTION_STEP}`);
  const targetFenceCreateIndex = text.indexOf(`\n      - name: ${TARGET_FENCE_CREATE_STEP}`);
  const targetFenceMarkerIndex = text.indexOf(`\n      - name: ${TARGET_FENCE_MARKER_STEP}`);
  const receiptInitIndex = text.indexOf(`\n      - name: ${RECEIPT_INIT_STEP}`);
  const receiptInitialMarkerIndex = text.indexOf(`\n      - name: ${RECEIPT_INITIAL_MARKER_STEP}`);
  const metadataIndex = text.indexOf('Materialize exact metadata inputs and bounded provider config');
  const policyIndex = text.indexOf('Consume sealed ordinary plan and run fresh compatibility or contract gates');
  const diagnosticUploadIndex = text.indexOf('Upload metadata-only D1 gate diagnostics on policy failure');
  const materializeIndex = text.indexOf('Materialize exactly authorized planned_migrations from typed successor projection');
  const firstPendingIndex = text.indexOf('Compare Wrangler pending list to exact authorized plan with observe credential');
  const targetFenceVerifyIndex = text.indexOf(`\n      - name: ${TARGET_FENCE_VERIFY_STEP}`);
  const receiptPrewriteIndex = text.indexOf(`\n      - name: ${RECEIPT_PREWRITE_STEP}`);
  const preapplyObserveIndex = text.indexOf(`\n      - name: ${PREAPPLY_OBSERVE_STEP}`);
  const receiptMutationStartedIndex = text.indexOf(`\n      - name: ${RECEIPT_MUTATION_STARTED_STEP}`);
  const receiptMutationMarkerIndex = text.indexOf(`\n      - name: ${RECEIPT_MUTATION_MARKER_STEP}`);
  const applyStepIndex = text.indexOf(`\n      - name: ${APPLY_STEP}`);
  const receiptAppliedIndex = text.indexOf(`\n      - name: ${RECEIPT_APPLIED_STEP}`);
  const receiptAppliedMarkerIndex = text.indexOf(`\n      - name: ${RECEIPT_APPLIED_MARKER_STEP}`);
  const rereadIndex = text.indexOf(`\n      - name: ${REREAD_STEP}`);
  const receiptPostObservedIndex = text.indexOf(`\n      - name: ${RECEIPT_POST_OBSERVED_STEP}`);
  const postVerifyIndex = text.indexOf('Require exact target through credential-free opsctl verify');
  const postInvariantIndex = text.indexOf('Verify post-apply database invariants with observe credential', postVerifyIndex);
  const receiptCompleteIndex = text.indexOf(`\n      - name: ${RECEIPT_COMPLETE_STEP}`);
  const receiptTerminalizeIndex = text.indexOf(`\n      - name: ${RECEIPT_TERMINALIZE_STEP}`);
  const receiptTerminalMarkerIndex = text.indexOf(`\n      - name: ${RECEIPT_TERMINAL_MARKER_STEP}`);
  const evidenceIndex = text.indexOf('Build metadata-only migration evidence');
  const observeIndex = text.indexOf(OBSERVE_REF);
  const deployIndex = text.indexOf(DEPLOY_REF);
  if (!(provenanceIndex >= 0
      && admissionIndex > provenanceIndex
      && replayIndex > admissionIndex
      && consumptionIndex > replayIndex
      && targetFenceCreateIndex > consumptionIndex
      && targetFenceMarkerIndex > targetFenceCreateIndex
      && receiptInitIndex > targetFenceMarkerIndex
      && receiptInitialMarkerIndex > receiptInitIndex
      && metadataIndex > receiptInitialMarkerIndex
      && observeIndex > metadataIndex
      && policyIndex > observeIndex
      && diagnosticUploadIndex > policyIndex
      && materializeIndex > diagnosticUploadIndex
      && firstPendingIndex > materializeIndex
      && targetFenceVerifyIndex > firstPendingIndex
      && receiptPrewriteIndex > targetFenceVerifyIndex
      && preapplyObserveIndex > receiptPrewriteIndex
      && receiptMutationStartedIndex > preapplyObserveIndex
      && receiptMutationMarkerIndex > receiptMutationStartedIndex
      && applyStepIndex > receiptMutationMarkerIndex
      && deployIndex > receiptMutationMarkerIndex
      && receiptAppliedIndex > applyStepIndex
      && receiptAppliedMarkerIndex > receiptAppliedIndex
      && rereadIndex > receiptAppliedMarkerIndex
      && receiptPostObservedIndex > rereadIndex
      && postVerifyIndex > receiptPostObservedIndex
      && postInvariantIndex > postVerifyIndex
      && receiptCompleteIndex > postInvariantIndex
      && receiptTerminalizeIndex > receiptCompleteIndex
      && receiptTerminalMarkerIndex > receiptTerminalizeIndex
      && evidenceIndex > receiptTerminalMarkerIndex)) {
    fail('TX-4C ordering must preserve admission -> durable fence -> durable initial receipt -> typed prewrite fence -> observe-only revalidation -> durable MUTATION_STARTED -> sole deploy apply -> mechanically known applied events -> post-observed -> verified/completed -> fail-closed terminal receipt -> evidence');
  }

  const replayBody = stepBody(text, REPLAY_PROOF_STEP);
  for (const marker of [
    'actions/workflows/d1-migration-executor.yml/runs?event=workflow_dispatch&per_page=100',
    REPLAY_RUN_MARKER, 'actions/runs/$run_id/jobs?per_page=100', REPLAY_SUCCESS_MARKER,
    'test "$consumed" -eq 0', CONSUMPTION_ARTIFACT, 'AUTHORIZATION_CONSUMPTION_READY',
  ]) {
    if (!replayBody.includes(marker)) fail(`post-admission replay proof lost successful-consumption invariant: ${marker}`);
  }
  if (replayBody.includes('CLOUDFLARE_API_TOKEN')) {
    fail('post-admission replay proof must remain provider-credential-free');
  }

  const consumptionBody = stepBody(text, CONSUMPTION_STEP);
  for (const marker of [
    'actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a',
    CONSUMPTION_ARTIFACT, 'if-no-files-found: error', 'retention-days: 30',
  ]) {
    if (!consumptionBody.includes(marker)) fail(`authorization consumption marker lost durable artifact contract: ${marker}`);
  }
  if (consumptionBody.includes('CLOUDFLARE_API_TOKEN')) {
    fail('authorization consumption marker must be emitted before provider credentials are exposed');
  }

  const targetFenceCreateBody = stepBody(text, TARGET_FENCE_CREATE_STEP);
  for (const marker of [
    "'fence_epoch': int(os.environ['GITHUB_RUN_NUMBER'])", "'executor_run_id': int(os.environ['GITHUB_RUN_ID'])",
    "'run_attempt': int(os.environ['GITHUB_RUN_ATTEMPT'])", 'd1-execution-control acquire-fence',
    TARGET_FENCE_LEASE, 'TARGET_FENCE_ACQUIRED',
  ]) {
    if (!targetFenceCreateBody.includes(marker)) fail(`typed target-fence acquisition lost marker: ${marker}`);
  }
  if (targetFenceCreateBody.includes('CLOUDFLARE_API_TOKEN: ${{ secrets.')) {
    fail('typed target-fence acquisition must remain provider-credential-free');
  }

  const targetFenceMarkerBody = stepBody(text, TARGET_FENCE_MARKER_STEP);
  for (const marker of [
    'actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a',
    'd1-target-fence-${{ github.run_number }}-${{ github.run_id }}-${{ github.run_attempt }}',
    TARGET_FENCE_LEASE, 'if-no-files-found: error', 'retention-days: 30',
  ]) {
    if (!targetFenceMarkerBody.includes(marker)) fail(`durable target-fence marker lost artifact contract: ${marker}`);
  }
  if (targetFenceMarkerBody.includes('CLOUDFLARE_API_TOKEN')) {
    fail('durable target-fence marker must remain provider-credential-free');
  }

  const receiptInitBody = stepBody(text, RECEIPT_INIT_STEP);
  for (const marker of [
    'execution-receipt-seed.json', "transaction_plan', {}).get('recovery_strategy')",
    "recovery = 'FAIL_FORWARD_ONLY'", 'd1-execution-control initialize-receipt',
    '--prepared-at-unix-seconds', '--authorized-at-unix-seconds', RECEIPT,
    '.events[0].kind == "PREPARED"', '.events[1].kind == "AUTHORIZED"',
  ]) {
    if (!receiptInitBody.includes(marker)) fail(`execution receipt initialization lost typed attempt binding: ${marker}`);
  }
  if (receiptInitBody.includes('CLOUDFLARE_API_TOKEN: ${{ secrets.')) {
    fail('execution receipt initialization must be credential-free');
  }

  const receiptInitialBody = stepBody(text, RECEIPT_INITIAL_MARKER_STEP);
  for (const marker of [
    'actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a', RECEIPT,
    'd1-execution-receipt-${{ github.run_number }}-${{ github.run_id }}-${{ github.run_attempt }}-initial',
    'if-no-files-found: error', 'retention-days: 30',
  ]) {
    if (!receiptInitialBody.includes(marker)) fail(`initial receipt snapshot lost durable artifact contract: ${marker}`);
  }
  if (receiptInitialBody.includes('CLOUDFLARE_API_TOKEN')) fail('initial receipt snapshot must remain credential-free');

  const targetFenceVerifyBody = stepBody(text, TARGET_FENCE_VERIFY_STEP);
  for (const marker of [
    'actions/workflows/d1-migration-executor.yml/runs?event=workflow_dispatch&per_page=100',
    TARGET_FENCE_NEWER_RUN_MARKER, 'actions/runs/$run_id/jobs?filter=all&per_page=100',
    TARGET_FENCE_SUCCESS_MARKER, 'actions/runs/$run_id/artifacts?per_page=100',
    "'history_complete': True", "'current_marker_succeeded': True",
    TARGET_FENCE_OBSERVATION, 'd1-execution-control verify-fence', TARGET_FENCE_VERIFICATION,
    'TARGET_FENCE_VERIFIED',
  ]) {
    if (!targetFenceVerifyBody.includes(marker)) fail(`typed prewrite target-fence verification lost marker: ${marker}`);
  }
  if (targetFenceVerifyBody.includes('CLOUDFLARE_API_TOKEN: ${{ secrets.')) {
    fail('typed prewrite target-fence verification must complete before provider mutation credentials exist');
  }

  const receiptPrewriteBody = stepBody(text, RECEIPT_PREWRITE_STEP);
  for (const marker of ['PREWRITE_FENCE_PASS', 'd1-execution-control append-receipt', RECEIPT]) {
    if (!receiptPrewriteBody.includes(marker)) fail(`PREWRITE_FENCE_PASS receipt projection lost marker: ${marker}`);
  }
  if (receiptPrewriteBody.includes('CLOUDFLARE_API_TOKEN: ${{ secrets.')) fail('PREWRITE_FENCE_PASS append must be credential-free');

  const policyBody = text.slice(policyIndex, diagnosticUploadIndex);
  if (!policyBody.includes(SEALED_PLAN_EXTRACTION)) {
    fail('ordinary post-authorization policy must consume executor-admission.execution_plan verbatim');
  }
  if (policyBody.includes('d1-prepare.py prepare')) {
    fail('ordinary post-authorization policy must abort on drift instead of replanning an authorized transaction');
  }
  if (!policyBody.includes('d1 compatibility')) {
    fail('fresh compatibility must remain a fail-closed pre-write drift gate after sealed-plan admission');
  }
  const materializeBody = stepBody(text, 'Materialize exactly authorized planned_migrations from typed successor projection');
  if (!materializeBody.includes('--require-plan-digests')) {
    fail('ordinary authorized materialization must require sealed migration digests and predecessor state');
  }

  const diagnosticUploadStep = stepBody(text, 'Upload metadata-only D1 gate diagnostics on policy failure');
  for (const marker of [
    "if: failure() && steps.policy.outcome == 'failure'", DIAGNOSTICS_PATH,
    'if-no-files-found: error', 'retention-days: 30',
  ]) {
    if (!diagnosticUploadStep.includes(marker)) fail(`D1 policy failure diagnostic upload lost marker: ${marker}`);
  }
  if (diagnosticUploadStep.includes('CLOUDFLARE_API_TOKEN') || diagnosticUploadStep.includes('ledger-before.json')) {
    fail('D1 policy failure diagnostic upload must remain metadata-only');
  }

  const preapplyObserveBody = stepBody(text, PREAPPLY_OBSERVE_STEP);
  for (const marker of [
    OBSERVE_REF, 'provider-identity-fence.json', 'ledger-fence.json', 'status-fence.json',
    'cmp --silent artifacts/d1-migration/status-before.json artifacts/d1-migration/status-fence.json',
    'cmp --silent artifacts/d1-migration/ledger-before-names.json artifacts/d1-migration/ledger-fence-names.json',
    'wrangler-pending-fence.txt', 'd1-executor-plan.py verify-pending',
  ]) {
    if (!preapplyObserveBody.includes(marker)) fail(`observe-only preapply revalidation lost marker: ${marker}`);
  }
  if (preapplyObserveBody.includes(DEPLOY_REF) || preapplyObserveBody.includes('d1 migrations apply')) {
    fail('preapply identity/ledger/pending revalidation must use observe credential only and must not mutate provider state');
  }

  const mutationStartedBody = stepBody(text, RECEIPT_MUTATION_STARTED_STEP);
  for (const marker of ['MUTATION_STARTED', 'd1-execution-control append-receipt', RECEIPT]) {
    if (!mutationStartedBody.includes(marker)) fail(`MUTATION_STARTED receipt projection lost marker: ${marker}`);
  }
  if (mutationStartedBody.includes('CLOUDFLARE_API_TOKEN: ${{ secrets.')) fail('MUTATION_STARTED append must be credential-free');

  const mutationMarkerBody = stepBody(text, RECEIPT_MUTATION_MARKER_STEP);
  for (const marker of [
    'actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a', RECEIPT,
    'd1-execution-receipt-${{ github.run_number }}-${{ github.run_id }}-${{ github.run_attempt }}-mutation-started',
    'if-no-files-found: error', 'retention-days: 30',
  ]) {
    if (!mutationMarkerBody.includes(marker)) fail(`durable MUTATION_STARTED snapshot lost marker: ${marker}`);
  }
  if (mutationMarkerBody.includes('CLOUDFLARE_API_TOKEN')) fail('durable MUTATION_STARTED snapshot must be credential-free');

  const deployStep = stepBody(text, APPLY_STEP);
  for (const marker of [DEPLOY_REF, 'test -n "$CLOUDFLARE_API_TOKEN"', 'd1 migrations apply', '--remote']) {
    if (!deployStep.includes(marker)) fail(`sole deploy step lost exact apply marker: ${marker}`);
  }
  for (const forbidden of [OBSERVE_REF, 'd1 info', 'd1 execute', 'd1 migrations list', 'ledger-fence.json', 'status-fence.json', 'verify-pending']) {
    if (deployStep.includes(forbidden)) fail(`sole deploy credential step must contain only actual apply boundary, not read-only revalidation: ${forbidden}`);
  }

  const appliedBody = stepBody(text, RECEIPT_APPLIED_STEP);
  for (const marker of ['MIGRATION_APPLIED', 'expected-pending.json', 'd1-execution-control append-receipt', RECEIPT]) {
    if (!appliedBody.includes(marker)) fail(`mechanically known MIGRATION_APPLIED projection lost marker: ${marker}`);
  }
  if (appliedBody.includes('CLOUDFLARE_API_TOKEN: ${{ secrets.')) fail('MIGRATION_APPLIED append must be credential-free and occur only after successful apply');

  const appliedMarkerBody = stepBody(text, RECEIPT_APPLIED_MARKER_STEP);
  if (!appliedMarkerBody.includes(RECEIPT) || !appliedMarkerBody.includes('-applied')) {
    fail('post-apply receipt snapshot must durably preserve mechanically known applied events');
  }

  const postObservedBody = stepBody(text, RECEIPT_POST_OBSERVED_STEP);
  for (const marker of ['POST_OBSERVED', 'd1-execution-control append-receipt', RECEIPT]) {
    if (!postObservedBody.includes(marker)) fail(`POST_OBSERVED receipt projection lost marker: ${marker}`);
  }
  if (postObservedBody.includes('CLOUDFLARE_API_TOKEN: ${{ secrets.')) fail('POST_OBSERVED append must be credential-free');

  if (postVerifyIndex < 0 || postInvariantIndex <= postVerifyIndex) {
    fail('post-apply verification step is missing or malformed');
  }
  const postVerifyBody = text.slice(postVerifyIndex, postInvariantIndex);
  for (const marker of [
    POST_CONTRACT_LEDGER, 'd1 contract-transition verify', 'predecessor_ledger_state',
    'runtime_target_revision', 'supported_schema_max', 'transition_migrations',
  ]) {
    if (!postVerifyBody.includes(marker)) fail(`post-CONTRACT verification lost required typed proof: ${marker}`);
  }

  const completeBody = stepBody(text, RECEIPT_COMPLETE_STEP);
  for (const marker of ['VERIFIED', 'COMPLETED', 'd1-execution-control append-receipt', RECEIPT]) {
    if (!completeBody.includes(marker)) fail(`successful receipt completion lost marker: ${marker}`);
  }
  if (completeBody.includes('CLOUDFLARE_API_TOKEN: ${{ secrets.')) fail('VERIFIED/COMPLETED receipt append must be credential-free');

  const terminalizeBody = stepBody(text, RECEIPT_TERMINALIZE_STEP);
  for (const marker of [
    'if: always()', 'AUTHORIZED)', 'append_event PREWRITE_ABORTED', 'append_event FAILED_NO_EFFECT',
    'PREWRITE_FENCE_PASS)', 'MUTATION_STARTED|MIGRATION_APPLIED|POST_OBSERVED)',
    'append_event RECOVERY_REQUIRED', 'VERIFIED)', 'append_event COMPLETED',
    'COMPLETED|RECOVERY_REQUIRED|FAILED_NO_EFFECT)', 'd1-execution-control append-receipt', RECEIPT,
  ]) {
    if (!terminalizeBody.includes(marker)) fail(`fail-closed receipt terminalizer lost marker: ${marker}`);
  }
  if (terminalizeBody.includes('CLOUDFLARE_API_TOKEN: ${{ secrets.')) fail('receipt terminalizer must be credential-free');

  const terminalMarkerBody = stepBody(text, RECEIPT_TERMINAL_MARKER_STEP);
  for (const marker of [
    "if: always() && hashFiles('artifacts/d1-migration/execution-receipt.json') != ''",
    'actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a', RECEIPT,
    'd1-execution-receipt-${{ github.run_number }}-${{ github.run_id }}-${{ github.run_attempt }}-terminal',
    'if-no-files-found: error', 'retention-days: 30',
  ]) {
    if (!terminalMarkerBody.includes(marker)) fail(`terminal receipt artifact lost marker: ${marker}`);
  }

  const evidenceBody = stepBody(text, 'Build metadata-only migration evidence');
  for (const marker of [
    "receipt = load('execution-receipt.json')", "receipt['events'][-1]['kind'] != 'COMPLETED'",
    "'execution_receipt': receipt", "'execution_receipt_id': receipt['receipt_id']",
  ]) {
    if (!evidenceBody.includes(marker)) fail(`successful migration evidence lost final receipt binding: ${marker}`);
  }

  const remoteMutationPaths = [];
  for (const workflowPath of workflows) {
    const candidate = normalizedShell(await readFile(path.join(root, workflowPath), 'utf8'));
    if (/d1 migrations apply\b[^\n]*?--remote\b/.test(candidate)) remoteMutationPaths.push(workflowPath);
  }
  if (remoteMutationPaths.length !== 1 || remoteMutationPaths[0] !== EXECUTOR) {
    fail(`exactly one workflow may own remote D1 migrations apply; observed=${JSON.stringify(remoteMutationPaths)}`);
  }
}

async function expectRejected(label, text) {
  try {
    await validateExecutor(text, ROOT);
  } catch {
    return;
  }
  fail(`negative protected-executor fixture unexpectedly passed: ${label}`);
}

async function selfTest(text) {
  runDiagnosticsHelperSelfTest();
  await validateExecutor(text, ROOT);
  const promotionText = await readFile(path.join(ROOT, PROMOTION), 'utf8');
  let mutexRejected = false;
  try {
    validateSharedMutationGroup(
      text,
      replaceFixture('promotion mutex drift', promotionText, `group: ${SHARED_MUTATION_GROUP}`, 'group: unsafe-independent-promotion'),
    );
  } catch {
    mutexRejected = true;
  }
  if (!mutexRejected) fail('independent D1/promotion mutation-group fixture unexpectedly passed');

  for (const [label, from, to] of [
    ['automatic restore', 'd1 time-travel info', 'd1 time-travel restore'],
    ['concurrency cancellation', 'cancel-in-progress: false', 'cancel-in-progress: true'],
    ['provider auto-create', '--experimental-auto-create=false', '--experimental-auto-create=true'],
    ['raw legacy Catalog execution root', "'migrations_dir': 'migrations-bounded'", "'migrations_dir': '../../migrations/d1'"],
    ['missing normalized ledger fence', 'cmp --silent artifacts/d1-migration/ledger-before-names.json artifacts/d1-migration/ledger-fence-names.json', '# removed-ledger-fence'],
    ['missing Wrangler pending fence', 'd1-executor-plan.py verify-pending', 'd1-executor-plan.py removed-pending-check'],
    ['missing contract confirmation binding', CONTRACT_CONFIRMATION, 'test -n "$CONFIRMATION"'],
    ['missing typed post-contract ledger proof', POST_CONTRACT_LEDGER, '--removed-post-ledger-json'],
    ['missing compatibility command diagnostic', 'D1_COMPATIBILITY_COMMAND_FAILED', 'D1_REMOVED_COMPATIBILITY_COMMAND_DIAGNOSTIC'],
    ['missing sealed ordinary plan extraction', SEALED_PLAN_EXTRACTION, '# removed-sealed-plan-extraction'],
    ['missing authorized migration digest enforcement', '--require-plan-digests', '--removed-require-plan-digests'],
    ['missing durable diagnostic path', `          path: ${DIAGNOSTICS_PATH}`, '          path: artifacts/d1-migration/removed-diagnostics.jsonl'],
    ['missing diagnostic failure condition', "if: failure() && steps.policy.outcome == 'failure'", "if: steps.policy.outcome == 'success'"],
    ['missing authorization digest format gate', '[[ "$AUTHORIZATION_DIGEST" =~ ^[0-9a-f]{64}$ ]]', 'test -n "$AUTHORIZATION_DIGEST"'],
    ['missing ordinary rerun rejection', 'test "$GITHUB_RUN_ATTEMPT" = "1"', 'test -n "$GITHUB_RUN_ATTEMPT"'],
    ['missing authorization digest recomputation', AUTH_DIGEST_MARKER, "hashlib.sha256(b'unsafe').hexdigest()"],
    ['missing immutable authorization comment', "updated_at != created_at", "updated_at == created_at"],
    ['missing prior-run replay fence', REPLAY_RUN_MARKER, '.run_number == $current_number'],
    ['missing successful consumption criterion', REPLAY_SUCCESS_MARKER, '.name == $step'],
    ['missing consumption marker step', `      - name: ${CONSUMPTION_STEP}`, '      - name: Removed authorization consumption marker'],
    ['missing canonical target-fence epoch', "'fence_epoch': int(os.environ['GITHUB_RUN_NUMBER'])", "'fence_epoch': 1"],
    ['missing durable target-fence marker', `      - name: ${TARGET_FENCE_MARKER_STEP}`, '      - name: Removed durable target fence'],
    ['missing newer target-fence horizon', TARGET_FENCE_NEWER_RUN_MARKER, '.run_number < $current_number'],
    ['missing complete target-fence history', "'history_complete': True", "'history_complete': False"],
    ['missing typed target-fence verification', 'd1-execution-control verify-fence', 'd1-execution-control removed-verify-fence'],
    ['missing receipt initialization', 'd1-execution-control initialize-receipt', 'd1-execution-control removed-initialize-receipt'],
    ['missing initial receipt durability', `      - name: ${RECEIPT_INITIAL_MARKER_STEP}`, '      - name: Removed initial execution receipt snapshot'],
    ['missing PREWRITE_FENCE_PASS projection', `      - name: ${RECEIPT_PREWRITE_STEP}`, '      - name: Removed PREWRITE_FENCE_PASS receipt'],
    ['missing MUTATION_STARTED projection', `      - name: ${RECEIPT_MUTATION_STARTED_STEP}`, '      - name: Removed MUTATION_STARTED receipt'],
    ['missing durable MUTATION_STARTED snapshot', `      - name: ${RECEIPT_MUTATION_MARKER_STEP}`, '      - name: Removed durable MUTATION_STARTED snapshot'],
    ['missing mechanically known MIGRATION_APPLIED events', `      - name: ${RECEIPT_APPLIED_STEP}`, '      - name: Removed MIGRATION_APPLIED receipt'],
    ['missing POST_OBSERVED projection', `      - name: ${RECEIPT_POST_OBSERVED_STEP}`, '      - name: Removed POST_OBSERVED receipt'],
    ['missing fail-closed receipt terminalizer', `      - name: ${RECEIPT_TERMINALIZE_STEP}`, '      - name: Removed receipt terminalizer'],
    ['missing terminal receipt durability', `      - name: ${RECEIPT_TERMINAL_MARKER_STEP}`, '      - name: Removed terminal receipt snapshot'],
    ['missing receipt evidence binding', "'execution_receipt': receipt", "'removed_execution_receipt': receipt"],
  ]) {
    await expectRejected(label, replaceFixture(label, text, from, to));
  }
  await expectRejected(
    'pre-admission run-existence replay fence',
    replaceFixture(
      'pre-admission run-existence replay fence',
      text,
      '          current_run="$RUNNER_TEMP/current-d1-executor-run.json"',
      '          # actions/workflows/d1-migration-executor.yml/runs?event=workflow_dispatch&per_page=100\n          current_run="$RUNNER_TEMP/current-d1-executor-run.json"',
    ),
  );
  await expectRejected(
    'post-authorization ordinary replanning',
    replaceFixture(
      'post-authorization ordinary replanning',
      text,
      SEALED_PLAN_EXTRACTION,
      'python scripts/d1-prepare.py prepare # forbidden post-authorization replan',
    ),
  );
  await expectRejected(
    'reintroduced workflow-call bypass',
    `# workflow_call:\n${text}`,
  );
  await expectRejected('deploy token used for observation', replaceFixture('deploy token used for observation', text, OBSERVE_REF, DEPLOY_REF));
  await expectRejected(
    'second remote apply',
    `${text}\n# npx --yes ${PINNED_WRANGLER} d1 migrations apply X --remote --experimental-provision=false --experimental-auto-create=false\n`,
  );
  console.log('Protected D1 executor typed-admission, durable target fence, append-only ExecutionReceipt ordering/durability, observe-only preapply revalidation, durable MUTATION_STARTED-before-deploy, fail-closed terminalization, sealed-plan, exact-prestate, diagnostics, post-CONTRACT and one-owner negative fixtures passed.');
}

async function main() {
  const text = await readFile(path.join(ROOT, EXECUTOR), 'utf8');
  if (process.argv.includes('--self-test')) {
    await selfTest(text);
    return;
  }
  if (process.argv.length > 2) fail(`unknown arguments: ${process.argv.slice(2).join(' ')}`);
  await validateExecutor(text, ROOT);
  console.log('Protected D1 executor contract passed: workflow-dispatch-only, immutable OWNER authorization provenance, typed admission before one-shot consumption, target-scoped typed fence, append-only ExecutionReceipt, observe-only preapply revalidation, durable MUTATION_STARTED before sole deploy credential/apply, mechanically known applied events, fail-closed terminal receipt, exact sealed plan/prestate, typed post-CONTRACT verification, one remote apply owner, no automatic restore or provisioning.');
}

main().catch((error) => {
  console.error(`D1 executor gate error: ${error instanceof Error ? error.message : error}`);
  process.exitCode = 1;
});