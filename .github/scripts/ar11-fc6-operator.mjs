#!/usr/bin/env node

import { readFileSync } from 'node:fs';
import process from 'node:process';

const REPOSITORY = 'iamaman11/part-crm-emai-profile';
const ISSUE_NUMBER = 399;
const WORKFLOW = 'release-set-promotion.yml';
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
const PROMOTE = new RegExp(`^/ar11-fc6-promote\\s+(${RELEASE})\\s+(${RELEASE}|NONE)\\s+(\\S+)$`);
const NEGATIVE = new RegExp(`^/ar11-fc6-negative\\s+(${RELEASE})\\s+([1-9][0-9]*)\\s+(\\S+)$`);
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

function fail(message) {
  throw new Error(`AR-11 FC-6 operator request rejected: ${message}`);
}

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

function parseCommand(body, requestId) {
  const text = typeof body === 'string' ? body.trim() : '';
  let match = text.match(PROMOTE);
  if (match) {
    const [, releaseSetId, expectedCurrent, confirmation] = match;
    return requestFromFields({ operation: 'promote', releaseSetId, expectedCurrent, sourceRunId: '', confirmation }, requestId);
  }
  match = text.match(NEGATIVE);
  if (match) {
    const [, releaseSetId, sourceRunId, confirmation] = match;
    return requestFromFields({ operation: 'rollback-negative', releaseSetId, expectedCurrent: releaseSetId, sourceRunId, confirmation }, requestId);
  }
  fail('command syntax is not exact');
}

function parseIssueComment(event) {
  if (event.action !== 'created') fail('only newly created issue comments are accepted');
  if (event.issue?.number !== ISSUE_NUMBER || event.issue?.pull_request) fail(`request must be an issue comment on #${ISSUE_NUMBER}`);
  if (event.comment?.user?.login !== OWNER || event.comment?.author_association !== 'OWNER') fail('requester must be the repository owner with OWNER association');
  if (!Number.isInteger(event.comment?.id) || event.comment.id <= 0) fail('comment id is missing');
  const body = typeof event.comment?.body === 'string' ? event.comment.body.trim() : '';
  if (!body.startsWith('/ar11-fc6-')) return null;
  return { request: parseCommand(body, String(event.comment.id)), responseIssue: ISSUE_NUMBER, workflowRun: null };
}

function parseWorkflowRun(event) {
  const run = event.workflow_run;
  const headBranch = run?.head_branch;
  const looksLikeOperator = typeof headBranch === 'string' && headBranch.startsWith(PR_BRANCH_PREFIX);
  if (!looksLikeOperator) return null;
  if (event.action !== 'completed') fail('operator workflow_run is accepted only after completion');
  if (run?.name !== TRIGGER_WORKFLOW_NAME || run?.path !== TRIGGER_WORKFLOW_PATH || run?.event !== 'pull_request') fail('operator workflow_run must be the canonical pull-request Release Architecture Gate');
  if (run?.status !== 'completed' || run?.conclusion !== 'success') fail('operator trigger gate must complete successfully');
  if (run?.head_repository?.full_name !== REPOSITORY || !SHA.test(run?.head_sha ?? '')) fail('operator trigger gate must use an exact same-repository head SHA');
  if (!Number.isInteger(run?.id) || run.id <= 0) fail('operator workflow_run id is invalid');
  const pulls = run?.pull_requests;
  if (!Array.isArray(pulls) || pulls.length !== 1 || !Number.isInteger(pulls[0]?.number) || pulls[0].number <= 0) fail('operator workflow_run must bind exactly one pull request');
  return {
    request: null,
    responseIssue: pulls[0].number,
    workflowRun: { id: run.id, headSha: run.head_sha, headBranch, pullNumber: pulls[0].number },
  };
}

function parseRequest(event) {
  if (!event || typeof event !== 'object' || Array.isArray(event)) fail('event must be one object');
  if (event.repository?.full_name !== REPOSITORY) fail(`repository must be ${REPOSITORY}`);
  if (event.workflow_run) return parseWorkflowRun(event);
  if (event.issue) return parseIssueComment(event);
  return null;
}

function canonicalTriggerBytes(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function validateTriggerDocument(value, requestId) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) fail('operator PR trigger document must be one object');
  const actualKeys = Object.keys(value);
  if (JSON.stringify(actualKeys) !== JSON.stringify(TRIGGER_KEYS)) fail('operator PR trigger document keys/order drifted');
  if (
    value.schema_version !== 1
    || value.kind !== 'AR11_FC6_EPHEMERAL_OPERATOR_TRIGGER'
    || value.authority !== 'NONE'
    || value.merge_authorized !== false
    || value.tracker_issue !== ISSUE_NUMBER
  ) fail('operator PR trigger document non-authority envelope drifted');
  return requestFromFields({
    operation: value.operation,
    releaseSetId: value.release_set_id,
    expectedCurrent: value.expected_current_release_set_id,
    sourceRunId: value.source_run_id,
    confirmation: value.confirmation,
  }, requestId);
}

function parseTriggerBytes(raw, requestId) {
  let trigger;
  try {
    trigger = JSON.parse(raw);
  } catch (error) {
    fail(`operator PR trigger document is invalid JSON: ${error instanceof Error ? error.message : error}`);
  }
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

function findBoundRun(runs, expectedTitle) {
  if (!Array.isArray(runs)) fail('workflow run list response is malformed');
  return runs.find((run) =>
    run?.event === 'workflow_dispatch'
    && run?.head_branch === 'main'
    && run?.display_title === expectedTitle
    && Number.isInteger(run?.id)
    && run.id > 0
  ) ?? null;
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
      'User-Agent': 'ar11-fc6-canonical-operator',
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

async function resolveWorkflowRunRequest(workflowRun) {
  if (!workflowRun) return null;
  const pull = await github(`/repos/${REPOSITORY}/pulls/${workflowRun.pullNumber}`);
  if (pull?.number !== workflowRun.pullNumber || pull?.state !== 'open') fail('operator PR must still be open');
  if (pull?.title !== PR_TITLE || pull?.head?.ref !== workflowRun.headBranch) fail('operator PR exact title or branch binding drifted');
  if (pull?.user?.login !== OWNER || pull?.author_association !== 'OWNER') fail('operator PR requester must be repository owner with OWNER association');
  if (pull?.draft !== true) fail('operator PR must remain Draft');
  if (pull?.base?.ref !== 'main' || !SHA.test(pull?.base?.sha ?? '')) fail('operator PR must target exact protected main');
  if (pull?.head?.repo?.full_name !== REPOSITORY || pull?.head?.sha !== workflowRun.headSha) fail('operator PR exact same-repository head binding drifted');

  const main = await github(`/repos/${REPOSITORY}/branches/main`);
  if (main?.protected !== true || main?.commit?.sha !== pull.base.sha) fail('operator PR base must equal the currently protected main SHA');

  const files = await github(`/repos/${REPOSITORY}/pulls/${workflowRun.pullNumber}/files?per_page=100`);
  const triggerSha = validatePullFiles(files);
  const encodedRef = encodeURIComponent(workflowRun.headSha);
  const payload = await github(`/repos/${REPOSITORY}/contents/${PR_TRIGGER_PATH}?ref=${encodedRef}`);
  if (payload?.type !== 'file' || payload?.encoding !== 'base64' || payload?.sha !== triggerSha || typeof payload?.content !== 'string') fail('operator PR trigger file response is malformed or unbound');
  const raw = Buffer.from(payload.content.replace(/\s/g, ''), 'base64').toString('utf8');
  return parseTriggerBytes(raw, `pr-${workflowRun.pullNumber}`);
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
    const match = findBoundRun(runs.filter((run) => !before.has(run.id)), expectedTitle);
    if (match) return { run: match, reused: false };
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
  fail(`dispatched canonical workflow but could not bind the resulting run to ${expectedTitle}`);
}

async function commentRun(responseIssue, request, run) {
  const body = [
    `FC-6 canonical ${request.operation} request accepted from bounded operator request ${request.requestId}.`,
    `Run: ${run.html_url}`,
    `Run id: ${run.id}`,
    `Target Release Set: \`${request.releaseSetId}\``,
    `Expected current: \`${request.expectedCurrent}\``,
    'The operator adapter has no provider credentials and performs no provider mutation; execution remains inside the canonical Release Set Promotion workflow.',
  ].join('\n\n');
  await github(`/repos/${REPOSITORY}/issues/${responseIssue}/comments`, { method: 'POST', body: { body } });
}

function expectReject(fn, marker) {
  try {
    fn();
    fail(`negative fixture unexpectedly passed: ${marker}`);
  } catch (error) {
    if (!String(error).includes(marker)) throw error;
  }
}

function selfTest() {
  const a = `release-set-v2-sha256-${'a'.repeat(64)}`;
  const b = `release-set-v2-sha256-${'b'.repeat(64)}`;
  const baseIssue = {
    action: 'created', repository: { full_name: REPOSITORY }, issue: { number: ISSUE_NUMBER },
    comment: { id: 12345, user: { login: OWNER }, author_association: 'OWNER', body: '' },
  };
  baseIssue.comment.body = `/ar11-fc6-promote ${b} ${a} ${b}:${a}`;
  const promote = parseRequest(baseIssue)?.request;
  if (promote?.operation !== 'promote' || promote.releaseSetId !== b || promote.expectedCurrent !== a) fail('positive promotion parse self-test failed');
  baseIssue.comment.body = `/ar11-fc6-negative ${a} 987654 ${a}:987654`;
  const negative = parseRequest(baseIssue)?.request;
  if (negative?.operation !== 'rollback-negative' || negative.sourceRunId !== '987654') fail('positive negative-evidence parse self-test failed');
  baseIssue.comment.body = 'ordinary tracker comment';
  if (parseRequest(baseIssue) !== null) fail('ordinary tracker comment unexpectedly became operator input');

  const wrongOwner = structuredClone(baseIssue);
  wrongOwner.comment.body = `/ar11-fc6-promote ${b} ${a} ${b}:${a}`;
  wrongOwner.comment.user.login = 'attacker';
  expectReject(() => parseRequest(wrongOwner), 'requester must be');

  const workflowRun = {
    action: 'completed', repository: { full_name: REPOSITORY },
    workflow_run: {
      id: 777, name: TRIGGER_WORKFLOW_NAME, path: TRIGGER_WORKFLOW_PATH, event: 'pull_request',
      status: 'completed', conclusion: 'success', head_branch: `${PR_BRANCH_PREFIX}a-to-b`, head_sha: '2'.repeat(40),
      head_repository: { full_name: REPOSITORY }, pull_requests: [{ number: 77 }],
    },
  };
  const parsedRun = parseRequest(workflowRun);
  if (parsedRun?.workflowRun?.pullNumber !== 77 || parsedRun.responseIssue !== 77) fail('positive workflow_run envelope self-test failed');
  const ordinaryRun = structuredClone(workflowRun);
  ordinaryRun.workflow_run.head_branch = 'feature/ordinary';
  if (parseRequest(ordinaryRun) !== null) fail('ordinary workflow_run unexpectedly became operator input');
  const failedRun = structuredClone(workflowRun);
  failedRun.workflow_run.conclusion = 'failure';
  expectReject(() => parseRequest(failedRun), 'complete successfully');

  const promoteTrigger = {
    schema_version: 1,
    kind: 'AR11_FC6_EPHEMERAL_OPERATOR_TRIGGER',
    authority: 'NONE',
    merge_authorized: false,
    tracker_issue: ISSUE_NUMBER,
    operation: 'promote',
    release_set_id: b,
    expected_current_release_set_id: a,
    source_run_id: '',
    confirmation: `${b}:${a}`,
  };
  const canonical = canonicalTriggerBytes(promoteTrigger);
  const fileRequest = parseTriggerBytes(canonical, 'pr-77');
  if (fileRequest.operation !== 'promote' || fileRequest.releaseSetId !== b || fileRequest.requestId !== 'pr-77') fail('immutable PR trigger parse self-test failed');
  expectReject(() => parseTriggerBytes(JSON.stringify(promoteTrigger), 'pr-77'), 'canonical JSON bytes');
  expectReject(() => validateTriggerDocument({ ...promoteTrigger, merge_authorized: true }, 'pr-77'), 'non-authority envelope drifted');
  expectReject(() => validateTriggerDocument({ ...promoteTrigger, confirmation: 'BAD' }, 'pr-77'), 'confirmation');
  const reordered = { kind: promoteTrigger.kind, schema_version: 1, ...Object.fromEntries(Object.entries(promoteTrigger).filter(([key]) => !['kind', 'schema_version'].includes(key))) };
  expectReject(() => validateTriggerDocument(reordered, 'pr-77'), 'keys/order drifted');
  if (resolveWorkflowRunRequest.toString().includes('pull.body')) fail('mutable PR body unexpectedly became operator authority');
  if (!resolveWorkflowRunRequest.toString().includes('parseTriggerBytes')) fail('immutable exact-head trigger parser is not bound into workflow-run resolution');

  validatePullFiles([{ filename: PR_TRIGGER_PATH, status: 'added', deletions: 0, sha: '3'.repeat(40) }]);
  expectReject(() => validatePullFiles([
    { filename: PR_TRIGGER_PATH, status: 'added', deletions: 0, sha: '3'.repeat(40) },
    { filename: 'src/untrusted.rs', status: 'added', deletions: 0, sha: '4'.repeat(40) },
  ]), 'exactly one data-only file');

  const boundRuns = [
    { id: 101, event: 'workflow_dispatch', head_branch: 'main', display_title: 'AR11 promote pr-77' },
    { id: 102, event: 'pull_request', head_branch: 'main', display_title: 'AR11 promote pr-77' },
  ];
  if (findBoundRun(boundRuns, 'AR11 promote pr-77')?.id !== 101) fail('idempotent request-to-run binding self-test failed');
  if (findBoundRun(boundRuns, 'AR11 promote pr-78') !== null) fail('unrelated request unexpectedly reused an existing canonical run');

  console.log('AR-11 FC-6 immutable-head operator positive and negative self-tests passed.');
}

async function main() {
  if (process.argv.includes('--self-test')) { selfTest(); return; }
  const eventPath = process.env.GITHUB_EVENT_PATH;
  if (!eventPath) fail('GITHUB_EVENT_PATH is unavailable');
  const event = JSON.parse(readFileSync(eventPath, 'utf8'));
  const parsed = parseRequest(event);
  if (parsed === null) return;
  const request = parsed.request ?? await resolveWorkflowRunRequest(parsed.workflowRun);
  if (!request) fail('operator request could not be resolved');
  const { run, reused } = await dispatch(request);
  if (!reused) await commentRun(parsed.responseIssue, request, run);
  console.log(JSON.stringify({ operation: request.operation, request_id: request.requestId, run_id: run.id, run_url: run.html_url, reused_existing_run: reused }));
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});