#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { execFileSync, spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const TAG_PREFIX = 'architecture/accepted/';
const CAP_S0_TAG = `${TAG_PREFIX}cap-s0`;
const SEQUENCE_PATH = 'architecture/architecture-program-sequence.json';
const CAMOUFOX_LOCK_PATH = 'runtime/camouhost/camoufox-patch-lock.json';

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
function gitBytes(args) { return execFileSync('git', args, { cwd: ROOT }); }
function exactSha(value, label) {
  if (!/^[0-9a-f]{40}$/.test(String(value ?? ''))) fail(`${label} must be exact lowercase 40-hex`);
}
function exactSha256(value, label) {
  if (!/^[0-9a-f]{64}$/.test(String(value ?? ''))) fail(`${label} must be exact lowercase sha256`);
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
function requiredCapS0Fields(record) {
  const fields = [
    'schema_version', 'kind', 'transaction', 'owning_issue', 'original_s0_issue',
    'implementation_pr', 'repair_pr', 'repair_base_sha', 'source_candidate_sha',
    'source_candidate_tree', 'repair_candidate_sha', 'repair_candidate_tree',
    'accepted_main_sha', 'accepted_main_tree', 'upstream_camoufox_commit',
    'patch_sha256', 'camoufox_candidate_sha256', 'release_set_id', 'ec_namespace',
    'ec_result', 'ec_unknown_count', 'evidence_refs', 'required_status_contexts_total',
    'required_status_contexts_success', 'post_merge_required_contexts',
    'release_publication', 'production_authorized', 'provider_mutation_authorized',
  ];
  for (const field of fields) if (!Object.hasOwn(record, field)) fail(`CAP S0 acceptance record missing ${field}`);
}
function observeCapS0Record(record, tag = null) {
  if (!record || typeof record !== 'object' || Array.isArray(record) || record.schema_version !== 2) fail('CAP S0 acceptance schema drifted');
  requiredCapS0Fields(record);
  if (record.kind !== 'CAP_S0_ACCEPTANCE' || record.transaction !== 'CAP-EXEC S0-PM') fail('CAP S0 acceptance identity drifted');
  if (record.owning_issue !== 539 || record.original_s0_issue !== 535 || record.implementation_pr !== 536) fail('CAP S0 issue provenance drifted');
  if (!Number.isSafeInteger(record.repair_pr) || record.repair_pr <= 0) fail('CAP S0 repair_pr must be positive');
  for (const key of [
    'repair_base_sha', 'source_candidate_sha', 'source_candidate_tree',
    'repair_candidate_sha', 'repair_candidate_tree', 'accepted_main_sha',
    'accepted_main_tree', 'upstream_camoufox_commit',
  ]) exactSha(record[key], key);
  exactSha256(record.patch_sha256, 'patch_sha256');
  const candidateDigests = record.camoufox_candidate_sha256;
  if (!candidateDigests || typeof candidateDigests !== 'object' || Array.isArray(candidateDigests)) fail('CAP S0 candidate digest map is malformed');
  exactSha256(candidateDigests.linux_x86_64, 'camoufox_candidate_sha256.linux_x86_64');
  exactSha256(candidateDigests.windows_x86_64, 'camoufox_candidate_sha256.windows_x86_64');
  if (!/^release-set-v3-sha256-[0-9a-f]{64}$/.test(record.release_set_id)) fail('CAP S0 Release Set identity is malformed');
  if (record.ec_namespace !== 'EC-S0-01..18' || record.ec_result !== 'PASS' || record.ec_unknown_count !== 0) fail('CAP S0 EC disposition drifted');
  if (!Array.isArray(record.evidence_refs) || record.evidence_refs.length === 0) fail('CAP S0 evidence refs are missing');
  for (const evidence of record.evidence_refs) {
    if (!evidence || typeof evidence !== 'object' || Array.isArray(evidence)
        || !Number.isSafeInteger(evidence.issue) || evidence.issue <= 0
        || !Number.isSafeInteger(evidence.comment) || evidence.comment <= 0
        || typeof evidence.role !== 'string' || evidence.role.length === 0) fail('CAP S0 evidence ref is malformed');
  }
  if (!Number.isSafeInteger(record.required_status_contexts_total)
      || record.required_status_contexts_total <= 0
      || record.required_status_contexts_success !== record.required_status_contexts_total) fail('CAP S0 required-context proof is incomplete');
  if (record.post_merge_required_contexts !== 'SUCCESS' || record.release_publication !== 'SUCCESS') fail('CAP S0 post-merge proof is incomplete');
  if (record.production_authorized !== false || record.provider_mutation_authorized !== false) fail('CAP S0 acceptance may not authorize Production/provider mutation');

  const acceptedTree = gitText(['rev-parse', `${record.accepted_main_sha}^{tree}`]);
  const acceptedParent = gitText(['rev-parse', `${record.accepted_main_sha}^1`]);
  if (acceptedTree !== record.accepted_main_tree || acceptedTree !== record.repair_candidate_tree) fail('CAP S0 accepted-main tree differs from repair candidate');
  if (acceptedParent !== record.repair_base_sha) fail('CAP S0 accepted-main first parent differs from repair base');
  const repairCandidate = git(['cat-file', '-e', `${record.repair_candidate_sha}^{commit}`], { check: false });
  if (repairCandidate.status === 0 && gitText(['rev-parse', `${record.repair_candidate_sha}^{tree}`]) !== record.repair_candidate_tree) fail('CAP S0 repair candidate tree differs from record');
  const sourceCandidate = git(['cat-file', '-e', `${record.source_candidate_sha}^{commit}`], { check: false });
  if (sourceCandidate.status === 0 && gitText(['rev-parse', `${record.source_candidate_sha}^{tree}`]) !== record.source_candidate_tree) fail('CAP S0 source candidate tree differs from record');
  const tagTarget = tag ? gitText(['rev-list', '-n', '1', tag]) : record.accepted_main_sha;
  if (tagTarget !== record.accepted_main_sha) fail('CAP S0 tag target differs from accepted main');

  const lock = JSON.parse(gitText(['show', `${record.accepted_main_sha}:${CAMOUFOX_LOCK_PATH}`]));
  if (lock.schema_version !== 2) fail('accepted-main Camoufox patch lock schema drifted');
  if (record.upstream_camoufox_commit !== lock.browser?.release_commit) fail('CAP S0 upstream Camoufox commit differs from accepted-main lock');
  if (record.patch_sha256 !== lock.patch?.sha256) fail('CAP S0 patch digest differs from accepted-main lock');
  const patchBytes = gitBytes(['show', `${record.accepted_main_sha}:${lock.patch.path}`]);
  const observedPatchSha = createHash('sha256').update(patchBytes).digest('hex');
  if (observedPatchSha !== record.patch_sha256) fail('CAP S0 patch bytes differ from recorded digest');
  if (candidateDigests.linux_x86_64 !== lock.candidates?.linux?.artifact_sha256
      || candidateDigests.windows_x86_64 !== lock.candidates?.windows?.artifact_sha256) fail('CAP S0 candidate digests differ from accepted-main lock');

  return {
    record_schema_version: record.schema_version,
    kind: record.kind,
    transaction: record.transaction,
    repair_pr: record.repair_pr,
    accepted_main_sha: record.accepted_main_sha,
    accepted_main_tree: record.accepted_main_tree,
    tag_target_sha: tagTarget,
    release_set_id: record.release_set_id,
    required_status_contexts_total: record.required_status_contexts_total,
    required_status_contexts_success: record.required_status_contexts_success,
    post_merge_required_contexts: record.post_merge_required_contexts,
    release_publication: record.release_publication,
    production_authorized: record.production_authorized,
    provider_mutation_authorized: record.provider_mutation_authorized,
  };
}
function evidence(appendFile) {
  const ids = sequenceIds();
  const observations = [];
  const tags = gitText(['tag', '--list', `${TAG_PREFIX}*`]).split(/\r?\n/).filter(Boolean);
  for (const tag of tags) {
    if (gitText(['cat-file', '-t', tag]) !== 'tag') fail(`acceptance ref is not an annotated tag: ${tag}`);
    const record = JSON.parse(gitText(['for-each-ref', `refs/tags/${tag}`, '--format=%(contents)']));
    if (tag === CAP_S0_TAG) {
      observeCapS0Record(record, tag);
      continue;
    }
    const suffix = tag.slice(TAG_PREFIX.length).toUpperCase();
    const expectedSlice = normalizeSlice(suffix);
    if (!expectedSlice || !ids.has(expectedSlice)) fail(`unknown architecture acceptance tag: ${tag}`);
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
else if (args.command === 'cap-record') {
  if (!args.file) fail('cap-record requires --file');
  console.log(JSON.stringify(observeCapS0Record(readJson(args.file))));
} else fail(`unknown command ${args.command}; expected evidence, premerge or cap-record`);
