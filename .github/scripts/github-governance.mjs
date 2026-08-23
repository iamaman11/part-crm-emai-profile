#!/usr/bin/env node

import { readFile } from 'node:fs/promises';
import process from 'node:process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = path.resolve(SCRIPT_DIR, '..', '..');
const DESIRED_STATE_RELATIVE = 'architecture/github-governance.json';
const EXPECTED_REPOSITORY = 'iamaman11/part-crm-emai-profile';
const EXPECTED_ENVIRONMENTS = ['rehearsal', 'staging', 'production'];

function isObject(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function sameStringSet(actual, expected) {
  if (!Array.isArray(actual) || !Array.isArray(expected)) return false;
  if (actual.some((value) => typeof value !== 'string' || value.length === 0)) return false;
  if (expected.some((value) => typeof value !== 'string' || value.length === 0)) return false;
  const left = new Set(actual);
  const right = new Set(expected);
  return left.size === actual.length
    && right.size === expected.length
    && left.size === right.size
    && [...left].every((value) => right.has(value));
}

function exactKeys(object, expected) {
  return isObject(object) && sameStringSet(Object.keys(object), expected);
}

function validateDesiredState(desired) {
  const errors = [];
  const expect = (condition, message) => { if (!condition) errors.push(message); };

  expect(isObject(desired), 'desired governance state must be one object');
  if (!isObject(desired)) return errors;

  expect(
    exactKeys(desired, ['schema_version', 'kind', 'repository', 'observation_mode', 'main', 'environments', 'evaluation']),
    'desired governance state contains missing or unknown top-level fields',
  );
  expect(desired.schema_version === 1, 'schema_version must be 1');
  expect(desired.kind === 'GITHUB_GOVERNANCE_DESIRED_STATE', 'kind must be GITHUB_GOVERNANCE_DESIRED_STATE');
  expect(desired.repository === EXPECTED_REPOSITORY, `repository must be ${EXPECTED_REPOSITORY}`);
  expect(desired.observation_mode === 'READ_ONLY', 'observation_mode must be READ_ONLY');

  const main = desired.main;
  expect(
    exactKeys(main, [
      'branch',
      'protection_mechanism',
      'require_pull_request',
      'require_conversation_resolution',
      'enforce_admins',
      'strict_required_status_checks',
      'allow_force_pushes',
      'allow_deletions',
      'required_checks',
    ]),
    'main governance contains missing or unknown fields',
  );
  if (isObject(main)) {
    expect(main.branch === 'main', 'main.branch must be main');
    expect(main.protection_mechanism === 'classic_branch_protection', 'main must use classic_branch_protection');
    expect(main.require_pull_request === true, 'main must require pull requests');
    expect(main.require_conversation_resolution === true, 'main must require conversation resolution');
    expect(main.enforce_admins === true, 'main protection must enforce administrators');
    expect(main.strict_required_status_checks === true, 'main required status checks must be strict');
    expect(main.allow_force_pushes === false, 'main must block force pushes');
    expect(main.allow_deletions === false, 'main must block deletion');
    expect(
      Array.isArray(main.required_checks)
        && main.required_checks.length > 0
        && main.required_checks.every((value) => typeof value === 'string' && value.length > 0)
        && new Set(main.required_checks).size === main.required_checks.length,
      'main.required_checks must be a non-empty unique string set',
    );
  }

  const environments = desired.environments;
  expect(exactKeys(environments, EXPECTED_ENVIRONMENTS), 'environments must be exactly rehearsal, staging, production');
  if (isObject(environments)) {
    for (const name of EXPECTED_ENVIRONMENTS) {
      const environment = environments[name];
      const expectedKeys = name === 'production'
        ? ['required', 'allowed_branches', 'minimum_reviewers', 'can_admins_bypass']
        : ['required', 'allowed_branches', 'minimum_reviewers'];
      expect(exactKeys(environment, expectedKeys), `environment ${name} contains missing or unknown fields`);
      if (!isObject(environment)) continue;
      expect(environment.required === true, `environment ${name} must be required`);
      expect(sameStringSet(environment.allowed_branches, ['main']), `environment ${name} must allow exactly main`);
      expect(
        Number.isInteger(environment.minimum_reviewers) && environment.minimum_reviewers >= 0,
        `environment ${name} minimum_reviewers must be a non-negative integer`,
      );
    }
    expect(environments?.production?.minimum_reviewers >= 1, 'production must require at least one deployment reviewer');
    expect(environments?.production?.can_admins_bypass === false, 'production.can_admins_bypass must be false');
  }

  const evaluation = desired.evaluation;
  expect(
    exactKeys(evaluation, ['required_check_match', 'unknown_or_unreadable_live_state', 'mutation_authority', 'production_mutation']),
    'evaluation contains missing or unknown fields',
  );
  if (isObject(evaluation)) {
    expect(evaluation.required_check_match === 'EXACT', 'evaluation.required_check_match must be EXACT');
    expect(evaluation.unknown_or_unreadable_live_state === 'BLOCK', 'unknown or unreadable live state must BLOCK');
    expect(evaluation.mutation_authority === false, 'governance evaluator must not have mutation authority');
    expect(evaluation.production_mutation === false, 'N3 governance normalization must not mutate production');
  }

  return errors;
}

async function loadDesiredState(root) {
  const target = path.join(root, DESIRED_STATE_RELATIVE);
  const payload = JSON.parse(await readFile(target, 'utf8'));
  if (!isObject(payload)) throw new Error(`${DESIRED_STATE_RELATIVE} must contain one JSON object`);
  return payload;
}

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

function report(errors) {
  for (const error of errors) console.error(error);
  return errors.length === 0;
}

function selfTest(desired) {
  const baseline = validateDesiredState(desired);
  if (baseline.length !== 0) {
    console.error('governance self-test requires a valid desired-state contract');
    return report(baseline);
  }

  const fixtures = [
    {
      name: 'admin bypass',
      expected: 'enforce administrators',
      mutate: (copy) => { copy.main.enforce_admins = false; },
    },
    {
      name: 'non-strict checks',
      expected: 'must be strict',
      mutate: (copy) => { copy.main.strict_required_status_checks = false; },
    },
    {
      name: 'force push',
      expected: 'block force pushes',
      mutate: (copy) => { copy.main.allow_force_pushes = true; },
    },
    {
      name: 'duplicate required check',
      expected: 'unique string set',
      mutate: (copy) => { copy.main.required_checks.push(copy.main.required_checks[0]); },
    },
    {
      name: 'production branch broadening',
      expected: 'production must allow exactly main',
      mutate: (copy) => { copy.environments.production.allowed_branches = ['*']; },
    },
    {
      name: 'production reviewer removal',
      expected: 'at least one deployment reviewer',
      mutate: (copy) => { copy.environments.production.minimum_reviewers = 0; },
    },
    {
      name: 'mutation authority',
      expected: 'must not have mutation authority',
      mutate: (copy) => { copy.evaluation.mutation_authority = true; },
    },
    {
      name: 'non-exact live comparison',
      expected: 'must be EXACT',
      mutate: (copy) => { copy.evaluation.required_check_match = 'SUBSET'; },
    },
  ];

  for (const fixture of fixtures) {
    const copy = clone(desired);
    fixture.mutate(copy);
    const errors = validateDesiredState(copy);
    if (errors.length === 0 || !errors.some((error) => error.toLowerCase().includes(fixture.expected.toLowerCase()))) {
      console.error(`negative fixture ${fixture.name} was not rejected as expected: ${JSON.stringify(errors)}`);
      return false;
    }
  }

  console.log('Current GitHub governance negative fixtures passed.');
  return true;
}

async function githubJson(apiPath, token) {
  const response = await fetch(`https://api.github.com${apiPath}`, {
    headers: {
      Accept: 'application/vnd.github+json',
      Authorization: `Bearer ${token}`,
      'User-Agent': 'part-crm-github-governance-audit',
      'X-GitHub-Api-Version': '2022-11-28',
    },
  });
  if (!response.ok) {
    const body = (await response.text()).slice(0, 1000);
    throw new Error(`GitHub API ${apiPath} failed closed: HTTP ${response.status}: ${body}`);
  }
  return response.json();
}

function protectionCheckNames(protection) {
  const status = protection?.required_status_checks ?? {};
  const names = new Set();
  for (const context of status.contexts ?? []) {
    if (typeof context === 'string') names.add(context);
  }
  for (const check of status.checks ?? []) {
    if (typeof check?.context === 'string') names.add(check.context);
  }
  return [...names];
}

function requiredReviewerCount(environment) {
  const rule = (environment?.protection_rules ?? []).find((candidate) => candidate?.type === 'required_reviewers');
  return Array.isArray(rule?.reviewers) ? rule.reviewers.length : 0;
}

async function liveAudit(desired) {
  const errors = validateDesiredState(desired);
  if (errors.length !== 0) return errors;

  const token = process.env.GOVERNANCE_AUDIT_TOKEN;
  const repository = process.env.GITHUB_REPOSITORY || desired.repository;
  if (!token) return ['GOVERNANCE_AUDIT_TOKEN is required for the live Administration:read audit'];
  if (repository !== desired.repository) return [`GITHUB_REPOSITORY must be ${desired.repository}; observed ${repository}`];

  const branchName = desired.main.branch;
  const encodedBranch = encodeURIComponent(branchName);
  const branch = await githubJson(`/repos/${repository}/branches/${encodedBranch}`, token);
  if (branch?.protected !== true) errors.push('live main branch is not protected');

  let protection;
  try {
    protection = await githubJson(`/repos/${repository}/branches/${encodedBranch}/protection`, token);
  } catch (error) {
    return [...errors, String(error instanceof Error ? error.message : error)];
  }

  const requiredStatus = protection?.required_status_checks ?? {};
  if (requiredStatus.strict !== desired.main.strict_required_status_checks) {
    errors.push('live main required status check strictness does not match desired state');
  }
  const observedChecks = protectionCheckNames(protection);
  if (!sameStringSet(observedChecks, desired.main.required_checks)) {
    errors.push(`live main required status checks differ from desired exact set; desired=${JSON.stringify([...desired.main.required_checks].sort())} observed=${JSON.stringify([...observedChecks].sort())}`);
  }
  if (protection?.enforce_admins?.enabled !== desired.main.enforce_admins) {
    errors.push('live main enforce_admins does not match desired state');
  }
  if (Boolean(protection?.required_pull_request_reviews) !== desired.main.require_pull_request) {
    errors.push('live main pull-request requirement does not match desired state');
  }
  if (protection?.required_conversation_resolution?.enabled !== desired.main.require_conversation_resolution) {
    errors.push('live main conversation-resolution requirement does not match desired state');
  }
  if (protection?.allow_force_pushes?.enabled !== desired.main.allow_force_pushes) {
    errors.push('live main force-push policy does not match desired state');
  }
  if (protection?.allow_deletions?.enabled !== desired.main.allow_deletions) {
    errors.push('live main deletion policy does not match desired state');
  }

  for (const name of EXPECTED_ENVIRONMENTS) {
    const encodedName = encodeURIComponent(name);
    let environment;
    let policies;
    try {
      environment = await githubJson(`/repos/${repository}/environments/${encodedName}`, token);
      policies = await githubJson(`/repos/${repository}/environments/${encodedName}/deployment-branch-policies`, token);
    } catch (error) {
      errors.push(String(error instanceof Error ? error.message : error));
      continue;
    }

    const expected = desired.environments[name];
    if (environment?.deployment_branch_policy?.custom_branch_policies !== true) {
      errors.push(`live ${name} must use custom deployment branch policies`);
    }
    const policyNames = Array.isArray(policies?.branch_policies)
      ? policies.branch_policies.map((policy) => policy?.name).filter((value) => typeof value === 'string')
      : [];
    if (!sameStringSet(policyNames, expected.allowed_branches)) {
      errors.push(`live ${name} deployment branch policies do not equal ${JSON.stringify(expected.allowed_branches)}`);
    }
    if (requiredReviewerCount(environment) < expected.minimum_reviewers) {
      errors.push(`live ${name} has fewer than ${expected.minimum_reviewers} required deployment reviewers`);
    }
    if ('can_admins_bypass' in expected && environment?.can_admins_bypass !== expected.can_admins_bypass) {
      errors.push(`live ${name}.can_admins_bypass does not match desired state`);
    }
  }

  return errors;
}

function parseArgs(argv) {
  const command = argv[2] ?? 'contract';
  let root = DEFAULT_ROOT;
  for (let index = 3; index < argv.length; index += 1) {
    if (argv[index] === '--root') {
      if (!argv[index + 1]) throw new Error('--root requires a value');
      root = path.resolve(argv[index + 1]);
      index += 1;
      continue;
    }
    throw new Error(`unknown argument: ${argv[index]}`);
  }
  return { command, root };
}

async function main() {
  const { command, root } = parseArgs(process.argv);
  const desired = await loadDesiredState(root);

  if (command === 'contract') {
    const errors = validateDesiredState(desired);
    if (!report(errors)) return 1;
    console.log('Current GitHub governance desired-state contract is internally consistent.');
    return 0;
  }
  if (command === 'self-test') return selfTest(desired) ? 0 : 1;
  if (command === 'live') {
    const errors = await liveAudit(desired);
    if (!report(errors)) return 1;
    console.log('Live GitHub governance exactly matches current desired state.');
    return 0;
  }

  console.error(`unknown command: ${command}; expected contract, self-test, or live`);
  return 2;
}

main()
  .then((code) => { process.exitCode = code; })
  .catch((error) => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  });
