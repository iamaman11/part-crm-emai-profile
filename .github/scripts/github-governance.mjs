#!/usr/bin/env node

import { readFile } from 'node:fs/promises';
import process from 'node:process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = path.resolve(SCRIPT_DIR, '..', '..');
const CONTRACT_RELATIVE = 'architecture/github-governance-ar7.json';

const EXPECTED_REQUIRED_CHECKS = [
  'Certification Linux And WASM',
  'Certification Windows',
  'Cloudflare Worker Release Build',
  'D1 Catalog Migrations',
  'Encrypted Generation Linux And WASM',
  'Encrypted Generation Windows',
  'External Evidence Metadata',
  'External Readiness Projection',
  'External Review Attestations',
  'GitHub Governance Contract',
  'Invariants And Fail-Closed Boundaries',
  'Local Profile Linux',
  'Local Profile Windows',
  'React Operator UI',
  'Registry Domain D1 Adapter Worker And Contract',
  'Repository-Local Standalone Flow',
  'Resolver D1 first-bootstrap implementation',
  'Runtime Bundle Linux',
  'Runtime Bundle Windows',
  'Rust Linux and WASM',
  'Rust Windows And Profile Bridge Artifact',
];

function sameStringSet(actual, expected) {
  if (!Array.isArray(actual) || actual.some((value) => typeof value !== 'string')) return false;
  const actualSet = new Set(actual);
  const expectedSet = new Set(expected);
  if (actualSet.size !== actual.length || expectedSet.size !== expected.length) return false;
  if (actualSet.size !== expectedSet.size) return false;
  return [...actualSet].every((value) => expectedSet.has(value));
}

function validateContract(contract) {
  const errors = [];
  const expect = (condition, message) => {
    if (!condition) errors.push(message);
  };

  expect(contract?.schema_version === 1, 'schema_version must be 1');
  expect(contract?.slice === 'AR-7', 'slice must be AR-7');
  expect(
    contract?.status === 'ACCEPTED_AR7_GITHUB_GOVERNANCE',
    'status must remain the accepted AR-7 GitHub governance authority',
  );
  expect(contract?.repository === 'iamaman11/part-crm-emai-profile', 'repository authority drifted');
  expect(contract?.baseline_main === 'dde7123586b080c1c053e90ad0ba489d4620e4d2', 'AR-7 baseline_main drifted');
  expect(contract?.issue === 298, 'AR-7 implementation issue must remain #298');

  const source = contract?.source_authority ?? {};
  expect(source.accepted_branch === 'main', 'source_authority.accepted_branch must be main');
  expect(source.production_ref === 'refs/heads/main', 'source_authority.production_ref must be refs/heads/main');
  expect(source.immutable_bits_across_promotion === true, 'promotion must preserve immutable source bits');
  expect(source.rebuild_on_promotion === false, 'promotion must not rebuild source bits');

  const promotion = contract?.promotion ?? {};
  expect(
    JSON.stringify(promotion.order) === JSON.stringify(['dev', 'rehearsal', 'staging', 'production']),
    'promotion.order must be dev -> rehearsal -> staging -> production',
  );
  expect(
    JSON.stringify(promotion.hosted_environments) === JSON.stringify(['rehearsal', 'staging', 'production']),
    'promotion.hosted_environments must be rehearsal, staging, production',
  );
  expect(promotion.dev_authority === 'local_or_pull_request_candidate', 'dev authority drifted');

  const main = contract?.main_governance ?? {};
  expect(main.protection_mechanism === 'classic_branch_protection', 'main must use classic_branch_protection for AR-7');
  expect(main.require_pull_request === true, 'main must require pull requests');
  expect(main.require_conversation_resolution === true, 'main must require conversation resolution');
  expect(main.enforce_admins === true, 'main protection must enforce administrators');
  expect(main.strict_required_status_checks === true, 'main required status checks must be strict');
  expect(main.allow_force_pushes === false, 'main must block force pushes');
  expect(main.allow_deletions === false, 'main must block deletion');
  expect(
    sameStringSet(main.required_checks, EXPECTED_REQUIRED_CHECKS),
    'main_governance.required_checks must equal the exact AR-7 permanent PR check set',
  );

  const environments = contract?.environments ?? {};
  for (const name of ['rehearsal', 'staging', 'production']) {
    const environment = environments[name] ?? {};
    expect(environment.required === true, `environment ${name} must be required`);
    expect(
      sameStringSet(environment.allowed_branches, ['main']),
      `environment ${name} must allow exactly main`,
    );
    expect(
      Number.isInteger(environment.minimum_reviewers) && environment.minimum_reviewers >= 0,
      `environment ${name} minimum_reviewers must be a non-negative integer`,
    );
  }
  expect(environments?.production?.minimum_reviewers >= 1, 'production must require at least one deployment reviewer');
  expect(environments?.production?.can_admins_bypass === false, 'production.can_admins_bypass must be false');

  const acceptance = contract?.acceptance ?? {};
  expect(acceptance.direct_main_negative_probe_required === true, 'direct-main negative probe must remain required');
  expect(acceptance.live_audit_required === true, 'live governance audit must remain required');
  expect(acceptance.production_ready === false, 'AR-7 contract must not claim production readiness');
  expect(acceptance.implementation_issue === 298, 'AR-7 accepted implementation issue must be #298');
  expect(acceptance.implementation_pr === 299, 'AR-7 accepted implementation PR must be #299');
  expect(acceptance.implementation_exact_green_head === '1ebb9f42bb52cf86f1794667f5c9d630ce78e8a7', 'AR-7 exact-green head drifted');
  expect(acceptance.implementation_merge === '3492273cb9237850e3fa27343cc5edbdb0f66aa1', 'AR-7 implementation merge drifted');
  expect(acceptance.applicable_permanent_workflows === '14/14', 'AR-7 permanent workflow evidence drifted');
  expect(acceptance.hosted_audit_run_id === 31953316327, 'AR-7 hosted audit run drifted');
  expect(acceptance.hosted_contract_job === 'success', 'AR-7 hosted contract job must remain successful');
  expect(acceptance.hosted_state_job === 'success', 'AR-7 hosted state job must remain successful');
  expect(acceptance.direct_main_negative_probe === 'HTTP_409_REJECTED_NO_SENTINEL', 'AR-7 direct-main negative probe evidence drifted');
  expect(acceptance.closeout_issue === 300, 'AR-7 closeout issue must be #300');

  return errors;
}

async function loadContract(root) {
  const target = path.join(root, CONTRACT_RELATIVE);
  const text = await readFile(target, 'utf8');
  const payload = JSON.parse(text);
  if (!payload || typeof payload !== 'object' || Array.isArray(payload)) {
    throw new Error(`${CONTRACT_RELATIVE} must contain one JSON object`);
  }
  return payload;
}

function report(errors) {
  for (const error of errors) console.error(error);
  return errors.length === 0;
}

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

function selfTest(contract) {
  const baseline = validateContract(contract);
  if (baseline.length !== 0) {
    console.error('governance self-test requires a valid baseline contract');
    return report(baseline);
  }

  const fixtures = [
    {
      name: 'promotion bypass',
      expected: 'promotion.order',
      mutate: (copy) => { copy.promotion.order = ['dev', 'staging', 'production']; },
    },
    {
      name: 'required check removal',
      expected: 'required_checks',
      mutate: (copy) => { copy.main_governance.required_checks.pop(); },
    },
    {
      name: 'admin branch bypass',
      expected: 'enforce administrators',
      mutate: (copy) => { copy.main_governance.enforce_admins = false; },
    },
    {
      name: 'production environment bypass',
      expected: 'production.can_admins_bypass',
      mutate: (copy) => { copy.environments.production.can_admins_bypass = true; },
    },
    {
      name: 'production branch broadening',
      expected: 'production must allow exactly main',
      mutate: (copy) => { copy.environments.production.allowed_branches = ['*']; },
    },
    {
      name: 'premature production readiness',
      expected: 'production readiness',
      mutate: (copy) => { copy.acceptance.production_ready = true; },
    },
  ];

  for (const fixture of fixtures) {
    const copy = clone(contract);
    fixture.mutate(copy);
    const errors = validateContract(copy);
    if (errors.length === 0 || !errors.some((error) => error.toLowerCase().includes(fixture.expected.toLowerCase()))) {
      console.error(`negative fixture ${fixture.name} was not rejected as expected: ${JSON.stringify(errors)}`);
      return false;
    }
  }

  console.log('AR-7 GitHub governance negative fixtures passed.');
  return true;
}

async function githubJson(apiPath, token) {
  const response = await fetch(`https://api.github.com${apiPath}`, {
    headers: {
      Accept: 'application/vnd.github+json',
      Authorization: `Bearer ${token}`,
      'User-Agent': 'part-crm-ar7-governance-audit',
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

async function liveAudit(contract) {
  const errors = validateContract(contract);
  if (errors.length !== 0) return errors;

  const token = process.env.GOVERNANCE_AUDIT_TOKEN;
  const repository = process.env.GITHUB_REPOSITORY || contract.repository;
  if (!token) return ['GOVERNANCE_AUDIT_TOKEN is required for the live Administration:read audit'];
  if (repository !== contract.repository) return [`GITHUB_REPOSITORY must be ${contract.repository}; observed ${repository}`];

  const branchName = contract.source_authority.accepted_branch;
  const encodedBranch = encodeURIComponent(branchName);
  const branch = await githubJson(`/repos/${repository}/branches/${encodedBranch}`, token);
  if (branch?.protected !== true) errors.push('live main branch is not protected');

  let protection;
  try {
    protection = await githubJson(`/repos/${repository}/branches/${encodedBranch}/protection`, token);
  } catch (error) {
    errors.push(String(error instanceof Error ? error.message : error));
    protection = null;
  }

  if (protection) {
    const requiredStatus = protection.required_status_checks ?? {};
    if (requiredStatus.strict !== contract.main_governance.strict_required_status_checks) {
      errors.push('live main required status checks are not strict');
    }
    if (!sameStringSet(protectionCheckNames(protection), contract.main_governance.required_checks)) {
      errors.push('live main required status checks do not equal the contract');
    }
    if (protection?.enforce_admins?.enabled !== contract.main_governance.enforce_admins) {
      errors.push('live main enforce_admins does not match the contract');
    }
    if (!protection?.required_pull_request_reviews) {
      errors.push('live main does not require pull-request review flow');
    }
    if (protection?.required_conversation_resolution?.enabled !== contract.main_governance.require_conversation_resolution) {
      errors.push('live main required conversation resolution does not match the contract');
    }
    if (protection?.allow_force_pushes?.enabled !== contract.main_governance.allow_force_pushes) {
      errors.push('live main force-push policy does not match the contract');
    }
    if (protection?.allow_deletions?.enabled !== contract.main_governance.allow_deletions) {
      errors.push('live main deletion policy does not match the contract');
    }
  }

  for (const name of contract.promotion.hosted_environments) {
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

    const expected = contract.environments[name];
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
      errors.push(`live ${name}.can_admins_bypass does not match the contract`);
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
  const contract = await loadContract(root);

  if (command === 'contract') {
    const errors = validateContract(contract);
    if (!report(errors)) return 1;
    console.log('AR-7 GitHub governance contract is internally consistent.');
    return 0;
  }
  if (command === 'self-test') return selfTest(contract) ? 0 : 1;
  if (command === 'live') {
    const errors = await liveAudit(contract);
    if (!report(errors)) return 1;
    console.log('AR-7 live GitHub governance matches the checked-in contract.');
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
