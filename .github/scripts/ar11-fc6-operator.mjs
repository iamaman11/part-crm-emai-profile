#!/usr/bin/env node

import { readFileSync } from 'node:fs';
import process from 'node:process';

const REPOSITORY = 'iamaman11/part-crm-emai-profile';
const ISSUE_NUMBER = 399;
const WORKFLOW = 'release-set-promotion.yml';
const RELEASE = 'release-set-v2-sha256-[0-9a-f]{64}';
const PROMOTE = new RegExp(`^/ar11-fc6-promote\\s+(${RELEASE})\\s+(${RELEASE}|NONE)\\s+(\\S+)$`);
const NEGATIVE = new RegExp(`^/ar11-fc6-negative\\s+(${RELEASE})\\s+([1-9][0-9]*)\\s+(\\S+)$`);

function fail(message) {
  throw new Error(`AR-11 FC-6 operator request rejected: ${message}`);
}

function parseRequest(event) {
  if (!event || typeof event !== 'object' || Array.isArray(event)) fail('event must be one object');
  if (event.action !== 'created') fail('only newly created issue comments are accepted');
  if (event.repository?.full_name !== REPOSITORY) fail(`repository must be ${REPOSITORY}`);
  if (event.issue?.number !== ISSUE_NUMBER || event.issue?.pull_request) fail(`request must be an issue comment on #${ISSUE_NUMBER}`);
  if (event.comment?.user?.login !== 'iamaman11' || event.comment?.author_association !== 'OWNER') {
    fail('requester must be the repository owner with OWNER association');
  }
  if (!Number.isInteger(event.comment?.id) || event.comment.id <= 0) fail('comment id is missing');
  const body = typeof event.comment?.body === 'string' ? event.comment.body.trim() : '';
  if (!body.startsWith('/ar11-fc6-')) return null;

  let match = body.match(PROMOTE);
  if (match) {
    const [, releaseSetId, expectedCurrent, confirmation] = match;
    if (confirmation !== `${releaseSetId}:${expectedCurrent}`) fail('promotion confirmation does not bind target and expected current');
    return {
      operation: 'promote',
      requestId: String(event.comment.id),
      releaseSetId,
      expectedCurrent,
      sourceRunId: '',
      confirmation,
    };
  }

  match = body.match(NEGATIVE);
  if (match) {
    const [, releaseSetId, sourceRunId, confirmation] = match;
    if (confirmation !== `${releaseSetId}:${sourceRunId}`) fail('negative confirmation does not bind Release Set and source run');
    return {
      operation: 'rollback-negative',
      requestId: String(event.comment.id),
      releaseSetId,
      expectedCurrent: releaseSetId,
      sourceRunId,
      confirmation,
    };
  }

  fail('command syntax is not exact');
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

async function commentRun(event, request, run) {
  const body = [
    `FC-6 canonical ${request.operation} request accepted from issue comment ${request.requestId}.`,
    `Run: ${run.html_url}`,
    `Run id: ${run.id}`,
    `Target Release Set: \`${request.releaseSetId}\``,
    `Expected current: \`${request.expectedCurrent}\``,
    'This adapter has no provider credentials and performs no provider mutation; execution remains inside the canonical Release Set Promotion workflow.',
  ].join('\n\n');
  await github(`/repos/${REPOSITORY}/issues/${event.issue.number}/comments`, { method: 'POST', body: { body } });
}

function selfTest() {
  const base = {
    action: 'created',
    repository: { full_name: REPOSITORY },
    issue: { number: ISSUE_NUMBER },
    comment: { id: 12345, user: { login: 'iamaman11' }, author_association: 'OWNER', body: '' },
  };
  const a = `release-set-v2-sha256-${'a'.repeat(64)}`;
  const b = `release-set-v2-sha256-${'b'.repeat(64)}`;

  base.comment.body = `/ar11-fc6-promote ${b} ${a} ${b}:${a}`;
  const promote = parseRequest(base);
  if (promote?.operation !== 'promote' || promote.releaseSetId !== b || promote.expectedCurrent !== a) fail('positive promotion parse self-test failed');

  base.comment.body = `/ar11-fc6-negative ${a} 987654 ${a}:987654`;
  const negative = parseRequest(base);
  if (negative?.operation !== 'rollback-negative' || negative.sourceRunId !== '987654') fail('positive negative-evidence parse self-test failed');

  base.comment.body = 'ordinary tracker comment';
  if (parseRequest(base) !== null) fail('ordinary tracker comment unexpectedly became operator input');

  const wrongOwner = structuredClone(base);
  wrongOwner.comment.body = `/ar11-fc6-promote ${b} ${a} ${b}:${a}`;
  wrongOwner.comment.user.login = 'attacker';
  try { parseRequest(wrongOwner); fail('non-owner request unexpectedly passed'); } catch (error) { if (!String(error).includes('requester must be')) throw error; }

  const badConfirmation = structuredClone(base);
  badConfirmation.comment.body = `/ar11-fc6-promote ${b} ${a} BAD`;
  try { parseRequest(badConfirmation); fail('unbound confirmation unexpectedly passed'); } catch (error) { if (!String(error).includes('confirmation')) throw error; }

  const wrongIssue = structuredClone(base);
  wrongIssue.comment.body = `/ar11-fc6-negative ${a} 987654 ${a}:987654`;
  wrongIssue.issue.number = 398;
  try { parseRequest(wrongIssue); fail('wrong issue unexpectedly passed'); } catch (error) { if (!String(error).includes('#399')) throw error; }

  console.log('AR-11 FC-6 bounded operator parser positive/negative self-tests passed.');
}

async function main() {
  if (process.argv.includes('--self-test')) {
    selfTest();
    return;
  }
  const eventPath = process.env.GITHUB_EVENT_PATH;
  if (!eventPath) fail('GITHUB_EVENT_PATH is unavailable');
  const event = JSON.parse(readFileSync(eventPath, 'utf8'));
  const request = parseRequest(event);
  if (request === null) return;
  const run = await dispatch(request);
  await commentRun(event, request, run);
  console.log(JSON.stringify({ operation: request.operation, request_id: request.requestId, run_id: run.id, run_url: run.html_url }));
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
