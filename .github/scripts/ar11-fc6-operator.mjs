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
const RELEASE = 'release-set-v2-sha256-[0-9a-f]{64}';
const SHA = /^[0-9a-f]{40}$/;
const PROMOTE = new RegExp(`^/ar11-fc6-promote\\s+(${RELEASE})\\s+(${RELEASE}|NONE)\\s+(\\S+)$`);
const NEGATIVE = new RegExp(`^/ar11-fc6-negative\\s+(${RELEASE})\\s+([1-9][0-9]*)\\s+(\\S+)$`);
const TRIGGER_DOCUMENT = Object.freeze({
  schema_version: 1,
  kind: 'AR11_FC6_EPHEMERAL_OPERATOR_TRIGGER',
  authority: 'NONE',
  merge_authorized: false,
  tracker_issue: ISSUE_NUMBER,
});

function fail(message) {
  throw new Error(`AR-11 FC-6 operator request rejected: ${message}`);
}

function parseCommand(body, requestId) {
  const text = typeof body === 'string' ? body.trim() : '';
  let match = text.match(PROMOTE);
  if (match) {
    const [, releaseSetId, expectedCurrent, confirmation] = match;
    if (confirmation !== `${releaseSetId}:${expectedCurrent}`) fail('promotion confirmation does not bind target and expected current');
    return {
      operation: 'promote',
      requestId,
      releaseSetId,
      expectedCurrent,
      sourceRunId: '',
      confirmation,
    };
  }

  match = text.match(NEGATIVE);
  if (match) {
    const [, releaseSetId, sourceRunId, confirmation] = match;
    if (confirmation !== `${releaseSetId}:${sourceRunId}`) fail('negative confirmation does not bind Release Set and source run');
    return {
      operation: 'rollback-negative',
      requestId,
      releaseSetId,
      expectedCurrent: releaseSetId,
      sourceRunId,
      confirmation,
    };
  }

  fail('command syntax is not exact');
}

function parseIssueComment(event) {
  if (event.action !== 'created') fail('only newly created issue comments are accepted');
  if (event.issue?.number !== ISSUE_NUMBER || event.issue?.pull_request) fail(`request must be an issue comment on #${ISSUE_NUMBER}`);
  if (event.comment?.user?.login !== OWNER || event.comment?.author_association !== 'OWNER') {
    fail('requester must be the repository owner with OWNER association');
  }
  if (!Number.isInteger(event.comment?.id) || event.comment.id <= 0) fail('comment id is missing');
  const body = typeof event.comment?.body === 'string' ? event.comment.body.trim() : '';
  if (!body.startsWith('/ar11-fc6-')) return null;
  return { request: parseCommand(body, String(event.comment.id)), responseIssue: ISSUE_NUMBER, pullRequest: null };
}

function parsePullRequest(event) {
  const pull = event.pull_request;
  const headRef = pull?.head?.ref;
  const title = pull?.title;
  const looksLikeOperator = typeof headRef === 'string' && headRef.startsWith(PR_BRANCH_PREFIX);
  const hasOperatorTitle = title === PR_TITLE;
  if (!looksLikeOperator && !hasOperatorTitle) return null;
  if (!looksLikeOperator || !hasOperatorTitle) fail('operator PR branch prefix and exact title must both match');
  if (event.action !== 'opened') fail('operator PR is accepted only on initial open');
  if (!Number.isInteger(event.number) || event.number <= 0 || pull?.number !== event.number) fail('operator PR number is invalid');
  if (pull?.user?.login !== OWNER || pull?.author_association !== 'OWNER') fail('operator PR requester must be repository owner with OWNER association');
  if (pull?.draft !== true) fail('operator PR must remain Draft');
  if (pull?.base?.ref !== 'main' || !SHA.test(pull?.base?.sha ?? '')) fail('operator PR must target exact protected main');
  if (pull?.head?.repo?.full_name !== REPOSITORY || !SHA.test(pull?.head?.sha ?? '')) fail('operator PR must use an exact same-repository head SHA');
  return {
    request: parseCommand(pull.body, `pr-${event.number}`),
    responseIssue: event.number,
    pullRequest: {
      number: event.number,
      baseSha: pull.base.sha,
      headSha: pull.head.sha,
      headRef,
    },
  };
}

function parseRequest(event) {
  if (!event || typeof event !== 'object' || Array.isArray(event)) fail('event must be one object');
  if (event.repository?.full_name !== REPOSITORY) fail(`repository must be ${REPOSITORY}`);
  if (event.pull_request) return parsePullRequest(event);
  if (event.issue) return parseIssueComment(event);
  return null;
}

function validateTriggerDocument(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) fail('operator PR trigger document must be one object');
  const expectedKeys = Object.keys(TRIGGER_DOCUMENT).sort();
  const actualKeys = Object.keys(value).sort();
  if (JSON.stringify(actualKeys) !== JSON.stringify(expectedKeys)) fail('operator PR trigger document keys drifted');
  for (const [key, expected] of Object.entries(TRIGGER_DOCUMENT)) {
    if (value[key] !== expected) fail(`operator PR trigger document field ${key} drifted`);
  }
}

function validatePullFiles(files) {
  if (!Array.isArray(files) || files.length !== 1) fail('operator PR must change exactly one data-only file');
  const file = files[0];
  if (file?.filename !== PR_TRIGGER_PATH || file?.status !== 'added' || file?.deletions !== 0) {
    fail(`operator PR may only add ${PR_TRIGGER_PATH}`);
  }
  if (typeof file?.sha !== 'string' || !SHA.test(file.sha)) fail('operator PR trigger blob SHA is invalid');
  return file.sha;
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

async function validatePullRequestEnvelope(pullRequest) {
  if (!pullRequest) return;
  const main = await github(`/repos/${REPOSITORY}/branches/main`);
  if (main?.protected !== true || main?.commit?.sha !== pullRequest.baseSha) {
    fail('operator PR base must equal the currently protected main SHA');
  }
  const files = await github(`/repos/${REPOSITORY}/pulls/${pullRequest.number}/files?per_page=100`);
  const triggerSha = validatePullFiles(files);
  const encodedRef = encodeURIComponent(pullRequest.headSha);
  const payload = await github(`/repos/${REPOSITORY}/contents/${PR_TRIGGER_PATH}?ref=${encodedRef}`);
  if (payload?.type !== 'file' || payload?.encoding !== 'base64' || payload?.sha !== triggerSha || typeof payload?.content !== 'string') {
    fail('operator PR trigger file response is malformed or unbound');
  }
  let trigger;
  try {
    trigger = JSON.parse(Buffer.from(payload.content.replace(/\s/g, ''), 'base64').toString('utf8'));
  } catch (error) {
    fail(`operator PR trigger document is invalid JSON: ${error instanceof Error ? error.message : error}`);
  }
  validateTriggerDocument(trigger);
}

async function listDispatchRuns() {
  const value = await github(`/repos/${REPOSITORY}/actions/workflows/${WORKFLOW}/runs?event=workflow_dispatch&branch=main&per_page=50`);
  if (!Array.isArray(value?.workflow_runs)) fail('workflow run list response is malformed');
  return value.workflow_runs;
}

async function dispatch(request) {
  const before = new Set((await listDispatchRuns()).map((run) => run.id));
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

  const expectedTitle = `AR11 ${request.operation} ${request.requestId}`;
  for (let attempt = 0; attempt < 30; attempt += 1) {
    const runs = await listDispatchRuns();
    const match = runs.find((run) => !before.has(run.id) && run.event === 'workflow_dispatch' && run.head_branch === 'main' && run.display_title === expectedTitle);
    if (match) return match;
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
    action: 'created',
    repository: { full_name: REPOSITORY },
    issue: { number: ISSUE_NUMBER },
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

  const wrongIssue = structuredClone(baseIssue);
  wrongIssue.comment.body = `/ar11-fc6-negative ${a} 987654 ${a}:987654`;
  wrongIssue.issue.number = 398;
  expectReject(() => parseRequest(wrongIssue), '#399');

  const pr = {
    action: 'opened',
    number: 77,
    repository: { full_name: REPOSITORY },
    pull_request: {
      number: 77,
      title: PR_TITLE,
      body: `/ar11-fc6-promote ${b} ${a} ${b}:${a}`,
      draft: true,
      author_association: 'OWNER',
      user: { login: OWNER },
      base: { ref: 'main', sha: '1'.repeat(40) },
      head: { ref: `${PR_BRANCH_PREFIX}a-to-b`, sha: '2'.repeat(40), repo: { full_name: REPOSITORY } },
    },
  };
  const prRequest = parseRequest(pr);
  if (prRequest?.request?.operation !== 'promote' || prRequest.request.requestId !== 'pr-77' || prRequest.responseIssue !== 77) fail('positive operator PR parse self-test failed');

  const ordinaryPr = structuredClone(pr);
  ordinaryPr.pull_request.title = 'ordinary feature';
  ordinaryPr.pull_request.head.ref = 'feature/ordinary';
  if (parseRequest(ordinaryPr) !== null) fail('ordinary PR unexpectedly became operator input');

  const nonDraft = structuredClone(pr);
  nonDraft.pull_request.draft = false;
  expectReject(() => parseRequest(nonDraft), 'must remain Draft');

  const wrongBase = structuredClone(pr);
  wrongBase.pull_request.base.ref = 'develop';
  expectReject(() => parseRequest(wrongBase), 'protected main');

  validateTriggerDocument(structuredClone(TRIGGER_DOCUMENT));
  const extraKey = { ...TRIGGER_DOCUMENT, surprise: true };
  expectReject(() => validateTriggerDocument(extraKey), 'keys drifted');

  validatePullFiles([{ filename: PR_TRIGGER_PATH, status: 'added', deletions: 0, sha: '3'.repeat(40) }]);
  expectReject(
    () => validatePullFiles([
      { filename: PR_TRIGGER_PATH, status: 'added', deletions: 0, sha: '3'.repeat(40) },
      { filename: 'src/untrusted.rs', status: 'added', deletions: 0, sha: '4'.repeat(40) },
    ]),
    'exactly one data-only file',
  );

  console.log('AR-11 FC-6 bounded comment/PR operator positive and negative self-tests passed.');
}

async function main() {
  if (process.argv.includes('--self-test')) {
    selfTest();
    return;
  }
  const eventPath = process.env.GITHUB_EVENT_PATH;
  if (!eventPath) fail('GITHUB_EVENT_PATH is unavailable');
  const event = JSON.parse(readFileSync(eventPath, 'utf8'));
  const parsed = parseRequest(event);
  if (parsed === null) return;
  await validatePullRequestEnvelope(parsed.pullRequest);
  const run = await dispatch(parsed.request);
  await commentRun(parsed.responseIssue, parsed.request, run);
  console.log(JSON.stringify({ operation: parsed.request.operation, request_id: parsed.request.requestId, run_id: run.id, run_url: run.html_url }));
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
