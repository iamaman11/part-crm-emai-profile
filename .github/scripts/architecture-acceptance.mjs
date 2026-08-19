#!/usr/bin/env node

import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const POLICY_PATH = 'architecture/architecture-acceptance-policy.json';
const SEQUENCE_PATH = 'architecture/architecture-program-sequence.json';

function fail(message) {
  throw new Error(`architecture acceptance: ${message}`);
}
function readJson(relative) {
  return JSON.parse(readFileSync(path.join(ROOT, relative), 'utf8'));
}
function git(args, { check = true } = {}) {
  const result = spawnSync('git', args, { cwd: ROOT, encoding: 'utf8' });
  if (check && result.status !== 0) {
    fail(`git ${args.join(' ')} failed: ${(result.stderr || result.stdout).trim()}`);
  }
  return result;
}
function gitText(args) {
  return execFileSync('git', args, { cwd: ROOT, encoding: 'utf8' }).trim();
}
function sha(value, label) {
  if (!/^[0-9a-f]{40}$/.test(String(value ?? ''))) fail(`${label} must be an exact 40-hex SHA`);
}
function load() {
  return { policy: readJson(POLICY_PATH), sequence: readJson(SEQUENCE_PATH) };
}
function validateSequence(sequence) {
  if (sequence.schema_version !== 1 || sequence.kind !== 'ARCHITECTURE_PROGRAM_SEQUENCE') fail('program sequence identity drifted');
  if (sequence.program_issue !== 266 || sequence.state_model !== 'STATIC_ORDER_ONLY' || sequence.mutable_lifecycle_state_forbidden !== true) fail('program sequence mutable-state boundary drifted');
  const slices = sequence.slices;
  if (!Array.isArray(slices) || slices.length < 2) fail('program sequence must contain a linear slice list');
  const ids = slices.map((item) => item?.id);
  if (ids.some((id) => typeof id !== 'string') || new Set(ids).size !== ids.length) fail('program sequence slice ids must be unique strings');
  for (let i = 0; i < slices.length; i += 1) {
    const item = slices[i];
    const predecessor = i === 0 ? null : slices[i - 1].id;
    const successor = i + 1 === slices.length ? null : slices[i + 1].id;
    if (item.predecessor !== predecessor || item.successor !== successor) fail(`non-linear sequence around ${item.id}`);
    for (const forbidden of ['accepted', 'current', 'current_slice', 'accepted_checkpoint', 'implementation_state']) {
      if (Object.hasOwn(item, forbidden)) fail(`static sequence stores mutable lifecycle field ${forbidden} on ${item.id}`);
    }
  }
  const ar4d = sequence.non_linear_preserved_decisions?.find((item) => item.id === 'AR-4D');
  if (!ar4d || ar4d.state !== 'NOT_REQUIRED' || ar4d.not_in_linear_acceptance_chain !== true) fail('AR-4D NOT_REQUIRED decision lost');
  return slices;
}
function validatePolicy(policy, slices) {
  if (policy.schema_version !== 1 || policy.kind !== 'ARCHITECTURE_ACCEPTANCE_POLICY' || policy.status !== 'current') fail('acceptance policy identity drifted');
  if (policy.tracking_issue !== 375 || policy.program_issue !== 266) fail('acceptance policy issue ownership drifted');
  if (policy.program_sequence !== SEQUENCE_PATH || policy.source_branch !== 'main' || policy.source_history_count !== 1) fail('single-main/source-history boundary drifted');
  if (policy.merge_method !== 'squash') fail('architecture acceptance merge method must remain squash');
  const identity = policy.architecture_pr_identification ?? {};
  if (identity.title_prefix_required !== true || identity.head_branch_prefix_required !== true || identity.title_and_branch_slice_must_match !== true) fail('architecture PR identification boundary drifted');
  const pre = policy.premerge ?? {};
  if (pre.exact_head_required !== true || pre.behind_by_required !== 0 || pre.blocking_reviews_required !== 0 || pre.unresolved_review_threads_required !== 0 || pre.all_applicable_permanent_workflows_success !== true || pre.base_ref_required !== 'main' || pre.derived_current_slice_match_required !== true) fail('premerge acceptance matrix drifted');
  const merge = policy.merge ?? {};
  if (merge.expected_head_sha_binding_required !== true || merge.accepted_main_reread_required !== true || merge.candidate_tree_equals_merge_tree_required !== true || merge.merge_first_parent_equals_premerge_base_required !== true) fail('merge identity contract drifted');
  const metadata = policy.acceptance_metadata ?? {};
  if (metadata.storage !== 'annotated_git_tag' || metadata.namespace !== 'architecture/accepted/' || metadata.append_only !== true || metadata.force_move_allowed !== false || metadata.delete_allowed !== false || metadata.source_commit_required !== false || metadata.source_commit_forbidden_for_acceptance_only !== true || metadata.tag_required_from !== 'AR-12') fail('acceptance metadata boundary drifted');
  const requiredFields = new Set(metadata.required_fields ?? []);
  for (const required of ['slice', 'pr', 'base_sha', 'candidate_sha', 'candidate_tree', 'merge_sha', 'merge_tree', 'required_status_contexts_total', 'required_status_contexts_success', 'applicable_permanent_workflows_total', 'applicable_permanent_workflows_success', 'behind_by', 'blocking_reviews', 'unresolved_review_threads', 'accepted_main_reread', 'architecture_complete', 'production_core_gate', 'production_ready', 'production_mutation']) {
    if (!requiredFields.has(required)) fail(`acceptance record field is not mandatory: ${required}`);
  }
  const state = policy.state_derivation ?? {};
  if (state.frozen_legacy_accepted_through !== 'AR-10' || state.migration_bootstrap_acceptance?.slice !== 'AR-11' || state.current_slice_rule !== 'successor of accepted checkpoint' || state.missing_or_ambiguous_acceptance !== 'FAIL_CLOSED') fail('state derivation contract drifted');
  const prod = policy.production_invariants ?? {};
  if (prod.architecture_complete !== false || prod.production_core_gate !== 'BLOCKED' || prod.production_ready !== false || prod.production_mutation !== false || prod.until_slice !== 'AR-17') fail('production fail-closed boundary drifted');
  if (!slices.some((item) => item.id === state.frozen_legacy_accepted_through) || !slices.some((item) => item.id === state.migration_bootstrap_acceptance.slice)) fail('acceptance baseline references unknown slices');
}
function validateBootstrap(policy) {
  const record = policy.state_derivation.migration_bootstrap_acceptance;
  for (const key of ['base_sha', 'candidate_sha', 'candidate_tree', 'merge_sha', 'merge_tree', 'accepted_main_reread']) sha(record[key], `bootstrap ${key}`);
  if (record.issue !== 372 || record.pr !== 374 || record.required_status_contexts_total !== 23 || record.required_status_contexts_success !== 23 || record.applicable_permanent_workflows_total !== 17 || record.applicable_permanent_workflows_success !== 17 || record.behind_by !== 0 || record.blocking_reviews !== 0 || record.unresolved_review_threads !== 0 || record.architecture_complete !== false || record.production_core_gate !== 'BLOCKED' || record.production_ready !== false || record.production_mutation !== false) fail('AR-11 bootstrap acceptance evidence drifted');
  if (record.candidate_tree !== record.merge_tree || record.accepted_main_reread !== record.merge_sha) fail('AR-11 bootstrap tree/reread identity drifted');
  const mergeTree = gitText(['rev-parse', `${record.merge_sha}^{tree}`]);
  const parent = gitText(['rev-parse', `${record.merge_sha}^1`]);
  if (mergeTree !== record.merge_tree || mergeTree !== record.candidate_tree) fail('accepted AR-11 merge tree no longer matches exact-green candidate tree evidence');
  if (parent !== record.base_sha) fail('accepted AR-11 merge first parent no longer matches premerge base');
  const candidate = git(['cat-file', '-e', `${record.candidate_sha}^{commit}`], { check: false });
  if (candidate.status === 0 && gitText(['rev-parse', `${record.candidate_sha}^{tree}`]) !== record.candidate_tree) fail('AR-11 candidate commit tree drifted');
}
function scanForbidden() {
  const roots = ['.github/scripts', '.github/workflows', 'scripts'];
  const namePattern = /(?:ar\d+[a-z]?[-_])?acceptance[-_]closeout|closeout[-_]once/i;
  for (const relative of roots) {
    const dir = path.join(ROOT, relative);
    if (!existsSync(dir)) continue;
    for (const name of readdirSync(dir)) {
      if (namePattern.test(name)) fail(`retired per-slice closeout executable remains in current tree: ${relative}/${name}`);
    }
  }
}
function parseTagMessage(tag, requiredFields) {
  const type = gitText(['cat-file', '-t', tag]);
  if (type !== 'tag') fail(`architecture acceptance ref must be an annotated tag: ${tag}`);
  const raw = gitText(['for-each-ref', `refs/tags/${tag}`, '--format=%(contents)']);
  let record;
  try { record = JSON.parse(raw); } catch { fail(`architecture acceptance tag message must be one JSON object: ${tag}`); }
  if (!record || typeof record !== 'object' || Array.isArray(record) || record.schema_version !== 1) fail(`architecture acceptance tag record schema drifted: ${tag}`);
  for (const field of requiredFields) if (!Object.hasOwn(record, field)) fail(`acceptance tag ${tag} missing ${field}`);
  return record;
}
function acceptedTagRecords(policy, slices) {
  const prefix = policy.acceptance_metadata.namespace;
  const requiredFields = policy.acceptance_metadata.required_fields;
  const result = git(['tag', '--list', `${prefix}*`]);
  if (result.status !== 0) fail('cannot enumerate architecture acceptance tags');
  const bySlice = new Map();
  for (const tag of result.stdout.split(/\r?\n/).filter(Boolean)) {
    const suffix = tag.slice(prefix.length).toUpperCase();
    const expectedSlice = slices.find((item) => item.id.toUpperCase() === suffix)?.id;
    if (!expectedSlice) fail(`unknown architecture acceptance tag: ${tag}`);
    if (bySlice.has(expectedSlice)) fail(`duplicate architecture acceptance metadata for ${expectedSlice}`);
    const record = parseTagMessage(tag, requiredFields);
    if (record.slice !== expectedSlice) fail(`acceptance tag/path slice mismatch: ${tag}`);
    const target = gitText(['rev-list', '-n', '1', tag]);
    if (record.merge_sha !== target) fail(`acceptance tag ${tag} does not target its merge_sha`);
    for (const key of ['base_sha', 'candidate_sha', 'candidate_tree', 'merge_sha', 'merge_tree', 'accepted_main_reread']) sha(record[key], `${tag} ${key}`);
    if (record.candidate_tree !== record.merge_tree || record.accepted_main_reread !== record.merge_sha || record.behind_by !== 0 || record.blocking_reviews !== 0 || record.unresolved_review_threads !== 0 || record.required_status_contexts_total !== record.required_status_contexts_success || record.applicable_permanent_workflows_total !== record.applicable_permanent_workflows_success || record.architecture_complete !== false || record.production_core_gate !== 'BLOCKED' || record.production_ready !== false || record.production_mutation !== false) fail(`acceptance tag ${tag} does not encode an exact fail-closed acceptance`);
    if (gitText(['rev-parse', `${record.merge_sha}^{tree}`]) !== record.merge_tree || gitText(['rev-parse', `${record.merge_sha}^1`]) !== record.base_sha) fail(`acceptance tag ${tag} merge identity is invalid`);
    bySlice.set(expectedSlice, record);
  }
  return bySlice;
}
function derive(policy, slices) {
  const bootstrap = policy.state_derivation.migration_bootstrap_acceptance.slice;
  const start = slices.findIndex((item) => item.id === bootstrap);
  if (start < 0) fail('bootstrap slice missing from sequence');
  const records = acceptedTagRecords(policy, slices);
  let acceptedIndex = start;
  let gap = false;
  for (let i = start + 1; i < slices.length; i += 1) {
    const present = records.has(slices[i].id);
    if (present && gap) fail(`non-contiguous acceptance tag exists after a gap: ${slices[i].id}`);
    if (!present) {
      gap = true;
      continue;
    }
    acceptedIndex = i;
  }
  const accepted = slices[acceptedIndex];
  return {
    schema_version: 1,
    accepted_checkpoint: accepted.id,
    current_slice: accepted.successor,
    architecture_complete: false,
    production_core_gate: 'BLOCKED',
    production_ready: false,
    production_mutation: false,
  };
}
function normalizeSlice(raw) {
  if (!raw) return null;
  const match = String(raw).toUpperCase().match(/^AR-(\d+[A-Z]?)$/);
  return match ? `AR-${match[1]}` : null;
}
function sliceFromTitle(title) {
  const match = String(title ?? '').toUpperCase().match(/^(AR-\d+[A-Z]?):/);
  return normalizeSlice(match?.[1]);
}
function sliceFromHeadRef(headRef) {
  const match = String(headRef ?? '').toUpperCase().match(/^AGENT\/AR(\d+[A-Z]?)(?:-|$)/);
  return match ? normalizeSlice(`AR-${match[1]}`) : null;
}
function identifyArchitectureSlice(args) {
  const explicit = normalizeSlice(args.slice);
  if (args.slice && !explicit) fail(`invalid explicit architecture slice ${args.slice}`);
  const title = sliceFromTitle(args.title);
  const branch = sliceFromHeadRef(args.headRef);
  const signaled = [explicit, title, branch].filter(Boolean);
  if (signaled.length === 0) return null;
  if (!title || !branch) fail('architecture slice PR must identify the same slice in both title and agent/arN-* head branch');
  if (new Set(signaled).size !== 1) fail(`architecture slice signals disagree: ${signaled.join(', ')}`);
  return signaled[0];
}
function premerge(args, policy, slices) {
  const slice = identifyArchitectureSlice(args);
  if (!slice) return { architecture_slice: false, derived_state: derive(policy, slices) };
  if (!slices.some((item) => item.id === slice)) fail(`unknown architecture slice ${slice}`);
  if (args.baseRef !== 'main') fail('architecture slice PR must target main');
  sha(args.base, 'premerge base');
  sha(args.head, 'premerge head');
  if (args.base === args.head) fail('architecture slice candidate must differ from base');
  const mergeBase = gitText(['merge-base', args.base, args.head]);
  if (mergeBase !== args.base) fail('behind_by must be zero: candidate does not contain exact premerge main base');
  const state = derive(policy, slices);
  if (state.current_slice !== slice) fail(`slice ${slice} is not the derived current slice; expected ${state.current_slice}`);
  return {
    architecture_slice: true,
    slice,
    base_sha: args.base,
    candidate_sha: args.head,
    candidate_tree: gitText(['rev-parse', `${args.head}^{tree}`]),
    derived_state: state,
  };
}
function parseArgs(argv) {
  const result = { command: argv[2] ?? 'contract' };
  for (let i = 3; i < argv.length; i += 1) {
    const key = argv[i];
    const value = argv[i + 1];
    if (!key.startsWith('--') || value === undefined) fail(`invalid argument ${key}`);
    result[key.slice(2).replace(/-([a-z])/g, (_, c) => c.toUpperCase())] = value;
    i += 1;
  }
  return result;
}
function selfTest(policy, sequence) {
  const badSequence = structuredClone(sequence);
  badSequence.slices[1].predecessor = 'AR-X';
  let rejected = false;
  try { validateSequence(badSequence); } catch { rejected = true; }
  if (!rejected) fail('non-linear sequence negative fixture passed');
  const badPolicy = structuredClone(policy);
  badPolicy.acceptance_metadata.force_move_allowed = true;
  rejected = false;
  try { validatePolicy(badPolicy, validateSequence(sequence)); } catch { rejected = true; }
  if (!rejected) fail('force-movable acceptance tag negative fixture passed');
  const badBootstrap = structuredClone(policy);
  badBootstrap.state_derivation.migration_bootstrap_acceptance.merge_tree = '0'.repeat(40);
  rejected = false;
  try { validateBootstrap(badBootstrap); } catch { rejected = true; }
  if (!rejected) fail('AR-11 tree mismatch negative fixture passed');
  rejected = false;
  try { identifyArchitectureSlice({ title: 'AR-12: candidate', headRef: 'agent/ar13-wrong' }); } catch { rejected = true; }
  if (!rejected) fail('title/branch slice mismatch negative fixture passed');
  rejected = false;
  try { identifyArchitectureSlice({ title: 'AR-12: candidate', headRef: 'agent/other' }); } catch { rejected = true; }
  if (!rejected) fail('single-signal architecture PR negative fixture passed');
}

const args = parseArgs(process.argv);
const { policy, sequence } = load();
const slices = validateSequence(sequence);
validatePolicy(policy, slices);
validateBootstrap(policy);
scanForbidden();

if (args.command === 'contract') {
  console.log('One-merge architecture acceptance contract passed.');
} else if (args.command === 'derive') {
  console.log(JSON.stringify(derive(policy, slices)));
} else if (args.command === 'premerge') {
  console.log(JSON.stringify(premerge(args, policy, slices)));
} else if (args.command === 'self-test') {
  selfTest(policy, sequence);
  console.log('Architecture acceptance negative matrix passed.');
} else {
  fail(`unknown command ${args.command}; expected contract, derive, premerge, self-test`);
}
