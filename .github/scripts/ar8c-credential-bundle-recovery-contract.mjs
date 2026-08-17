import { readFile } from 'node:fs/promises';
import { invariant } from './ar8c-provider-bootstrap-common.mjs';

const WORKFLOW_PATH = '.github/workflows/ar8c-credential-bundle-recovery.yml';
const EXECUTOR_PATH = '.github/scripts/ar8c-credential-bundle-recovery.mjs';
const ASSESSMENT_PATH = '.github/scripts/ar8c-credential-recovery-assessment.mjs';
const PROVIDER_PATH = '.github/scripts/ar8c-provider-bootstrap-provider.mjs';
const COMMON_PATH = '.github/scripts/ar8c-provider-bootstrap-common.mjs';
const CREDENTIAL_AUTHORITY_PATH = 'architecture/credential-authority-ar8b.json';
const CHECKOUT_PIN = 'actions/checkout@f548e57e544e1ff5a4c46bf1e1b8685f8e4a348a';
const NODE_PIN = 'actions/setup-node@48b55a011bda9f5d6aeb4c2d9c7362e8dae4041e';

function count(source, fragment) {
  return source.split(fragment).length - 1;
}

function exactCredential(authority, id) {
  const credentials = Array.isArray(authority?.credentials) ? authority.credentials : [];
  const matches = credentials.filter((entry) => entry?.id === id);
  invariant(matches.length === 1, `canonical credential authority must contain exactly one ${id} entry`);
  return matches[0];
}

function assertCanonicalOAuthCredential(entry, expectedBinding, providerSystem) {
  invariant(entry.class === 'OAUTH_APPLICATION_CREDENTIAL', `${entry.id} class drifted`);
  invariant(entry.provider_system === providerSystem, `${entry.id} provider drifted`);
  invariant(entry.externally_issued === true, `${entry.id} must remain provider-issued`);
  invariant(entry.future_cutover === 'AR-8E', `${entry.id} lifecycle ownership must remain AR-8E`);
  const names = (Array.isArray(entry.bindings) ? entry.bindings : []).map((binding) => binding?.name);
  invariant(names.includes(expectedBinding), `${entry.id} canonical binding ${expectedBinding} is missing`);
}

async function validate() {
  const [workflow, executor, assessment, provider, common, authorityRaw] = await Promise.all([
    readFile(WORKFLOW_PATH, 'utf8'),
    readFile(EXECUTOR_PATH, 'utf8'),
    readFile(ASSESSMENT_PATH, 'utf8'),
    readFile(PROVIDER_PATH, 'utf8'),
    readFile(COMMON_PATH, 'utf8'),
    readFile(CREDENTIAL_AUTHORITY_PATH, 'utf8'),
  ]);
  const authority = JSON.parse(authorityRaw);

  invariant(workflow.startsWith('name: AR-8C Staging Credential Bundle Recovery\n'), 'credential bundle recovery workflow name drifted');
  invariant(count(workflow, 'workflow_dispatch:') === 1, 'credential bundle recovery must expose exactly one workflow_dispatch trigger');
  invariant(!/\bpull_request(?:_target)?:/i.test(workflow) && !/^\s*push:/m.test(workflow), 'credential bundle recovery must never run on PR/push');
  invariant(!workflow.includes('inputs:'), 'credential bundle recovery must not accept dispatch inputs');
  invariant(workflow.includes('permissions:\n  contents: read\n'), 'credential bundle recovery must keep repository contents read-only');
  invariant(workflow.includes('group: ar8c-staging-credential-bundle-recovery'), 'credential bundle recovery concurrency group drifted');
  invariant(workflow.includes('cancel-in-progress: false'), 'credential bundle recovery must not cancel an in-flight recovery');
  invariant(workflow.includes("if: github.ref == 'refs/heads/main'"), 'credential bundle recovery must reject non-main dispatches before protected secret exposure');
  invariant(workflow.includes('environment: staging'), 'credential bundle recovery must use protected staging Environment');
  invariant(workflow.includes('AR8C_TARGET_ENVIRONMENT: staging'), 'credential bundle recovery target must remain staging');
  invariant(workflow.includes(`uses: ${CHECKOUT_PIN}`) && workflow.includes(`uses: ${NODE_PIN}`), 'credential bundle recovery action pins drifted');
  invariant(workflow.includes('ref: ${{ github.sha }}'), 'credential bundle recovery checkout must bind to dispatched exact main SHA');
  invariant(workflow.includes("node-version: '24.19.0'") && workflow.includes('test "$(node --version)" = "v24.19.0"'), 'credential bundle recovery Node runtime pin drifted');

  const referencedSecrets = [...workflow.matchAll(/secrets\.([A-Z0-9_]+)/g)].map((match) => match[1]).sort();
  const expectedSecrets = [
    'CLOUDFLARE_BOOTSTRAP_TOKEN',
    'CLOUDFLARE_TOKEN_ISSUER_TOKEN',
    'GH_BOOTSTRAP_ADMIN_TOKEN',
    'GOOGLE_OAUTH_CLIENT_SECRET',
    'MICROSOFT_OAUTH_CLIENT_SECRET',
  ].sort();
  invariant(JSON.stringify(referencedSecrets) === JSON.stringify(expectedSecrets), 'credential bundle recovery protected input set drifted');
  for (const forbidden of [
    'CLOUDFLARE_RESOLVER_SECRETS_JSON',
    'CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON',
    'CLOUDFLARE_API_TOKEN',
    'CLOUDFLARE_ACCESS_CLIENT_ID',
    'CLOUDFLARE_ACCESS_CLIENT_SECRET',
    'CLOUDFLARE_DEPLOY_MANIFEST_JSON',
  ]) {
    invariant(!workflow.includes(`secrets.${forbidden}`), `credential bundle recovery must not read final/runtime binding ${forbidden}`);
  }

  assertCanonicalOAuthCredential(
    exactCredential(authority, 'resolver.google-oauth-application'),
    'GOOGLE_OAUTH_CLIENT_SECRET',
    'GOOGLE_OAUTH',
  );
  assertCanonicalOAuthCredential(
    exactCredential(authority, 'resolver.microsoft-oauth-application'),
    'MICROSOFT_OAUTH_CLIENT_SECRET',
    'MICROSOFT_IDENTITY',
  );
  invariant(!executor.includes('AR8C_RECOVERY_GOOGLE_OAUTH_CLIENT_SECRET') && !executor.includes('AR8C_RECOVERY_MICROSOFT_OAUTH_CLIENT_SECRET'), 'recovery must not create alias OAuth credential identities');

  const assessmentCall = 'runCanonicalRecoveryAssessment()';
  const providerClassification = 'await classifyBeforeMutation({';
  const r2Issuance = 'await issueR2Credential({';
  const firstFinalWrite = 'bindEnvironmentSecret(ghToken, FINAL_RECOVERY_BINDINGS[0]';
  invariant(executor.indexOf(assessmentCall) < executor.indexOf(providerClassification), 'canonical zero-state assessment must precede provider classification');
  invariant(executor.indexOf(providerClassification) < executor.indexOf(r2Issuance), 'provider classification must precede R2 credential reissue');
  invariant(executor.indexOf(r2Issuance) < executor.indexOf(firstFinalWrite), 'R2 credential reissue must precede final bundle writes');
  invariant(executor.includes("spawnSync(process.execPath, [ASSESSMENT_SCRIPT, 'recovery-assess']"), 'recovery executor must reuse canonical assessment process');
  invariant(executor.includes("evidence?.classification === 'FRESH_PROJECT_SECRET_ISSUANCE_SAFE'"), 'recovery must require fresh-issuance-safe classification');
  invariant(executor.includes("evidence?.external_oauth_secret_authority === 'PROVIDER_OWNED_INPUT_REQUIRED'"), 'recovery must preserve provider-owned OAuth secret authority');
  invariant(assessment.includes('classification: keyPreservationRequired') && assessment.includes("'KEY_PRESERVATION_REQUIRED'") && assessment.includes("'FRESH_PROJECT_SECRET_ISSUANCE_SAFE'"), 'canonical assessment classification states drifted');

  invariant(executor.includes("'CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON'") && executor.includes("'CLOUDFLARE_RESOLVER_SECRETS_JSON'"), 'recovery final binding set is incomplete');
  invariant(executor.includes('JSON.stringify(FINAL_RECOVERY_BINDINGS)'), 'recovery final binding set lacks self-test coverage');
  invariant(count(executor, 'bindEnvironmentSecret(ghToken, FINAL_RECOVERY_BINDINGS[') === 2, 'recovery must perform exactly two canonical final Environment writes');
  invariant(executor.includes('deleteRecoveryInputBinding(ghToken, name)'), 'consumed temporary OAuth bindings must be removed after successful final writes');
  invariant(executor.includes("['secret', 'delete', name, '--env', EXPECTED.environment, '--repo', EXPECTED.repository]"), 'recovery cleanup must be restricted to exact staging Environment secret deletion');
  invariant(executor.includes('RECOVERY_INPUT_BINDINGS.includes(name)'), 'recovery cleanup must fail closed for non-recovery binding names');
  invariant(executor.includes("'GOOGLE_OAUTH_CLIENT_SECRET'") && executor.includes("'MICROSOFT_OAUTH_CLIENT_SECRET'"), 'recovery must use canonical OAuth credential identities');

  invariant(executor.includes('CLIENT_CONTACT_PROTECTION_KEYRING') && executor.includes('MAILBOX_RESOLVER_ENCRYPTION_KEYRING'), 'project-generated keyring material is incomplete');
  invariant(executor.includes('MAILBOX_RESOLVER_CALLER_AUTH_KEY') && executor.includes('MAILBOX_RESOLVER_HANDLE_HMAC_KEY'), 'project-generated resolver authentication/HMAC material is incomplete');
  invariant(executor.includes('randomBytesImpl(32)') && executor.includes("toString('hex')"), 'project-generated secrets must use independent 32-byte CSPRNG values encoded as hex');
  invariant(executor.includes('activeVersion: 1') && executor.includes('keys: [{ version: 1, keyHex: resolverEncryption }]'), 'resolver encryption keyring runtime format drifted');
  invariant(executor.includes('encryption: [{ version: 1, keyHex: contactEncryption }]') && executor.includes('lookup: [{ version: 1, keyHex: contactLookup }]'), 'contact protection keyring runtime format drifted');
  invariant(executor.includes('parseExactBundle(JSON.stringify(controlBundle), CONTROL_PLANE_KEYS') && executor.includes('parseExactBundle(JSON.stringify(resolverBundle), RESOLVER_KEYS'), 'generated recovery bundles must pass canonical exact-key validation');

  for (const forbiddenSurface of [
    'issueDeployToken',
    'reconcileAccess',
    'wrangler deploy',
    'wrangler secret',
    'terraform',
    '/workers/scripts/',
    '/access/service_tokens/',
    "method: 'DELETE'",
    'part-crm-catalog-production',
  ]) {
    invariant(!executor.toLowerCase().includes(forbiddenSurface.toLowerCase()), `credential bundle recovery contains forbidden mutable surface: ${forbiddenSurface}`);
  }
  invariant(provider.includes('issueR2Credential') && provider.includes(`/user/tokens/${'${existing.id}'}/value`), 'canonical R2 credential owning authority is unavailable');
  invariant(common.includes("gh', ['secret', 'set'") && !common.includes("'--body'") && common.includes('input: value'), 'canonical Environment write-only binding contract drifted');

  console.log('AR-8C credential bundle recovery execution contract: PASS');
}

await validate();
