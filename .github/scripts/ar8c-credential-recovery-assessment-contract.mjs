import { readFile } from 'node:fs/promises';
import { invariant } from './ar8c-provider-bootstrap-common.mjs';

const WORKFLOW_PATH = '.github/workflows/ar8c-credential-recovery-assessment.yml';
const ASSESSMENT_PATH = '.github/scripts/ar8c-credential-recovery-assessment.mjs';
const CHECKOUT_PIN = 'actions/checkout@f548e57e544e1ff5a4c46bf1e1b8685f8e4a348a';
const NODE_PIN = 'actions/setup-node@48b55a011bda9f5d6aeb4c2d9c7362e8dae4041e';

function count(source, fragment) {
  return source.split(fragment).length - 1;
}

async function validate() {
  const [workflow, assessment] = await Promise.all([
    readFile(WORKFLOW_PATH, 'utf8'),
    readFile(ASSESSMENT_PATH, 'utf8'),
  ]);
  const combined = `${workflow}\n${assessment}`;

  invariant(workflow.startsWith('name: AR-8C Staging Credential Recovery Assessment\n'), 'credential recovery assessment workflow name drifted');
  invariant(count(workflow, 'workflow_dispatch:') === 1, 'credential recovery assessment must expose exactly one workflow_dispatch trigger');
  invariant(!/\bpull_request(?:_target)?:/i.test(workflow) && !/^\s*push:/m.test(workflow), 'credential recovery assessment must never run on PR/push');
  invariant(workflow.includes('permissions:\n  contents: read\n'), 'credential recovery assessment must keep repository contents read-only');
  invariant(workflow.includes('group: ar8c-staging-credential-recovery-assessment'), 'credential recovery assessment concurrency group drifted');
  invariant(workflow.includes('cancel-in-progress: false'), 'credential recovery assessment must not cancel an in-flight read-only assessment');
  invariant(workflow.includes("if: github.ref == 'refs/heads/main'"), 'credential recovery assessment must reject non-main dispatches before protected secret exposure');
  invariant(workflow.includes('environment: staging'), 'credential recovery assessment must use protected staging Environment');
  invariant(workflow.includes('AR8C_TARGET_ENVIRONMENT: staging'), 'credential recovery target must remain staging');
  invariant(workflow.includes(`uses: ${CHECKOUT_PIN}`) && workflow.includes(`uses: ${NODE_PIN}`), 'credential recovery action pins drifted');
  invariant(workflow.includes('ref: ${{ github.sha }}'), 'credential recovery checkout must bind to dispatched exact main SHA');
  invariant(workflow.includes("node-version: '24.19.0'") && workflow.includes('test "$(node --version)" = "v24.19.0"'), 'credential recovery Node runtime pin drifted');
  invariant(workflow.includes('secrets.CLOUDFLARE_BOOTSTRAP_TOKEN'), 'credential recovery assessment must use the protected read-capable bootstrap token');
  invariant(!workflow.includes('inputs:'), 'credential recovery assessment must not accept user-supplied values or secret material');
  invariant(workflow.indexOf('self-test') < workflow.indexOf('recovery-assess'), 'credential recovery self-test must precede live read-only assessment');

  for (const forbiddenBinding of [
    'GH_BOOTSTRAP_ADMIN_TOKEN',
    'CLOUDFLARE_TOKEN_ISSUER_TOKEN',
    'CLOUDFLARE_API_TOKEN',
    'CLOUDFLARE_RESOLVER_SECRETS_JSON',
    'CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON',
    'CLOUDFLARE_DEPLOY_MANIFEST_JSON',
  ]) {
    invariant(!workflow.includes(`secrets.${forbiddenBinding}`), `credential recovery assessment must not receive protected mutable/runtime binding ${forbiddenBinding}`);
  }

  invariant(assessment.includes("import { EXPECTED, cfRequest, invariant, secret } from './ar8c-provider-bootstrap-common.mjs'"), 'credential recovery assessment must reuse canonical bootstrap identity/config authority');
  invariant(assessment.includes("import { acceptedPrerequisites, discoverIdentity } from './ar8c-provider-bootstrap-provider.mjs'"), 'credential recovery assessment must resolve exact canonical D1 targets through accepted prerequisites');
  invariant(assessment.includes('prerequisites.resolverId') && assessment.includes('prerequisites.catalogId'), 'credential recovery assessment must query exact accepted resolver/catalog D1 ids');
  invariant(assessment.includes('resolver_encrypted_records') && assessment.includes('resolver_idempotency_records'), 'resolver durable key dependencies are not assessed');
  invariant(assessment.includes('resolver_key_rotation_runs'), 'resolver prior key lifecycle evidence is not assessed');
  invariant(assessment.includes('client_contact_points'), 'contact protection key dependencies are not assessed');
  invariant(assessment.includes('mailbox_bindings') && assessment.includes('mailbox_binding_create_commands'), 'catalog resolver-handle references are not assessed');
  invariant(assessment.includes('mailbox_onboarding_state') && assessment.includes('mailbox_onboarding_history') && assessment.includes('mailbox_onboarding_commands'), 'mailbox onboarding resolver-handle references are not assessed');
  invariant(assessment.includes("'KEY_PRESERVATION_REQUIRED'") && assessment.includes("'FRESH_PROJECT_SECRET_ISSUANCE_SAFE'"), 'credential recovery classification states drifted');
  invariant(assessment.includes("external_oauth_secret_authority: 'PROVIDER_OWNED_INPUT_REQUIRED'"), 'OAuth client secrets must remain external provider-owned inputs');
  invariant(assessment.includes('COUNT(*)') && assessment.includes('isReadOnlyAggregateSql'), 'credential recovery assessment must remain aggregate-only D1 evidence');
  invariant(assessment.includes("!isReadOnlyAggregateSql('DELETE FROM resolver_encrypted_records')"), 'mutating SQL negative fixture is missing');
  invariant(assessment.includes("!isReadOnlyAggregateSql('SELECT * FROM resolver_encrypted_records')"), 'row-content SQL negative fixture is missing');

  for (const forbiddenSurface of [
    'gh secret',
    'bindEnvironmentSecret',
    'issueR2Credential',
    'issueDeployToken',
    'reconcileAccess',
    'wrangler deploy',
    'wrangler secret',
    'terraform',
    '/workers/scripts/',
    '/access/service_tokens/',
  ]) {
    invariant(!combined.toLowerCase().includes(forbiddenSurface.toLowerCase()), `credential recovery assessment contains forbidden mutable surface: ${forbiddenSurface}`);
  }
  invariant(!combined.toLowerCase().includes('part-crm-catalog-production'), 'production target leaked into credential recovery assessment');

  console.log('AR-8C credential recovery assessment contract: PASS');
}

await validate();
