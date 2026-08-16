# AR-7 — Environments + GitHub Governance + Operational Boundaries

**Document status:** EVIDENCE / AR-7 accepted  
**Architecture program:** Architecture Re-baseline v3  
**Implementation issue:** #298  
**Parent acceptance gate:** #268  
**Baseline `main`:** `dde7123586b080c1c053e90ad0ba489d4620e4d2`  
**Implementation exact-green head:** `1ebb9f42bb52cf86f1794667f5c9d630ce78e8a7`  
**Implementation merge:** `3492273cb9237850e3fa27343cc5edbdb0f66aa1`  
**Hosted governance run:** `31953316327`  
**Closeout issue:** #300  
**Production readiness:** `false`

## 1. Purpose

AR-7 makes the repository-to-production promotion boundary mechanically explicit. It does not
add application features and it does not authorize production readiness. The slice is accepted
only when the checked-in contract and the live hosted GitHub configuration agree and negative
paths fail closed.

The canonical machine-readable candidate is
[`architecture/github-governance-ar7.json`](../architecture/github-governance-ar7.json).
The permanent repository gate is
[`.github/workflows/github-governance-gate.yml`](../.github/workflows/github-governance-gate.yml),
with deterministic policy logic in
[`.github/scripts/github-governance.mjs`](../.github/scripts/github-governance.mjs).

## 2. Promotion authority

The only allowed promotion order is:

`dev -> rehearsal -> staging -> production`

`dev` is a local or pull-request candidate state. `rehearsal`, `staging`, and `production` are
hosted GitHub Environments. Promotion never changes source authority: accepted source is `main`,
production source is `refs/heads/main`, and the same immutable source bits move through hosted
environments. Rebuilding during promotion is forbidden.

All hosted deployment branch policies are exact `main` policies. Wildcards, tags, arbitrary
branches, and environment-specific rebuild branches are outside the AR-7 contract.

## 3. `main` governance contract

AR-7 deliberately chooses classic branch protection as the concrete mechanism for this slice so
that the acceptance semantics are explicit and directly auditable. `main` must:

- require pull-request flow;
- require conversation resolution;
- enforce the protection for administrators;
- require the exact permanent PR check contexts recorded in the JSON contract;
- require strict/up-to-date status checks;
- reject force pushes;
- reject branch deletion.

The required check list is based on the real successful check-run contexts observed on the AR-6
closeout candidate, plus the new `GitHub Governance Contract` check. Workflow display names are
not substituted for job/check-run contexts.

## 4. Hosted Environment contract

`rehearsal`, `staging`, and `production` must all exist and use an exact custom deployment branch
policy for `main`.

`production` additionally requires at least one deployment reviewer and
`can_admins_bypass=false`. AR-7 intentionally does not set `prevent_self_review=true`: the current
repository is operated by a single owner, so that setting would make the only production reviewer
incapable of completing the approval. The non-bypass requirement still ensures that production
promotion cannot silently skip the environment gate.

This solo-operator constraint is explicit rather than hidden. If an independent production
reviewer is added later, tightening self-review becomes a separate bounded governance change.

## 5. Permanent audit design

`GitHub Governance Gate` has two jobs with different trust boundaries:

1. **GitHub Governance Contract** runs on pull requests and accepted `main`. It uses no privileged
   credential and validates the checked-in contract plus negative fixtures.
2. **GitHub Governance Hosted State** runs only after source is on `main`, on the daily schedule,
   or by explicit workflow dispatch. It never runs on `pull_request`, so the privileged read-only
   audit credential is not exposed to candidate PR code.

The hosted audit requires repository Actions secret `GOVERNANCE_AUDIT_TOKEN`. It must be a
fine-grained credential restricted to this repository with the minimum read permissions needed to
read branch protection and Environments. The intended permissions are:

- repository **Administration: read** — required to read branch protection;
- repository **Actions: read** — required to read Environments and deployment branch policies.

No write permission is required by the audit. Missing token, missing API permission, missing
Environment, missing branch protection, or any semantic mismatch is a hard failure rather than a
skip.

## 6. Live drift at AR-7 start

The pre-implementation live read on 2026-08-16 proves AR-7 is not yet acceptable:

- `main` reports `protected=false`;
- repository rulesets are empty;
- `rehearsal` does not exist;
- `staging` exists with an exact `main` deployment branch policy;
- `production` exists with an exact `main` deployment branch policy and a reviewer;
- `production.can_admins_bypass=true`.

These are hosted GitHub settings, not repository files. Repository code must not claim to have
fixed them. Acceptance remains blocked until the live audit proves the hosted state matches the
contract.

## 7. Hosted configuration required before acceptance

Before this slice can leave candidate state, repository administration must:

1. create the `rehearsal` Environment and restrict it to `main`;
2. preserve exact `main` deployment branch policies for `staging` and `production`;
3. set `production.can_admins_bypass=false` while preserving at least one reviewer;
4. protect `main` with the exact contract semantics and required check contexts;
5. add the read-only `GOVERNANCE_AUDIT_TOKEN` Actions secret;
6. run the live governance audit successfully on accepted source;
7. perform and record a negative direct-update probe proving GitHub rejects an unauthorized direct
   update to `main`.

A successful repository-local or PR contract check is necessary but not sufficient. Hosted-state
proof and the negative direct-update probe are acceptance evidence.

## 8. Non-goals

AR-7 does not:

- add Terraform or a second infrastructure authority;
- redesign accepted Cloudflare immutable release provenance;
- add runtime/application product behavior;
- make production mutations on behalf of an operator;
- advance AR-8 or later slices;
- set `production_ready=true`.

## 9. Acceptance Definition of Done

The implementation candidate is ready for final AR-7 acceptance only when:

- the JSON contract validates and all negative fixtures fail closed;
- `GitHub Governance Contract` is green on one unchanged PR head;
- every other applicable permanent workflow is green on that same head;
- hosted GitHub configuration matches the contract;
- `GitHub Governance Hosted State` is green with the read-only credential;
- the direct-`main` negative probe is rejected and recorded;
- the branch is not behind accepted `main`;
- no blocking review or unresolved thread remains.

All conditions were simultaneously proven before closeout. The implementation head
`1ebb9f42bb52cf86f1794667f5c9d630ce78e8a7` passed **14/14 success** applicable permanent PR workflows,
merged as `3492273cb9237850e3fa27343cc5edbdb0f66aa1`, and the post-merge hosted audit run `31953316327`
completed both governance jobs successfully. The direct-main probe was rejected with **HTTP 409** and
the sentinel remained absent. AR-7 is therefore accepted; `architecture_complete=false`,
`production_core_gate = BLOCKED`, and `production_ready=false` remain unchanged. AR-8 is the next
required slice.

## 10. Accepted evidence

- implementation issue: #298;
- implementation PR: #299;
- exact-green candidate: `1ebb9f42bb52cf86f1794667f5c9d630ce78e8a7`;
- implementation merge: `3492273cb9237850e3fa27343cc5edbdb0f66aa1`;
- PR permanent workflows: **14/14 success** on one unchanged candidate;
- protected `main`: exact 21 required checks, PR-only flow, strict checks, conversation resolution,
  administrator enforcement, no force pushes, no deletion;
- hosted Environments: `rehearsal`, `staging`, `production`, each restricted exactly to `main`;
- production: one required reviewer and `can_admins_bypass=false`;
- hosted governance run `31953316327`: `GitHub Governance Contract` and
  `GitHub Governance Hosted State` both success;
- direct-main negative probe: **HTTP 409** rejected, no sentinel persisted;
- closeout authority issue: #300;
- production mutation: none.

