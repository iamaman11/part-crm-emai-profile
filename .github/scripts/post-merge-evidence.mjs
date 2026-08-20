#!/usr/bin/env node

import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = path.resolve(SCRIPT_DIR, '..', '..');
const RECORDER_RELATIVE = '.github/workflows/architecture-acceptance-recorder.yml';
const RELEASE_BUILD_RELATIVE = '.github/workflows/release-set-build.yml';
const REPOSITORY = 'iamaman11/part-crm-emai-profile';
const WORKFLOW_NAME = 'Release Set Build';
const TAG_PREFIX = 'evidence/release-set-build/';
const RECORD_KIND = 'RELEASE_SET_BUILD_SUCCESS_EVIDENCE';

function fail(message) {
  throw new Error(message);
}

function gitSha(value) {
  if (typeof value !== 'string' || !/^[0-9a-f]{40}$/u.test(value)) {
    fail('source SHA must be exact 40 lowercase hexadecimal');
  }
  return value;
}

function positiveInteger(value, label) {
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed <= 0) fail(`${label} must be a positive integer`);
  return parsed;
}

function evidenceTag(sourceSha) {
  return `${TAG_PREFIX}${gitSha(sourceSha)}`;
}

function buildRecord({ sourceSha, workflowName, event, headBranch, conclusion, runId, runAttempt }) {
  sourceSha = gitSha(sourceSha);
  if (workflowName !== WORKFLOW_NAME) fail(`workflow name must be ${WORKFLOW_NAME}`);
  if (event !== 'push') fail('Release Set build evidence requires event=push');
  if (headBranch !== 'main') fail('Release Set build evidence requires head_branch=main');
  if (conclusion !== 'success') fail('Release Set build evidence requires conclusion=success');
  return {
    schema_version: 1,
    kind: RECORD_KIND,
    repository: REPOSITORY,
    workflow: WORKFLOW_NAME,
    event: 'push',
    head_branch: 'main',
    conclusion: 'success',
    source_sha: sourceSha,
    workflow_run_id: positiveInteger(runId, 'workflow_run_id'),
    workflow_run_attempt: positiveInteger(runAttempt, 'workflow_run_attempt'),
    evidence_tag: evidenceTag(sourceSha),
    lifecycle_authority: false,
    production_authority: false,
    source_mutation_authority: false,
  };
}

function validateRecord(record, expectedSourceSha = null) {
  const errors = [];
  const expect = (condition, message) => { if (!condition) errors.push(message); };
  expect(record && typeof record === 'object' && !Array.isArray(record), 'evidence record must be one object');
  if (!record || typeof record !== 'object' || Array.isArray(record)) return errors;
  expect(record.schema_version === 1, 'schema_version must be 1');
  expect(record.kind === RECORD_KIND, `kind must be ${RECORD_KIND}`);
  expect(record.repository === REPOSITORY, `repository must be ${REPOSITORY}`);
  expect(record.workflow === WORKFLOW_NAME, `workflow must be ${WORKFLOW_NAME}`);
  expect(record.event === 'push', 'event must be push');
  expect(record.head_branch === 'main', 'head_branch must be main');
  expect(record.conclusion === 'success', 'conclusion must be success');
  expect(typeof record.source_sha === 'string' && /^[0-9a-f]{40}$/u.test(record.source_sha), 'source_sha must be exact lowercase Git SHA');
  if (typeof record.source_sha === 'string' && /^[0-9a-f]{40}$/u.test(record.source_sha)) {
    expect(record.evidence_tag === evidenceTag(record.source_sha), 'evidence_tag must be derived exactly from source_sha');
  }
  expect(Number.isInteger(record.workflow_run_id) && record.workflow_run_id > 0, 'workflow_run_id must be positive');
  expect(Number.isInteger(record.workflow_run_attempt) && record.workflow_run_attempt > 0, 'workflow_run_attempt must be positive');
  expect(record.lifecycle_authority === false, 'evidence must not become lifecycle authority');
  expect(record.production_authority === false, 'evidence must not become production authority');
  expect(record.source_mutation_authority === false, 'evidence must not gain source mutation authority');
  if (expectedSourceSha !== null) expect(record.source_sha === gitSha(expectedSourceSha), 'evidence source_sha differs from expected accepted-main SHA');
  return errors;
}

function requireSnippet(source, snippet, label, errors) {
  if (!source.includes(snippet)) errors.push(`${label} missing required contract snippet: ${JSON.stringify(snippet)}`);
}

async function contract(root) {
  const recorder = await readFile(path.join(root, RECORDER_RELATIVE), 'utf8');
  const releaseBuild = await readFile(path.join(root, RELEASE_BUILD_RELATIVE), 'utf8');
  const errors = [];

  requireSnippet(releaseBuild, 'name: Release Set Build', RELEASE_BUILD_RELATIVE, errors);
  requireSnippet(releaseBuild, '  push:\n    branches:\n      - main', RELEASE_BUILD_RELATIVE, errors);

  requireSnippet(recorder, '  workflow_run:\n    workflows: [Release Set Build]\n    types: [completed]', RECORDER_RELATIVE, errors);
  requireSnippet(recorder, 'record-release-set-evidence:', RECORDER_RELATIVE, errors);
  requireSnippet(recorder, "github.event_name == 'workflow_run'", RECORDER_RELATIVE, errors);
  requireSnippet(recorder, "github.event.workflow_run.conclusion == 'success'", RECORDER_RELATIVE, errors);
  requireSnippet(recorder, "github.event.workflow_run.event == 'push'", RECORDER_RELATIVE, errors);
  requireSnippet(recorder, "github.event.workflow_run.head_branch == 'main'", RECORDER_RELATIVE, errors);
  requireSnippet(recorder, 'node .github/scripts/post-merge-evidence.mjs record', RECORDER_RELATIVE, errors);
  requireSnippet(recorder, 'node .github/scripts/post-merge-evidence.mjs validate', RECORDER_RELATIVE, errors);
  requireSnippet(recorder, 'gh api --method POST "repos/$GITHUB_REPOSITORY/git/tags"', RECORDER_RELATIVE, errors);
  requireSnippet(recorder, 'gh api --method POST "repos/$GITHUB_REPOSITORY/git/refs"', RECORDER_RELATIVE, errors);

  if (TAG_PREFIX.startsWith('architecture/accepted/')) errors.push('post-merge evidence tag namespace must not overlap lifecycle acceptance tags');
  if (recorder.includes('production_mutation: true') || recorder.includes('production_ready: true')) {
    errors.push('post-merge evidence recorder must not authorize production');
  }

  if (errors.length !== 0) {
    for (const error of errors) console.error(error);
    return false;
  }
  console.log('Post-merge Release Set evidence observation contract is canonical.');
  return true;
}

function selfTest() {
  const baseline = buildRecord({
    sourceSha: 'a'.repeat(40),
    workflowName: WORKFLOW_NAME,
    event: 'push',
    headBranch: 'main',
    conclusion: 'success',
    runId: 123,
    runAttempt: 1,
  });
  if (validateRecord(baseline, 'a'.repeat(40)).length !== 0) fail('self-test baseline record must validate');
  if (evidenceTag('a'.repeat(40)) === evidenceTag('b'.repeat(40))) fail('evidence tag must bind exact source SHA');
  if (evidenceTag('a'.repeat(40)).startsWith('architecture/accepted/')) fail('evidence namespace overlaps lifecycle acceptance namespace');

  const fixtures = [
    ['workflow drift', { workflow: 'Other Workflow' }, 'workflow must be'],
    ['event drift', { event: 'pull_request' }, 'event must be push'],
    ['branch drift', { head_branch: 'feature' }, 'head_branch must be main'],
    ['conclusion drift', { conclusion: 'failure' }, 'conclusion must be success'],
    ['authority drift', { lifecycle_authority: true }, 'lifecycle authority'],
    ['source mutation drift', { source_mutation_authority: true }, 'source mutation authority'],
    ['tag drift', { evidence_tag: `${TAG_PREFIX}${'b'.repeat(40)}` }, 'evidence_tag must be derived'],
  ];
  for (const [name, mutation, expected] of fixtures) {
    const candidate = structuredClone(baseline);
    Object.assign(candidate, mutation);
    const errors = validateRecord(candidate, 'a'.repeat(40));
    if (!errors.some((error) => error.includes(expected))) {
      fail(`negative fixture ${name} was not rejected as expected: ${JSON.stringify(errors)}`);
    }
  }

  let rejected = false;
  try {
    buildRecord({ sourceSha: 'a'.repeat(40), workflowName: WORKFLOW_NAME, event: 'push', headBranch: 'main', conclusion: 'failure', runId: 1, runAttempt: 1 });
  } catch {
    rejected = true;
  }
  if (!rejected) fail('record builder accepted a non-success workflow conclusion');
  console.log('Post-merge evidence negative fixtures passed.');
}

function parseArgs(argv) {
  const command = argv[2] ?? 'contract';
  const options = {};
  for (let index = 3; index < argv.length; index += 1) {
    const key = argv[index];
    if (!key.startsWith('--') || !argv[index + 1]) fail(`invalid argument: ${key}`);
    options[key.slice(2)] = argv[index + 1];
    index += 1;
  }
  return { command, options };
}

async function main() {
  const { command, options } = parseArgs(process.argv);
  const root = options.root ? path.resolve(options.root) : DEFAULT_ROOT;
  if (command === 'contract') return (await contract(root)) ? 0 : 1;
  if (command === 'self-test') {
    selfTest();
    return 0;
  }
  if (command === 'tag') {
    process.stdout.write(`${evidenceTag(options['source-sha'])}\n`);
    return 0;
  }
  if (command === 'record') {
    const output = options.output;
    if (!output) fail('--output is required');
    const record = buildRecord({
      sourceSha: options['source-sha'],
      workflowName: options['workflow-name'],
      event: options.event,
      headBranch: options['head-branch'],
      conclusion: options.conclusion,
      runId: options['run-id'],
      runAttempt: options['run-attempt'],
    });
    await writeFile(output, `${JSON.stringify(record, null, 2)}\n`, 'utf8');
    process.stdout.write(`${record.evidence_tag}\n`);
    return 0;
  }
  if (command === 'validate') {
    const file = options.file;
    if (!file) fail('--file is required');
    const record = JSON.parse(await readFile(file, 'utf8'));
    const errors = validateRecord(record, options['source-sha'] ?? null);
    if (errors.length !== 0) {
      for (const error of errors) console.error(error);
      return 1;
    }
    process.stdout.write(`${record.evidence_tag}\n`);
    return 0;
  }
  fail(`unknown command: ${command}`);
}

main().then((code) => { process.exitCode = code; }).catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
