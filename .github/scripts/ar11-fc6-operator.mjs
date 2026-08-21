#!/usr/bin/env node

import { readFileSync } from 'node:fs';
import process from 'node:process';

const REPOSITORY = 'iamaman11/part-crm-emai-profile';
const ISSUE_NUMBER = 399;
const WORKFLOW = 'release-set-promotion.yml';
const WORKFLOW_PATH = '.github/workflows/release-set-promotion.yml';
const TRANSPORT_WORKFLOW = 'AR-11 FC-6 Operator Transport';
const TRANSACTION_PATH = 'docs/evidence/ar11-fc6-operator-transaction.json';
const RELEASE = 'release-set-v2-sha256-[0-9a-f]{64}';
const RELEASE_ID = new RegExp(`^${RELEASE}$`);
const SHA = /^[0-9a-f]{40}$/;
const TRANSACTION_KEYS = Object.freeze([
  'schema_version',
  'kind',
  'authority',
  'tracker_issue',
  'production_authorized',
  'operation',
  'release_set_a',
  'release_set_b',
  'initial_expected_current',
  'final_expected_current',
  'confirmation',
]);
const STAGES = Object.freeze(['a-to-b', 'b-no-change', 'b-to-a', 'a-no-change', 'negative-rollback']);

function fail(message) { throw new Error(`AR-11 FC-6 trusted-main operator rejected: ${message}`); }
function canonicalJson(value) { return `${JSON.stringify(value, null, 2)}\n`; }
function sleep(ms) { return new Promise((resolve) => setTimeout(resolve, ms)); }

function validateTransaction(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) fail('ceremony transaction must be one object');
  if (JSON.stringify(Object.keys(value)) !== JSON.stringify(TRANSACTION_KEYS)) fail('ceremony transaction keys/order drifted');
  if (value.schema_version !== 1) fail('ceremony transaction schema_version must be 1');
  if (value.kind !== 'AR11_FC6_STAGING_CEREMONY') fail('ceremony transaction kind drifted');
  if (value.authority !== 'TRANSPORT_REQUEST_ONLY') fail('ceremony transaction must not claim mutation authority');
  if (value.tracker_issue !== ISSUE_NUMBER) fail(`ceremony transaction tracker_issue must be ${ISSUE_NUMBER}`);
  if (value.production_authorized !== false) fail('ceremony transaction must keep production unauthorized');
  if (value.operation !== 'full-staging-ceremony') fail('ceremony transaction operation must be full-staging-ceremony');
  if (!RELEASE_ID.test(value.release_set_a ?? '') || !RELEASE_ID.test(value.release_set_b ?? '')) fail('ceremony Release Set IDs must be canonical v2 IDs');
  if (value.release_set_a === value.release_set_b) fail('ceremony A and B must be source-distinct Release Sets');
  if (value.initial_expected_current !== value.release_set_a) fail('ceremony initial expected-current must equal A');
  if (value.final_expected_current !== value.release_set_a) fail('ceremony final expected-current must equal A');
  if (value.confirmation !== `${value.release_set_a}:${value.release_set_b}:FC6`) fail('ceremony confirmation must bind exact A and B');
  return { a: value.release_set_a, b: value.release_set_b };
}

function parseTransactionBytes(raw) {
  let value;
  try { value = JSON.parse(raw); } catch (error) { fail(`ceremony transaction is invalid JSON: ${error instanceof Error ? error.message : error}`); }
  if (raw !== canonicalJson(value)) fail('ceremony transaction must use canonical JSON bytes');
  return validateTransaction(value);
}

function validatePushEvent(event, githubSha, githubRef, eventName) {
  if (eventName !== 'push' || githubRef !== 'refs/heads/main') fail('operator executes only for push on refs/heads/main');
  if (!event || typeof event !== 'object' || Array.isArray(event)) fail('push event must be one object');
  if (event.repository?.full_name !== REPOSITORY) fail(`push repository must be ${REPOSITORY}`);
  if (event.ref !== 'refs/heads/main' || event.deleted === true || event.created === true || event.forced === true) fail('operator requires an ordinary protected-main update');
  if (!SHA.test(event.before ?? '') || !SHA.test(event.after ?? '') || event.before === event.after) fail('push before/after identity is invalid');
  if (event.after !== githubSha || event.head_commit?.id !== event.after) fail('push event must bind exact checked-out main SHA');
  return { before: event.before, after: event.after };
}

function validateCompare(compare, before, after) {
  if (!compare || typeof compare !== 'object' || Array.isArray(compare)) fail('protected-main compare response is malformed');
  const expectedUrl = `https://api.github.com/repos/${REPOSITORY}/compare/${before}...${after}`;
  if (compare.url !== expectedUrl) fail('protected-main compare URL is not bound to exact before/after');
  if (compare.status !== 'ahead' || compare.ahead_by !== 1 || compare.behind_by !== 0 || compare.total_commits !== 1) fail('operator transaction must be exactly one accepted-main commit');
  if (compare.merge_base_commit?.sha !== before || compare.base_commit?.sha !== before) fail('operator transaction compare base/merge-base drifted');
  if (!Array.isArray(compare.commits) || compare.commits.length !== 1 || compare.commits[0]?.sha !== after) fail('operator transaction commit identity drifted');
  if (!Array.isArray(compare.files) || compare.files.length !== 1) fail('operator transaction commit must change exactly one data-only file');
  const file = compare.files[0];
  if (file?.filename !== TRANSACTION_PATH || !['added', 'modified'].includes(file?.status) || (file?.deletions ?? 0) !== 0) fail(`operator transaction may only add or modify ${TRANSACTION_PATH}`);
}

async function github(path, { method = 'GET', body } = {}) {
  const token = process.env.GITHUB_TOKEN;
  if (!token) fail('ephemeral github.token is unavailable');
  const response = await fetch(`https://api.github.com${path}`, {
    method,
    headers: {
      Accept: 'application/vnd.github+json',
      Authorization: `Bearer ${token}`,
      'Content-Type': 'application/json',
      'User-Agent': 'ar11-fc6-trusted-main-operator',
      'X-GitHub-Api-Version': '2022-11-28',
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!response.ok) {
    const text = (await response.text()).slice(0, 1200);
    fail(`GitHub API ${method} ${path} failed HTTP ${response.status}: ${text}`);
  }
  if (response.status === 204) return null;
  return response.json();
}

async function assertProtectedMain(expectedSha) {
  const main = await github(`/repos/${REPOSITORY}/branches/main`);
  if (main?.protected !== true || main?.commit?.sha !== expectedSha) fail('ceremony commit must remain the current protected main while dispatching');
}

async function bindHostedTransaction(before, after, localRaw) {
  await assertProtectedMain(after);
  const compare = await github(`/repos/${REPOSITORY}/compare/${before}...${after}`);
  validateCompare(compare, before, after);
  const payload = await github(`/repos/${REPOSITORY}/contents/${TRANSACTION_PATH}?ref=${after}`);
  if (payload?.type !== 'file' || payload?.encoding !== 'base64' || typeof payload?.content !== 'string') fail('hosted ceremony transaction response is malformed');
  const hostedRaw = Buffer.from(payload.content.replace(/\s/g, ''), 'base64').toString('utf8');
  if (hostedRaw !== localRaw) fail('checked-out ceremony transaction bytes differ from hosted exact-SHA bytes');
}

function stageRequest(stage, a, b, ceremonyId, sourceRunId = '') {
  if (!STAGES.includes(stage)) fail(`unknown ceremony stage ${stage}`);
  if (stage === 'a-to-b') return { operation: 'promote', releaseSetId: b, expectedCurrent: a, sourceRunId: '', confirmation: `${b}:${a}`, requestId: `${ceremonyId}-${stage}` };
  if (stage === 'b-no-change') return { operation: 'promote', releaseSetId: b, expectedCurrent: b, sourceRunId: '', confirmation: `${b}:${b}`, requestId: `${ceremonyId}-${stage}` };
  if (stage === 'b-to-a') return { operation: 'promote', releaseSetId: a, expectedCurrent: b, sourceRunId: '', confirmation: `${a}:${b}`, requestId: `${ceremonyId}-${stage}` };
  if (stage === 'a-no-change') return { operation: 'promote', releaseSetId: a, expectedCurrent: a, sourceRunId: '', confirmation: `${a}:${a}`, requestId: `${ceremonyId}-${stage}` };
  if (!/^[1-9][0-9]*$/.test(sourceRunId)) fail('negative rollback requires completed A-to-A canonical run id');
  return { operation: 'rollback-negative', releaseSetId: a, expectedCurrent: a, sourceRunId, confirmation: `${a}:${sourceRunId}`, requestId: `${ceremonyId}-${stage}` };
}

function expectedRunTitle(request) { return `AR11 ${request.operation} ${request.requestId}`; }

function findBoundRuns(runs, request, mainSha) {
  if (!Array.isArray(runs)) fail('workflow run list response is malformed');
  const title = expectedRunTitle(request);
  return runs.filter((run) => run?.event === 'workflow_dispatch' && run?.head_branch === 'main' && run?.head_sha === mainSha && run?.path === WORKFLOW_PATH && run?.display_title === title && Number.isInteger(run?.id) && run.id > 0);
}

async function listDispatchRuns() {
  const runs = [];
  for (let page = 1; page <= 20; page += 1) {
    const value = await github(`/repos/${REPOSITORY}/actions/workflows/${WORKFLOW}/runs?event=workflow_dispatch&branch=main&per_page=100&page=${page}`);
    if (!Array.isArray(value?.workflow_runs)) fail('workflow run list response is malformed');
    runs.push(...value.workflow_runs);
    if (value.workflow_runs.length < 100) return runs;
  }
  fail('canonical dispatch history exceeds bounded idempotency scan');
}

async function dispatchOrReuse(request, mainSha) {
  await assertProtectedMain(mainSha);
  const initial = await listDispatchRuns();
  const existing = findBoundRuns(initial, request, mainSha);
  if (existing.length > 1) fail(`duplicate canonical runs already exist for ${request.requestId}`);
  if (existing.length === 1) return { run: existing[0], reused: true };
  const before = new Set(initial.map((run) => run.id));
  await github(`/repos/${REPOSITORY}/actions/workflows/${WORKFLOW}/dispatches`, {
    method: 'POST',
    body: {
      ref: 'main',
      inputs: {
        operation: request.operation,
        release_set_id: request.releaseSetId,
        expected_current_release_set_id: request.expectedCurrent,
        source_run_id: request.sourceRunId,
        request_id: request.requestId,
        confirmation: request.confirmation,
      },
    },
  });
  for (let attempt = 0; attempt < 30; attempt += 1) {
    const runs = await listDispatchRuns();
    const fresh = findBoundRuns(runs.filter((run) => !before.has(run.id)), request, mainSha);
    if (fresh.length > 1) fail(`dispatch produced duplicate canonical runs for ${request.requestId}`);
    if (fresh.length === 1) return { run: fresh[0], reused: false };
    await sleep(2000);
  }
  fail(`dispatch accepted but resulting canonical run could not be bound for ${request.requestId}`);
}

async function waitForSuccess(runId, request, mainSha) {
  for (let attempt = 0; attempt < 300; attempt += 1) {
    const run = await github(`/repos/${REPOSITORY}/actions/runs/${runId}`);
    if (run?.id !== runId || run?.event !== 'workflow_dispatch' || run?.head_branch !== 'main' || run?.head_sha !== mainSha || run?.path !== WORKFLOW_PATH || run?.display_title !== expectedRunTitle(request)) fail(`canonical run ${runId} identity drifted`);
    if (run.status === 'completed') {
      if (run.conclusion !== 'success') fail(`canonical run ${runId} completed ${run.conclusion ?? 'without conclusion'}`);
      return run;
    }
    if (!['queued', 'in_progress', 'waiting', 'pending', 'requested'].includes(run.status)) fail(`canonical run ${runId} has unexpected status ${run.status}`);
    await sleep(10000);
  }
  fail(`canonical run ${runId} did not complete within bounded observation window`);
}

function auditMarker(ceremonyId, stage, phase) { return `<!-- AR11_FC6_TRUSTED_MAIN_AUDIT ceremony=${ceremonyId} stage=${stage} phase=${phase} -->`; }
function auditBody(ceremonyId, stage, phase, request, extra = []) {
  return [
    auditMarker(ceremonyId, stage, phase),
    `FC-6 trusted-main operator stage: **${stage} / ${phase}**.`,
    `Ceremony: \`${ceremonyId}\``,
    `Operation: \`${request.operation}\``,
    `Target Release Set: \`${request.releaseSetId}\``,
    `Expected current: \`${request.expectedCurrent}\``,
    ...extra,
    'This transport has repository Actions/issues authority only. It has no staging/provider environment and performs no provider mutation; mutation remains exclusively in Release Set Promotion.',
  ].join('\n\n');
}

async function loadAuditComments() {
  const comments = [];
  for (let page = 1; page <= 20; page += 1) {
    const value = await github(`/repos/${REPOSITORY}/issues/${ISSUE_NUMBER}/comments?per_page=100&page=${page}`);
    if (!Array.isArray(value)) fail('operator audit comment list response is malformed');
    comments.push(...value);
    if (value.length < 100) return comments;
  }
  fail('operator audit comment history exceeds bounded scan');
}

async function ensureAudit(comments, ceremonyId, stage, phase, request, extra = []) {
  const marker = auditMarker(ceremonyId, stage, phase);
  const matches = comments.filter((comment) => typeof comment?.body === 'string' && comment.body.includes(marker));
  if (matches.length > 1) fail(`duplicate audit marker for ${stage}/${phase}`);
  if (matches.length === 1) return matches[0];
  const created = await github(`/repos/${REPOSITORY}/issues/${ISSUE_NUMBER}/comments`, { method: 'POST', body: { body: auditBody(ceremonyId, stage, phase, request, extra) } });
  if (!Number.isInteger(created?.id) || created.id <= 0) fail('created operator audit comment id is invalid');
  comments.push(created);
  return created;
}

async function executeStage({ comments, ceremonyId, stage, request, mainSha }) {
  await ensureAudit(comments, ceremonyId, stage, 'DISPATCH_PENDING', request);
  const { run, reused } = await dispatchOrReuse(request, mainSha);
  await ensureAudit(comments, ceremonyId, stage, 'DISPATCH_BOUND', request, [`Canonical run id: \`${run.id}\``, `Canonical run: ${run.html_url}`, `Reused existing canonical run: \`${reused}\``]);
  const completed = await waitForSuccess(run.id, request, mainSha);
  await ensureAudit(comments, ceremonyId, stage, 'RUN_SUCCESS', request, [`Canonical run id: \`${completed.id}\``, `Canonical run: ${completed.html_url}`]);
  return completed;
}

function expectReject(fn, marker) {
  try { fn(); fail(`negative fixture unexpectedly passed: ${marker}`); }
  catch (error) { if (!String(error).includes(marker)) throw error; }
}

function selfTest() {
  const a = `release-set-v2-sha256-${'a'.repeat(64)}`;
  const b = `release-set-v2-sha256-${'b'.repeat(64)}`;
  const tx = { schema_version: 1, kind: 'AR11_FC6_STAGING_CEREMONY', authority: 'TRANSPORT_REQUEST_ONLY', tracker_issue: ISSUE_NUMBER, production_authorized: false, operation: 'full-staging-ceremony', release_set_a: a, release_set_b: b, initial_expected_current: a, final_expected_current: a, confirmation: `${a}:${b}:FC6` };
  const parsed = parseTransactionBytes(canonicalJson(tx));
  if (parsed.a !== a || parsed.b !== b) fail('positive transaction self-test failed');
  expectReject(() => parseTransactionBytes(JSON.stringify(tx)), 'canonical JSON bytes');
  expectReject(() => validateTransaction({ ...tx, production_authorized: true }), 'production unauthorized');
  expectReject(() => validateTransaction({ ...tx, release_set_b: a, confirmation: `${a}:${a}:FC6` }), 'source-distinct');
  expectReject(() => validateTransaction({ ...tx, initial_expected_current: b }), 'initial expected-current');
  const event = { repository: { full_name: REPOSITORY }, ref: 'refs/heads/main', created: false, deleted: false, forced: false, before: '1'.repeat(40), after: '2'.repeat(40), head_commit: { id: '2'.repeat(40) } };
  const push = validatePushEvent(event, '2'.repeat(40), 'refs/heads/main', 'push');
  if (push.before !== '1'.repeat(40) || push.after !== '2'.repeat(40)) fail('positive push envelope self-test failed');
  expectReject(() => validatePushEvent({ ...event, forced: true }, '2'.repeat(40), 'refs/heads/main', 'push'), 'ordinary protected-main');
  const compare = { url: `https://api.github.com/repos/${REPOSITORY}/compare/${'1'.repeat(40)}...${'2'.repeat(40)}`, status: 'ahead', ahead_by: 1, behind_by: 0, total_commits: 1, merge_base_commit: { sha: '1'.repeat(40) }, base_commit: { sha: '1'.repeat(40) }, commits: [{ sha: '2'.repeat(40) }], files: [{ filename: TRANSACTION_PATH, status: 'modified', deletions: 0 }] };
  validateCompare(compare, '1'.repeat(40), '2'.repeat(40));
  expectReject(() => validateCompare({ ...compare, files: [...compare.files, { filename: 'src/unsafe.rs', status: 'modified', deletions: 0 }] }, '1'.repeat(40), '2'.repeat(40)), 'exactly one data-only file');
  const ceremonyId = `main-${'2'.repeat(40)}`;
  const aToB = stageRequest('a-to-b', a, b, ceremonyId);
  if (aToB.releaseSetId !== b || aToB.expectedCurrent !== a || aToB.confirmation !== `${b}:${a}`) fail('A-to-B stage binding self-test failed');
  const aNoChange = stageRequest('a-no-change', a, b, ceremonyId);
  const negative = stageRequest('negative-rollback', a, b, ceremonyId, '123456');
  if (aNoChange.releaseSetId !== a || negative.sourceRunId !== '123456' || negative.confirmation !== `${a}:123456`) fail('A-no-change/negative stage binding self-test failed');
  expectReject(() => stageRequest('negative-rollback', a, b, ceremonyId, ''), 'completed A-to-A');
  const request = stageRequest('b-no-change', a, b, ceremonyId);
  const runs = [{ id: 77, event: 'workflow_dispatch', head_branch: 'main', head_sha: '2'.repeat(40), path: WORKFLOW_PATH, display_title: expectedRunTitle(request) }];
  if (findBoundRuns(runs, request, '2'.repeat(40)).length !== 1) fail('idempotent canonical run binding self-test failed');
  if (findBoundRuns(runs, request, '3'.repeat(40)).length !== 0) fail('wrong-main canonical run unexpectedly matched');
  console.log('AR-11 FC-6 trusted-main ceremony transport positive and negative self-tests passed.');
}

async function main() {
  if (process.argv.includes('--self-test')) { selfTest(); return; }
  if (process.env.GITHUB_WORKFLOW !== TRANSPORT_WORKFLOW) fail(`operator must execute only inside ${TRANSPORT_WORKFLOW}`);
  const eventPath = process.env.GITHUB_EVENT_PATH;
  if (!eventPath) fail('GITHUB_EVENT_PATH is unavailable');
  const event = JSON.parse(readFileSync(eventPath, 'utf8'));
  const push = validatePushEvent(event, process.env.GITHUB_SHA, process.env.GITHUB_REF, process.env.GITHUB_EVENT_NAME);
  const raw = readFileSync(TRANSACTION_PATH, 'utf8');
  const { a, b } = parseTransactionBytes(raw);
  await bindHostedTransaction(push.before, push.after, raw);

  const ceremonyId = `main-${push.after}`;
  const comments = await loadAuditComments();
  const completed = {};
  for (const stage of ['a-to-b', 'b-no-change', 'b-to-a', 'a-no-change']) {
    const request = stageRequest(stage, a, b, ceremonyId);
    completed[stage] = await executeStage({ comments, ceremonyId, stage, request, mainSha: push.after });
  }
  const negativeRequest = stageRequest('negative-rollback', a, b, ceremonyId, String(completed['a-no-change'].id));
  completed['negative-rollback'] = await executeStage({ comments, ceremonyId, stage: 'negative-rollback', request: negativeRequest, mainSha: push.after });
  const runIds = STAGES.map((stage) => `${stage}=${completed[stage].id}`).join(', ');
  await ensureAudit(comments, ceremonyId, 'ceremony', 'COMPLETE', negativeRequest, [`Canonical run ids: \`${runIds}\``, `Final expected provider Release Set after ceremony: \`${a}\``]);
  console.log(JSON.stringify({ ceremony_id: ceremonyId, release_set_a: a, release_set_b: b, run_ids: Object.fromEntries(STAGES.map((stage) => [stage, completed[stage].id])), production_authorized: false }));
}

main().catch(async (error) => {
  const message = error instanceof Error ? error.message : String(error);
  console.error(message);
  process.exitCode = 1;
});
