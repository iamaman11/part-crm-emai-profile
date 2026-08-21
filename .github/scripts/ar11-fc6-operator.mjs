#!/usr/bin/env node

import { readFileSync } from 'node:fs';
import process from 'node:process';

const REPOSITORY = 'iamaman11/part-crm-emai-profile';
const ISSUE_NUMBER = 399;
const WORKFLOW = 'release-set-promotion.yml';
const TRANSPORT_WORKFLOW = 'AR-11 FC-6 Operator Transport';
const OWNER = 'iamaman11';
const PR_TITLE = 'AR-11 FC-6 ephemeral operator request';
const PR_BRANCH_PREFIX = 'codex/ar11-fc6-request-';
const PR_TRIGGER_PATH = 'docs/evidence/ar11-fc6-operator-request.json';
const TRIGGER_WORKFLOW_NAME = 'Release Architecture Gate';
const TRIGGER_WORKFLOW_PATH = '.github/workflows/release-architecture-gate.yml';
const RELEASE = 'release-set-v2-sha256-[0-9a-f]{64}';
const RELEASE_ID = new RegExp(`^${RELEASE}$`);
const SHA = /^[0-9a-f]{40}$/;
const RUN_ID = /^[1-9][0-9]*$/;
const TRIGGER_KEYS = Object.freeze([
  'schema_version',
  'kind',
  'authority',
  'merge_authorized',
  'tracker_issue',
  'operation',
  'release_set_id',
  'expected_current_release_set_id',
  'source_run_id',
  'confirmation',
]);

function fail(message) { throw new Error(`AR-11 FC-6 operator request rejected: ${message}`); }

function requestFromFields({ operation, releaseSetId, expectedCurrent, sourceRunId, confirmation }, requestId) {
  if (operation === 'promote') {
    if (!RELEASE_ID.test(releaseSetId ?? '')) fail('promotion Release Set id is invalid');
    if (expectedCurrent !== 'NONE' && !RELEASE_ID.test(expectedCurrent ?? '')) fail('promotion expected-current Release Set id is invalid');
    if (sourceRunId !== '') fail('promotion source_run_id must be empty');
    if (confirmation !== `${releaseSetId}:${expectedCurrent}`) fail('promotion confirmation does not bind target and expected current');
    return { operation, requestId, releaseSetId, expectedCurrent, sourceRunId: '', confirmation };
  }
  if (operation === 'rollback-negative') {
    if (!RELEASE_ID.test(releaseSetId ?? '')) fail('negative-evidence Release Set id is invalid');
    if (expectedCurrent !== releaseSetId) fail('negative-evidence expected-current must equal Release Set id');
    if (!RUN_ID.test(sourceRunId ?? '')) fail('negative-evidence source_run_id is invalid');
    if (confirmation !== `${releaseSetId}:${sourceRunId}`) fail('negative confirmation does not bind Release Set and source run');
    return { operation, requestId, releaseSetId, expectedCurrent, sourceRunId, confirmation };
  }
  fail('operation must be promote or rollback-negative');
}

function parseWorkflowRun(event) {
  const run = event.workflow_run;
  if (!run) return null;
  const headBranch = run.head_branch;
  const looksLikeOperator = typeof headBranch === 'string' && headBranch.startsWith(PR_BRANCH_PREFIX);
  if (!looksLikeOperator) return null;
  if (event.action !== 'completed') fail('operator workflow_run is accepted only after completion');
  if (run.name !== TRIGGER_WORKFLOW_NAME || run.path !== TRIGGER_WORKFLOW_PATH || run.event !== 'pull_request') fail('operator workflow_run must be the canonical pull-request Release Architecture Gate');
  if (run.status !== 'completed' || run.conclusion !== 'success') fail('operator trigger gate must complete successfully');
  if (run.head_repository?.full_name !== REPOSITORY || !SHA.test(run.head_sha ?? '')) fail('operator trigger gate must use an exact same-repository head SHA');
  if (!Number.isInteger(run.id) || run.id <= 0) fail('operator workflow_run id is invalid');
  if (!Array.isArray(run.pull_requests)) fail('operator workflow_run pull request association must be an array');
  if (run.pull_requests.length > 1) fail('operator workflow_run must not bind multiple pull requests');
  let pullNumber = null;
  if (run.pull_requests.length === 1) {
    pullNumber = run.pull_requests[0]?.number;
    if (!Number.isInteger(pullNumber) || pullNumber <= 0) fail('operator workflow_run pull request number is invalid');
  }
  return { id: run.id, headSha: run.head_sha, headBranch, pullNumber };
}

function parseRequest(event) {
  if (!event || typeof event !== 'object' || Array.isArray(event)) fail('event must be one object');
  if (event.repository?.full_name !== REPOSITORY) fail(`repository must be ${REPOSITORY}`);
  if (event.workflow_run) return parseWorkflowRun(event);
  return null;
}

function canonicalTriggerBytes(value) { return `${JSON.stringify(value, null, 2)}\n`; }

function validateTriggerDocument(value, requestId) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) fail('operator PR trigger document must be one object');
  const actualKeys = Object.keys(value);
  if (JSON.stringify(actualKeys) !== JSON.stringify(TRIGGER_KEYS)) fail('operator PR trigger document keys/order drifted');
  if (value.schema_version !== 1 || value.kind !== 'AR11_FC6_EPHEMERAL_OPERATOR_TRIGGER' || value.authority !== 'NONE' || value.merge_authorized !== false || value.tracker_issue !== ISSUE_NUMBER) fail('operator PR trigger document non-authority envelope drifted');
  return requestFromFields({ operation: value.operation, releaseSetId: value.release_set_id, expectedCurrent: value.expected_current_release_set_id, sourceRunId: value.source_run_id, confirmation: value.confirmation }, requestId);
}

function parseTriggerBytes(raw, requestId) {
  let trigger;
  try { trigger = JSON.parse(raw); } catch (error) { fail(`operator PR trigger document is invalid JSON: ${error instanceof Error ? error.message : error}`); }
  if (raw !== canonicalTriggerBytes(trigger)) fail('operator PR trigger document must be canonical JSON bytes');
  return validateTriggerDocument(trigger, requestId);
}

function validatePullFiles(files) {
  if (!Array.isArray(files) || files.length !== 1) fail('operator PR must change exactly one data-only file');
  const file = files[0];
  if (file?.filename !== PR_TRIGGER_PATH || file?.status !== 'added' || file?.deletions !== 0) fail(`operator PR may only add ${PR_TRIGGER_PATH}`);
  if (typeof file?.sha !== 'string' || !SHA.test(file.sha)) fail('operator PR trigger blob SHA is invalid');
  return file.sha;
}

function selectFallbackPullNumber(pulls, workflowRun) {
  if (!Array.isArray(pulls)) fail('fallback pull request query response must be an array');
  const exact = pulls.filter((pull) => pull?.state === 'open' && pull?.base?.ref === 'main' && pull?.head?.ref === workflowRun.headBranch && pull?.head?.sha === workflowRun.headSha && pull?.head?.repo?.full_name === REPOSITORY);
  if (exact.length !== 1) fail(`workflow_run fallback must resolve exactly one open exact-head pull request; observed ${exact.length}`);
  const number = exact[0]?.number;
  if (!Number.isInteger(number) || number <= 0) fail('fallback pull request number is invalid');
  return number;
}

function findBoundRun(runs, expectedTitle) {
  if (!Array.isArray(runs)) fail('workflow run list response is malformed');
  return runs.find((run) => run?.event === 'workflow_dispatch' && run?.head_branch === 'main' && run?.path === '.github/workflows/release-set-promotion.yml' && run?.display_title === expectedTitle && Number.isInteger(run?.id) && run.id > 0) ?? null;
}

async function github(path, { method = 'GET', body } = {}) {
  const token = process.env.GITHUB_TOKEN;
  if (!token) fail('ephemeral github.token is unavailable');
  const response = await fetch(`https://api.github.com${path}`, {
    method,
    headers: { Accept: 'application/vnd.github+json', Authorization: `Bearer ${token}`, 'Content-Type': 'application/json', 'User-Agent': 'ar11-fc6-canonical-operator', 'X-GitHub-Api-Version': '2022-11-28' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!response.ok) {
    const text = (await response.text()).slice(0, 1200);
    fail(`GitHub API ${method} ${path} failed HTTP ${response.status}: ${text}`);
  }
  if (response.status === 204) return null;
  return response.json();
}

async function resolvePullNumber(workflowRun) {
  if (Number.isInteger(workflowRun.pullNumber) && workflowRun.pullNumber > 0) return workflowRun.pullNumber;
  const head = encodeURIComponent(`${OWNER}:${workflowRun.headBranch}`);
  const pulls = await github(`/repos/${REPOSITORY}/pulls?state=open&base=main&head=${head}&per_page=100`);
  return selectFallbackPullNumber(pulls, workflowRun);
}

async function resolveWorkflowRunRequest(workflowRun) {
  if (!workflowRun) return null;
  const pullNumber = await resolvePullNumber(workflowRun);
  const pull = await github(`/repos/${REPOSITORY}/pulls/${pullNumber}`);
  if (pull?.number !== pullNumber || pull?.state !== 'open') fail('operator PR must still be open');
  if (pull?.title !== PR_TITLE || pull?.head?.ref !== workflowRun.headBranch) fail('operator PR exact title or branch binding drifted');
  if (pull?.user?.login !== OWNER || pull?.author_association !== 'OWNER') fail('operator PR requester must be repository owner with OWNER association');
  if (pull?.draft !== true) fail('operator PR must remain Draft');
  if (pull?.changed_files !== 1) fail('operator PR must report exactly one changed file');
  if (pull?.base?.ref !== 'main' || !SHA.test(pull?.base?.sha ?? '')) fail('operator PR must target exact protected main');
  if (pull?.head?.repo?.full_name !== REPOSITORY || pull?.head?.sha !== workflowRun.headSha) fail('operator PR exact same-repository head binding drifted');

  const main = await github(`/repos/${REPOSITORY}/branches/main`);
  if (main?.protected !== true || main?.commit?.sha !== pull.base.sha) fail('operator PR base must equal the currently protected main SHA');

  const files = await github(`/repos/${REPOSITORY}/pulls/${pullNumber}/files?per_page=100`);
  const triggerSha = validatePullFiles(files);
  const encodedRef = encodeURIComponent(workflowRun.headSha);
  const payload = await github(`/repos/${REPOSITORY}/contents/${PR_TRIGGER_PATH}?ref=${encodedRef}`);
  if (payload?.type !== 'file' || payload?.encoding !== 'base64' || payload?.sha !== triggerSha || typeof payload?.content !== 'string') fail('operator PR trigger file response is malformed or unbound');
  const raw = Buffer.from(payload.content.replace(/\s/g, ''), 'base64').toString('utf8');
  const request = parseTriggerBytes(raw, `pr-${pullNumber}`);
  return { request, responseIssue: pullNumber };
}

async function listDispatchRuns() {
  const runs = [];
  for (let page = 1; page <= 100; page += 1) {
    const value = await github(`/repos/${REPOSITORY}/actions/workflows/${WORKFLOW}/runs?event=workflow_dispatch&branch=main&per_page=100&page=${page}`);
    if (!Array.isArray(value?.workflow_runs)) fail('workflow run list response is malformed');
    runs.push(...value.workflow_runs);
    if (value.workflow_runs.length < 100) return runs;
  }
  fail('canonical dispatch history exceeds bounded idempotency scan; refusing duplicate-risk dispatch');
}

async function dispatch(request) {
  const expectedTitle = `AR11 ${request.operation} ${request.requestId}`;
  const initialRuns = await listDispatchRuns();
  const existing = findBoundRun(initialRuns, expectedTitle);
  if (existing) return { run: existing, reused: true };
  const before = new Set(initialRuns.map((run) => run.id));
  await github(`/repos/${REPOSITORY}/actions/workflows/${WORKFLOW}/dispatches`, { method: 'POST', body: { ref: 'main', inputs: { operation: request.operation, release_set_id: request.releaseSetId, expected_current_release_set_id: request.expectedCurrent, source_run_id: request.sourceRunId, request_id: request.requestId, confirmation: request.confirmation } } });
  for (let attempt = 0; attempt < 30; attempt += 1) {
    const runs = await listDispatchRuns();
    const match = findBoundRun(runs.filter((run) => !before.has(run.id)), expectedTitle);
    if (match) return { run: match, reused: false };
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
  fail(`dispatched canonical workflow but could not bind the resulting run to ${expectedTitle}`);
}

function auditMarker(request) { return `<!-- AR11_FC6_OPERATOR_AUDIT request=${request.requestId} -->`; }

function auditBody(request, status, extra = []) {
  return [auditMarker(request), `FC-6 operator transport status: **${status}**.`, `Request: \`${request.requestId}\``, `Operation: \`${request.operation}\``, `Target Release Set: \`${request.releaseSetId}\``, `Expected current: \`${request.expectedCurrent}\``, ...extra, 'The transport has no provider credentials and performs no provider mutation. Provider mutation remains exclusively inside the canonical Release Set Promotion workflow.'].join('\n\n');
}

async function listIssueComments(issueNumber) {
  const comments = [];
  for (let page = 1; page <= 20; page += 1) {
    const value = await github(`/repos/${REPOSITORY}/issues/${issueNumber}/comments?per_page=100&page=${page}`);
    if (!Array.isArray(value)) fail('operator audit comment list response is malformed');
    comments.push(...value);
    if (value.length < 100) return comments;
  }
  fail('operator audit comment history exceeds bounded scan');
}

async function ensureAuditComment(issueNumber, request) {
  const marker = auditMarker(request);
  const matches = (await listIssueComments(issueNumber)).filter((comment) => typeof comment?.body === 'string' && comment.body.includes(marker));
  if (matches.length > 1) fail(`operator audit marker is duplicated for ${request.requestId}`);
  if (matches.length === 1) {
    if (!Number.isInteger(matches[0]?.id) || matches[0].id <= 0) fail('existing operator audit comment id is invalid');
    return matches[0].id;
  }
  const created = await github(`/repos/${REPOSITORY}/issues/${issueNumber}/comments`, { method: 'POST', body: { body: auditBody(request, 'VALIDATED / DISPATCH PENDING') } });
  if (!Number.isInteger(created?.id) || created.id <= 0) fail('created operator audit comment id is invalid');
  return created.id;
}

async function updateAuditComment(commentId, body) { await github(`/repos/${REPOSITORY}/issues/comments/${commentId}`, { method: 'PATCH', body: { body } }); }

function expectReject(fn, marker) {
  try { fn(); fail(`negative fixture unexpectedly passed: ${marker}`); } catch (error) { if (!String(error).includes(marker)) throw error; }
}

function selfTest() {
  const a = `release-set-v2-sha256-${'a'.repeat(64)}`;
  const b = `release-set-v2-sha256-${'b'.repeat(64)}`;
  const workflowRun = { action: 'completed', repository: { full_name: REPOSITORY }, workflow_run: { id: 777, name: TRIGGER_WORKFLOW_NAME, path: TRIGGER_WORKFLOW_PATH, event: 'pull_request', status: 'completed', conclusion: 'success', head_branch: `${PR_BRANCH_PREFIX}a-to-b`, head_sha: '2'.repeat(40), head_repository: { full_name: REPOSITORY }, pull_requests: [{ number: 77 }] } };
  const parsedRun = parseRequest(workflowRun);
  if (parsedRun?.pullNumber !== 77 || parsedRun.headSha !== '2'.repeat(40)) fail('positive workflow_run envelope self-test failed');
  const missingAssociation = structuredClone(workflowRun); missingAssociation.workflow_run.pull_requests = [];
  if (parseRequest(missingAssociation)?.pullNumber !== null) fail('missing workflow_run PR association must defer to exact-head fallback');
  const ambiguousAssociation = structuredClone(workflowRun); ambiguousAssociation.workflow_run.pull_requests = [{ number: 77 }, { number: 78 }];
  expectReject(() => parseRequest(ambiguousAssociation), 'must not bind multiple');
  const ordinaryRun = structuredClone(workflowRun); ordinaryRun.workflow_run.head_branch = 'feature/ordinary';
  if (parseRequest(ordinaryRun) !== null) fail('ordinary workflow_run unexpectedly became operator input');
  const failedRun = structuredClone(workflowRun); failedRun.workflow_run.conclusion = 'failure';
  expectReject(() => parseRequest(failedRun), 'complete successfully');
  const wrongWorkflow = structuredClone(workflowRun); wrongWorkflow.workflow_run.name = 'Quality Gate';
  expectReject(() => parseRequest(wrongWorkflow), 'canonical pull-request Release Architecture Gate');
  if (parseRequest({ repository: { full_name: REPOSITORY }, issue: { number: ISSUE_NUMBER } }) !== null) fail('issue comments must not be an operator authority');

  const trigger = { schema_version: 1, kind: 'AR11_FC6_EPHEMERAL_OPERATOR_TRIGGER', authority: 'NONE', merge_authorized: false, tracker_issue: ISSUE_NUMBER, operation: 'promote', release_set_id: b, expected_current_release_set_id: a, source_run_id: '', confirmation: `${b}:${a}` };
  const request = parseTriggerBytes(canonicalTriggerBytes(trigger), 'pr-77');
  if (request.operation !== 'promote' || request.releaseSetId !== b || request.expectedCurrent !== a) fail('positive canonical trigger self-test failed');
  expectReject(() => parseTriggerBytes(JSON.stringify(trigger), 'pr-77'), 'canonical JSON bytes');
  expectReject(() => validateTriggerDocument({ ...trigger, surprise: true }, 'pr-77'), 'keys/order drifted');
  const negativeTrigger = { ...trigger, operation: 'rollback-negative', release_set_id: a, expected_current_release_set_id: a, source_run_id: '987654', confirmation: `${a}:987654` };
  if (parseTriggerBytes(canonicalTriggerBytes(negativeTrigger), 'pr-78').sourceRunId !== '987654') fail('positive rollback-negative trigger self-test failed');

  validatePullFiles([{ filename: PR_TRIGGER_PATH, status: 'added', deletions: 0, sha: '3'.repeat(40) }]);
  expectReject(() => validatePullFiles([{ filename: PR_TRIGGER_PATH, status: 'added', deletions: 0, sha: '3'.repeat(40) }, { filename: 'src/untrusted.rs', status: 'added', deletions: 0, sha: '4'.repeat(40) }]), 'exactly one data-only file');
  const fallbackRun = { headBranch: `${PR_BRANCH_PREFIX}x`, headSha: '5'.repeat(40) };
  const fallbackRows = [{ number: 88, state: 'open', base: { ref: 'main' }, head: { ref: fallbackRun.headBranch, sha: fallbackRun.headSha, repo: { full_name: REPOSITORY } } }];
  if (selectFallbackPullNumber(fallbackRows, fallbackRun) !== 88) fail('exact-head fallback PR selection self-test failed');
  expectReject(() => selectFallbackPullNumber([], fallbackRun), 'exactly one open exact-head');
  expectReject(() => selectFallbackPullNumber([...fallbackRows, { ...fallbackRows[0], number: 89 }], fallbackRun), 'exactly one open exact-head');
  const boundRuns = [{ id: 101, event: 'workflow_dispatch', head_branch: 'main', path: '.github/workflows/release-set-promotion.yml', display_title: 'AR11 promote pr-77' }, { id: 102, event: 'pull_request', head_branch: 'main', path: '.github/workflows/release-set-promotion.yml', display_title: 'AR11 promote pr-77' }];
  if (findBoundRun(boundRuns, 'AR11 promote pr-77')?.id !== 101) fail('idempotent request-to-run binding self-test failed');
  if (findBoundRun(boundRuns, 'AR11 promote pr-78') !== null) fail('unrelated request unexpectedly reused an existing canonical run');
  const mutableBodyAuthority = ['pull', 'body'].join('.');
  if (resolveWorkflowRunRequest.toString().includes(mutableBodyAuthority)) fail('mutable PR body must never be operator authority');
  console.log('AR-11 FC-6 isolated immutable-head operator positive and negative self-tests passed.');
}

async function main() {
  if (process.argv.includes('--self-test')) { selfTest(); return; }
  if (process.env.GITHUB_WORKFLOW !== TRANSPORT_WORKFLOW) fail(`operator must execute only inside ${TRANSPORT_WORKFLOW}`);
  const eventPath = process.env.GITHUB_EVENT_PATH;
  if (!eventPath) fail('GITHUB_EVENT_PATH is unavailable');
  const event = JSON.parse(readFileSync(eventPath, 'utf8'));
  const workflowRun = parseRequest(event);
  if (workflowRun === null) return;
  let auditCommentId = null;
  let request = null;
  let responseIssue = null;
  try {
    const resolved = await resolveWorkflowRunRequest(workflowRun);
    if (!resolved?.request || !Number.isInteger(resolved?.responseIssue)) fail('operator request could not be resolved');
    request = resolved.request;
    responseIssue = resolved.responseIssue;
    auditCommentId = await ensureAuditComment(responseIssue, request);
    const { run, reused } = await dispatch(request);
    await updateAuditComment(auditCommentId, auditBody(request, 'ACCEPTED', [`Canonical run: ${run.html_url}`, `Run id: ${run.id}`, `Reused existing canonical run: \`${reused}\``]));
    console.log(JSON.stringify({ operation: request.operation, request_id: request.requestId, run_id: run.id, run_url: run.html_url, reused_existing_run: reused }));
  } catch (error) {
    if (auditCommentId && request && responseIssue) {
      const message = error instanceof Error ? error.message : String(error);
      try { await updateAuditComment(auditCommentId, auditBody(request, 'FAILED / NO NEW AUTHORITY', [`Failure: \`${message.slice(0, 600).replace(/`/g, "'")}\``])); }
      catch (commentError) { console.error(`failed to update operator audit comment after rejection: ${commentError instanceof Error ? commentError.message : commentError}`); }
    }
    throw error;
  }
}

main().catch((error) => { console.error(error instanceof Error ? error.message : error); process.exitCode = 1; });
