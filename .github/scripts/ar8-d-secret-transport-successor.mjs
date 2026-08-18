#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';

const ROOT = process.cwd();
const AUTHORITY = 'architecture/ar8-d-secret-transport-successor.json';
const PROMOTION = '.github/workflows/mailbox-secret-resolver-promotion.yml';
const BINDING_HELPER = '.github/scripts/worker-secret-bindings.mjs';
const D3_AUTHORITY = 'architecture/pre2j-d3-resolver-bootstrap-authority.json';
const D3_MARKER = 'architecture/pre2j-d3-resolver-bootstrap-implementation.json';
const SUPERSEDED_ROUTINE_BINDINGS = [
  'CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON',
  'CLOUDFLARE_RESOLVER_SECRETS_JSON',
];

const EXPECTED_AUTHORITY = {
  schema_version: 1,
  kind: 'POLICY_TRANSITION',
  status: 'candidate',
  tracking_issue: 361,
  parent_issue: 308,
  canonical_inventory: 'architecture/inventory.json',
  predecessor: {
    policy: 'Pre-2J D3 resolver/bootstrap secret-bundle transport',
    d3_authority: D3_AUTHORITY,
    d3_authority_commit: '6a7dad9f74a25ccfd77cdd1a76216d8a46694e10',
    d3_implementation_marker: D3_MARKER,
    d3_implementation_merge_commit: '25bf15887fc835a6109c34ce21c083f3c307c455',
    transition_base_main: '9635ef21aafa0e2ff04551ef4cecf9497cbc87d5',
    promotion_workflow: PROMOTION,
    promotion_workflow_git_blob_sha: '85fd78557c97c96c179ff5d45f338bf12e639305',
  },
  successor: {
    policy: 'AR-8D steady-state Worker secret binding verification',
    promotion_workflow: PROMOTION,
    binding_metadata_helper: BINDING_HELPER,
    worker_secret_value_authority: 'Cloudflare Worker secret store',
    required_binding_contract: 'env.<environment>.secrets.required',
    verification_command: 'wrangler secret list --format json',
    superseded_routine_deploy_bindings: SUPERSEDED_ROUTINE_BINDINGS,
    routine_deploy_secret_value_transport: false,
    routine_deploy_secret_mutation: false,
    rotation_lifecycle: 'separate_explicit_rotation_authority',
  },
  invariants: [
    'historical D3 secret-bundle transport remains mechanically provable at the transition base',
    'routine promotion verifies exact Worker secret binding names using metadata only',
    'routine promotion never receives Worker runtime secret bundles',
    'routine promotion never mutates Worker secret values',
    'superseded D3 bundle bindings remain historical metadata only and are not routine deployment inputs',
    'secret rotation remains a separate governed lifecycle',
    'AR-8 performs no production credential mutation',
  ],
};

function absolute(relative) {
  return path.join(ROOT, relative);
}

function read(relative) {
  return readFileSync(absolute(relative), 'utf8').replace(/\r\n?/g, '\n');
}

function load(relative) {
  const value = JSON.parse(read(relative));
  if (value === null || Array.isArray(value) || typeof value !== 'object') {
    throw new Error(`${relative}: root must be an object`);
  }
  return value;
}

function normalize(value) {
  if (Array.isArray(value)) {
    return value.map(normalize);
  }
  if (value !== null && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([key, nested]) => [key, normalize(nested)]),
    );
  }
  return value;
}

function equal(left, right) {
  return JSON.stringify(normalize(left)) === JSON.stringify(normalize(right));
}

function run(command, args, { check = true } = {}) {
  const result = spawnSync(command, args, {
    cwd: ROOT,
    encoding: 'utf8',
    windowsHide: true,
  });
  if (result.error) {
    if (check) {
      throw result.error;
    }
    return { status: -1, stdout: '', stderr: String(result.error) };
  }
  const status = result.status ?? -1;
  if (check && status !== 0) {
    throw new Error(
      `${command} ${args.join(' ')} failed (${status}): ${(result.stderr || result.stdout || '').trim()}`,
    );
  }
  return { status, stdout: result.stdout ?? '', stderr: result.stderr ?? '' };
}

function git(args, options = {}) {
  return run('git', args, options);
}

function ensureCommit(ref) {
  if (git(['cat-file', '-e', `${ref}^{commit}`], { check: false }).status === 0) {
    return [];
  }
  git(['fetch', '--no-tags', '--depth=1', 'origin', ref], { check: false });
  if (git(['cat-file', '-e', `${ref}^{commit}`], { check: false }).status !== 0) {
    return [`governed predecessor commit is unavailable: ${ref}`];
  }
  return [];
}

function predecessorErrors(authority) {
  const predecessor = authority.predecessor;
  if (predecessor === null || Array.isArray(predecessor) || typeof predecessor !== 'object') {
    return ['AR-8D transition predecessor metadata is missing'];
  }
  const ref = String(predecessor.transition_base_main ?? '');
  const errors = ensureCommit(ref);
  if (errors.length > 0) {
    return errors;
  }
  const actualBlob = git(['rev-parse', `${ref}:${PROMOTION}`], { check: false });
  if (
    actualBlob.status !== 0 ||
    actualBlob.stdout.trim() !== predecessor.promotion_workflow_git_blob_sha
  ) {
    errors.push('historical D3 promotion workflow blob drifted from the governed transition base');
  }
  for (const relative of [D3_AUTHORITY, D3_MARKER]) {
    const historical = git(['show', `${ref}:${relative}`], { check: false });
    if (
      historical.status !== 0 ||
      historical.stdout.replace(/\r\n?/g, '\n') !== read(relative)
    ) {
      errors.push(`historical D3 authority changed after the AR-8D transition: ${relative}`);
    }
  }
  return errors;
}

function promotionErrors(promotion) {
  const errors = [];
  const required = [
    'workflow_dispatch:',
    'github-preflight',
    'mailbox-secret-resolver-promotion.py download-raw-artifact',
    '--expected-digest',
    '--expected-name',
    'validate-release-identities',
    'validate-staging-evidence',
    'verify-remote-d1',
    'deployments status',
    'attest',
    'CLOUDFLARE_API_TOKEN',
    'CLOUDFLARE_ACCESS_CLIENT_ID',
    'CLOUDFLARE_ACCESS_CLIENT_SECRET',
    'CLOUDFLARE_DEPLOY_MANIFEST_JSON',
    'worker-secret-bindings.mjs --normalize',
    'wrangler@4.94.0 secret list',
    '--format json',
    '--secret-list',
    'deploy --dry-run',
    '--strict',
    '--experimental-autoconfig=false',
  ];
  for (const marker of required) {
    if (!promotion.includes(marker)) {
      errors.push(`AR-8D routine promotion is missing ${JSON.stringify(marker)}`);
    }
  }

  const forbidden = [
    ...SUPERSEDED_ROUTINE_BINDINGS,
    '--secrets-file',
    'validate-secrets',
    ' secret put ',
    ' secret bulk ',
    ' secret delete ',
    ' secrets put ',
    ' secrets bulk ',
    ' secrets delete ',
    'worker-build --release',
    'cargo build',
    'wrangler d1 create',
    'wrangler r2 bucket create',
    'wrangler queues create',
    'BOOTSTRAP_API_TOKEN',
  ];
  const lowered = ` ${promotion.toLowerCase()} `;
  for (const marker of forbidden) {
    if (lowered.includes(marker.toLowerCase())) {
      errors.push(`AR-8D routine promotion contains forbidden secret/rebuild authority: ${marker}`);
    }
  }

  const count = (needle) => promotion.split(needle).length - 1;
  if (count('worker-secret-bindings.mjs --normalize') !== 2) {
    errors.push('resolver and control-plane configs must each restore secrets.required exactly once');
  }
  if (count('wrangler@4.94.0 secret list') !== 2) {
    errors.push('resolver and control-plane Workers must each expose one metadata-only secret inventory');
  }
  if (count('--secret-list') !== 2) {
    errors.push('resolver and control-plane secret inventories must each be validated exactly once');
  }
  if (count('mailbox-secret-resolver-promotion.py download-raw-artifact') !== 4) {
    errors.push('immutable resolver/control-plane raw artifacts must still be acquired four times');
  }
  if (count('--expected-digest') !== 4 || count('--expected-name') !== 4) {
    errors.push('all immutable raw artifact acquisitions must remain digest/name bound');
  }
  if (count('validate-release-identities') !== 2) {
    errors.push('preflight and deployment must both bind exact release identities');
  }
  if (count('validate-staging-evidence') !== 1) {
    errors.push('production must validate exactly one immutable staging evidence artifact');
  }
  if (count('deploy --dry-run') !== 2) {
    errors.push('both immutable Worker artifacts must still pass Wrangler dry-run');
  }
  if (count('--strict') !== 4 || count('--experimental-autoconfig=false') !== 4) {
    errors.push('both dry-runs and both deploys must retain strict/autoconfig-off validation');
  }

  const normalizeEnd = promotion.lastIndexOf('worker-secret-bindings.mjs --normalize');
  const secretListStart = promotion.indexOf('wrangler@4.94.0 secret list');
  if (normalizeEnd < 0 || secretListStart < 0 || normalizeEnd >= secretListStart) {
    errors.push('declarative secrets.required must be restored before remote binding metadata is read');
  }
  const firstValidation = promotion.indexOf('--secret-list');
  const firstDryRun = promotion.indexOf('deploy --dry-run');
  if (firstValidation < 0 || firstDryRun < 0 || firstValidation >= firstDryRun) {
    errors.push('exact Worker secret binding validation must complete before immutable artifact dry-run');
  }
  const resolverConfig = promotion.indexOf('--config "$resolver_config"', secretListStart);
  const controlConfig = promotion.indexOf('--config "$control_config"', secretListStart);
  if (resolverConfig < 0 || controlConfig < 0 || resolverConfig >= controlConfig) {
    errors.push('resolver verification/deployment must remain ordered before the control plane');
  }
  return errors;
}

function helperErrors(helper) {
  const errors = [];
  for (const marker of [
    'FORBIDDEN_VALUE_KEYS',
    'rejectValueShapedFields(secretList)',
    'secrets.required',
    'ALLOWED_SECRET_TYPES',
    'secret_text',
    'JSON.stringify(actual) !== JSON.stringify(expected)',
    '--normalize',
    'mode: 0o600',
    'selfTest()',
  ]) {
    if (!helper.includes(marker)) {
      errors.push(`Worker secret binding helper is missing ${JSON.stringify(marker)}`);
    }
  }
  return errors;
}

function successorBindingErrors(authority, promotion) {
  const successor = authority.successor;
  if (successor === null || Array.isArray(successor) || typeof successor !== 'object') {
    return ['AR-8D transition successor metadata is missing'];
  }
  if (!equal(successor.superseded_routine_deploy_bindings, SUPERSEDED_ROUTINE_BINDINGS)) {
    return ['AR-8D successor must govern exactly the two historical D3 bundle bindings'];
  }
  const errors = [];
  for (const name of SUPERSEDED_ROUTINE_BINDINGS) {
    if (promotion.includes(name)) {
      errors.push(`superseded D3 bundle binding leaked back into routine promotion: ${name}`);
    }
  }
  return errors;
}

function diffErrors(baseRef) {
  if (git(['cat-file', '-e', `${baseRef}^{commit}`], { check: false }).status !== 0) {
    return [`AR-8D base ref is unavailable: ${baseRef}`];
  }
  const errors = [];
  for (const relative of [D3_AUTHORITY, D3_MARKER]) {
    if (git(['diff', '--quiet', baseRef, '--', relative], { check: false }).status !== 0) {
      errors.push(`AR-8D successor must not edit historical D3 authority: ${relative}`);
    }
  }
  return errors;
}

function currentErrors() {
  let authority;
  try {
    authority = load(AUTHORITY);
  } catch (error) {
    return [`AR-8D transition authority is unreadable: ${error instanceof Error ? error.message : String(error)}`];
  }
  const errors = [];
  if (!equal(authority, EXPECTED_AUTHORITY)) {
    errors.push('AR-8D secret-transport successor authority drifted from its exact governed contract');
  }
  const promotion = read(PROMOTION);
  errors.push(...predecessorErrors(authority));
  errors.push(...successorBindingErrors(authority, promotion));
  errors.push(...promotionErrors(promotion));
  errors.push(...helperErrors(read(BINDING_HELPER)));
  return errors;
}

function selfTest() {
  const errors = currentErrors();
  if (errors.length > 0) {
    throw new Error(errors.join('\n'));
  }
  const promotion = read(PROMOTION);
  const helper = read(BINDING_HELPER);
  const mustReject = (label, result) => {
    if (result.length === 0) {
      throw new Error(`AR-8D negative fixture unexpectedly passed: ${label}`);
    }
  };
  mustReject('legacy bundle transport', promotionErrors(`${promotion}\nCLOUDFLARE_RESOLVER_SECRETS_JSON`));
  mustReject('secrets file', promotionErrors(`${promotion}\n--secrets-file forbidden.json`));
  mustReject('secret mutator', promotionErrors(`${promotion}\nnpx wrangler secret put NAME`));
  mustReject(
    'secret list replacement',
    promotionErrors(promotion.replace('wrangler@4.94.0 secret list', 'wrangler@4.94.0 secret get')),
  );
  mustReject(
    'value-shape rejection removal',
    helperErrors(helper.replace('rejectValueShapedFields(secretList);', '')),
  );
  const malformedAuthority = structuredClone(EXPECTED_AUTHORITY);
  malformedAuthority.successor.superseded_routine_deploy_bindings.push('AR8D_UNRELATED_STALE_BINDING');
  mustReject('unrelated stale binding', successorBindingErrors(malformedAuthority, promotion));

  const helperSelfTest = run(process.execPath, [BINDING_HELPER, '--self-test'], { check: false });
  if (helperSelfTest.status !== 0) {
    throw new Error((helperSelfTest.stderr || helperSelfTest.stdout).trim());
  }
  console.log('AR-8D secret-transport successor negative policy self-test passed.');
}

function parseArgs(argv) {
  let baseRef = null;
  let selfTestRequested = false;
  for (let index = 0; index < argv.length; index += 1) {
    const value = argv[index];
    if (value === '--self-test') {
      selfTestRequested = true;
    } else if (value === '--base-ref') {
      if (index + 1 >= argv.length) {
        throw new Error('--base-ref requires a value');
      }
      baseRef = argv[index + 1];
      index += 1;
    } else {
      throw new Error(`unknown argument: ${value}`);
    }
  }
  return { baseRef, selfTestRequested };
}

function main() {
  const { baseRef, selfTestRequested } = parseArgs(process.argv.slice(2));
  if (selfTestRequested) {
    selfTest();
    return;
  }
  const errors = currentErrors();
  if (baseRef !== null) {
    errors.push(...diffErrors(baseRef));
  }
  if (errors.length > 0) {
    throw new Error(errors.join('\n'));
  }
  console.log('AR-8D governed D3 -> steady-state secret-transport transition is valid.');
}

try {
  main();
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}
