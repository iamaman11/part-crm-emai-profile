#!/usr/bin/env node

import { execFileSync, spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const TAG_PREFIX = 'architecture/accepted/';
const SEQUENCE_PATH = 'architecture/architecture-program-sequence.json';

function fail(message) { throw new Error(`architecture acceptance observation: ${message}`); }
function readJson(relative) {
  const absolute = path.resolve(ROOT, relative);
  if (absolute !== ROOT && !absolute.startsWith(`${ROOT}${path.sep}`)) fail(`JSON input escapes repository workspace: ${relative}`);
  return JSON.parse(readFileSync(absolute, 'utf8'));
}
function git(args, { check = true } = {}) {
  const result = spawnSync('git', args, { cwd: ROOT, encoding: 'utf8' });
  if (check && result.status !== 0) fail(`git ${args.join(' ')} failed: ${(result.stderr || result.stdout).trim()}`);
  return result;
}
function gitText(args) { return execFileSync('git', args, { cwd: ROOT, encoding: 'utf8' }).trim(); }
function exactSha(value, label) {
  if (!/^[0-9a-f]{40}$/.test(String(value ?? ''))) fail(`${label} must be exact lowercase 40-hex`);
}
function normalizeSlice(raw) {
  const match = String(raw ?? '').toUpperCase().match(/^AR-(\d+[A-Z]?)$/);
  return match ? `AR-${match[1]}` : null;
}
function sequenceIds() {
  const sequence = readJson(SEQUENCE_PATH);
  if (sequence.schema_version !== 1 || sequence.kind !== 'ARCHITECTURE_PROGRAM_SEQUENCE') fail('program sequence identity drifted');
  const ids = sequence.slices?.map((slice) => slice.id);
  if (!Array.isArray(ids) || ids.some((id) => typeof id !== 'string') || new Set(ids).size !== ids.length) fail('program sequence IDs are malformed');
  return new Set(ids);
}
function requiredRecordFields(record) {
  const fields = [
    'schema_version', 'slice', 'pr', 'base_sha', 'candidate_sha', 'candidate_tree',
    'merge_sha', 'merge_tree', 'required_status_contexts_total',
    'required_status_contexts_success', 'applicable_permanent_workflows_total',
    'applicable_permanent_workflows_success', 'behind_by', 'blocking_reviews',
    'unresolved_review_threads', 'accepted_main_reread', 'architecture_complete',
    'production_core_gate', 'production_ready', 'production_mutation',
  ];
  for (const field of fields) if (!Object.hasOwn(record, field)) fail(`acceptance record missing ${field}`);
}
function observeRecord(record, ids, tag = null) {
  if (!record || typeof record !== 'object' || Array.isArray(record) || record.schema_version !== 1) fail('acceptance record schema drifted');
  requiredRecordFields(record);
  const slice = normalizeSlice(record.slice);
  if (!slice || slice !== record.slice || !ids.has(slice)) fail(`unknown acceptance slice ${record.slice}`);
  for (const key of ['base_sha', 'candidate_sha', 'candidate_tree', 'merge_sha', 'merge_tree', 'accepted_main_reread']) exactSha(record[key], key);
  const mergeTree = gitText(['rev-parse', `${record.merge_sha}^{tree}`]);
  const mergeParent = gitText(['rev-parse', `${record.merge_sha}^1`]);
  if (mergeTree !== record.merge_tree) fail(`${slice} merge tree observation differs from record`);
  if (mergeParent !== record.base_sha) fail(`${slice} merge first parent differs from record`);
  const candidate = git(['cat-file', '-e', `${record.candidate_sha}^{commit}`], { check: false });
  const candidateTree = candidate.status === 0
    ? gitText(['rev-parse', `${record.candidate_sha}^{tree}`])
    : record.candidate_tree;
  if (candidateTree !== record.candidate_tree) fail(`${slice} candidate tree observation differs from record`);
  const tagTarget = tag ? gitText(['rev-list', '-n', '1', tag]) : record.merge_sha;
  if (tagTarget !== record.merge_sha) fail(`${slice} tag target differs from record`);
  return {
    record_schema_version: record.schema_version,
    slice,
    pr: record.pr,
    base_sha: record.base_sha,
    candidate_sha: record.candidate_sha,
    candidate_tree: record.candidate_tree,
    observed_candidate_tree: candidateTree,
    merge_sha: record.merge_sha,
    merge_tree: record.merge_tree,
    observed_merge_tree: mergeTree,
    observed_merge_first_parent: mergeParent,
    tag_target_sha: tagTarget,
    required_status_contexts_total: record.required_status_contexts_total,
    required_status_contexts_success: record.required_status_contexts_success,
    applicable_permanent_workflows_total: record.applicable_permanent_workflows_total,
    applicable_permanent_workflows_success: record.applicable_permanent_workflows_success,
    behind_by: record.behind_by,
    blocking_reviews: record.blocking_reviews,
    unresolved_review_threads: record.unresolved_review_threads,
    accepted_main_reread: record.accepted_main_reread,
    architecture_complete: record.architecture_complete,
    production_core_gate: record.production_core_gate,
    production_ready: record.production_ready,
    production_mutation: record.production_mutation,
  };
}
function evidence(appendFile) {
  const ids = sequenceIds();
  const observations = [];
  const tags = gitText(['tag', '--list', `${TAG_PREFIX}*`]).split(/\r?\n/).filter(Boolean);
  for (const tag of tags) {
    if (gitText(['cat-file', '-t', tag]) !== 'tag') fail(`acceptance ref is not an annotated tag: ${tag}`);
    const suffix = tag.slice(TAG_PREFIX.length).toUpperCase();
    const expectedSlice = normalizeSlice(suffix);
    if (!expectedSlice || !ids.has(expectedSlice)) fail(`unknown architecture acceptance tag: ${tag}`);
    const record = JSON.parse(gitText(['for-each-ref', `refs/tags/${tag}`, '--format=%(contents)']));
    if (record.slice !== expectedSlice) fail(`acceptance tag/path slice mismatch: ${tag}`);
    observations.push(observeRecord(record, ids, tag));
  }
  if (appendFile) observations.push(observeRecord(readJson(appendFile), ids));
  return { schema_version: 1, source_branch: 'main', acceptance_observations: observations };
}
function premerge(args) {
  const explicit = normalizeSlice(args.slice);
  const title = normalizeSlice(String(args.title ?? '').match(/^(AR-\d+[A-Za-z]?):/i)?.[1]);
  const branchMatch = String(args.headRef ?? '').match(/^agent\/ar(\d+[A-Za-z]?)(?:-|$)/i);
  const branch = normalizeSlice(branchMatch ? `AR-${branchMatch[1]}` : null);
  if (!explicit && !title && !branch) return { architecture_slice: false };
  if (!title || !branch || new Set([explicit, title, branch].filter(Boolean)).size !== 1) fail('architecture slice signals disagree');
  if (!sequenceIds().has(title)) fail(`unknown architecture slice ${title}`);
  if (args.baseRef !== 'main') fail('architecture slice PR must target main');
  exactSha(args.base, 'premerge base');
  exactSha(args.head, 'premerge head');
  if (args.base === args.head || gitText(['merge-base', args.base, args.head]) !== args.base) fail('candidate is not based on exact current main');
  return { architecture_slice: true, slice: title, base_sha: args.base, candidate_sha: args.head, candidate_tree: gitText(['rev-parse', `${args.head}^{tree}`]) };
}
function parseArgs(argv) {
  const result = { command: argv[2] ?? 'evidence' };
  for (let index = 3; index < argv.length; index += 2) {
    const key = argv[index];
    const value = argv[index + 1];
    if (!key?.startsWith('--') || value === undefined) fail(`invalid argument ${key}`);
    result[key.slice(2).replace(/-([a-z])/g, (_, char) => char.toUpperCase())] = value;
  }
  return result;
}

const args = parseArgs(process.argv);
if (args.command === 'evidence') console.log(JSON.stringify(evidence(args.append)));
else if (args.command === 'premerge') console.log(JSON.stringify(premerge(args)));
else fail(`unknown command ${args.command}; expected evidence or premerge`);
