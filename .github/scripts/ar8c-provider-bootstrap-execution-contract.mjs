import { readFile } from 'node:fs/promises';
import {
  R2_CREDENTIAL_VALIDATION_POLICY,
  invariant,
  isRetryableR2ValidationStatus,
  r2CredentialValidationDelayMs,
  validateR2Credential,
} from './ar8c-provider-bootstrap-common.mjs';
import {
  GITHUB_READ_RETRY_POLICY,
  githubReadJson,
  githubReadRetryDelayMs,
  isRetryableGitHubReadStatus,
  isRetryableGitHubTransportError,
} from './ar8c-github-read-retry.mjs';

const WORKFLOW_PATH = '.github/workflows/ar8c-provider-bootstrap-execution.yml';
const PREFLIGHT_PATH = '.github/scripts/ar8c-staging-bootstrap-preflight.mjs';
const EXECUTION_PATH = '.github/scripts/ar8c-provider-bootstrap-execution.mjs';
const PROVIDER_PATH = '.github/scripts/ar8c-provider-bootstrap-provider.mjs';
const COMMON_PATH = '.github/scripts/ar8c-provider-bootstrap-common.mjs';
const RETRY_PATH = '.github/scripts/ar8c-github-read-retry.mjs';
const CHECKOUT_PIN = 'actions/checkout@f548e57e544e1ff5a4c46bf1e1b8685f8e4a348a';
const NODE_PIN = 'actions/setup-node@48b55a011bda9f5d6aeb4c2d9c7362e8dae4041e';

function count(source, fragment) { return source.split(fragment).length - 1; }

async function validate() {
  const [workflow, preflight, execution, provider, common, retry] = await Promise.all([
    readFile(WORKFLOW_PATH, 'utf8'), readFile(PREFLIGHT_PATH, 'utf8'), readFile(EXECUTION_PATH, 'utf8'),
    readFile(PROVIDER_PATH, 'utf8'), readFile(COMMON_PATH, 'utf8'), readFile(RETRY_PATH, 'utf8'),
  ]);
  const providerCombined = `${workflow}\n${execution}\n${provider}\n${common}\n${retry}`;

  invariant(workflow.startsWith('name: AR-8C Staging Provider Bootstrap Execution\n'), 'provider bootstrap workflow name drifted');
  invariant(count(workflow, 'workflow_dispatch:') === 1, 'provider bootstrap workflow must expose exactly one workflow_dispatch trigger');
  invariant(!/\bpull_request(?:_target)?:/i.test(workflow) && !/^\s*push:/m.test(workflow), 'privileged provider bootstrap must never run on PR/push');
  invariant(workflow.includes('permissions:\n  contents: read\n'), 'provider bootstrap workflow must keep repository contents read-only');
  invariant(workflow.includes('group: ar8c-staging-provider-bootstrap-execution'), 'provider bootstrap concurrency group drifted');
  invariant(workflow.includes('cancel-in-progress: false'), 'provider bootstrap must not cancel an in-flight credential handoff');
  invariant(workflow.includes("if: github.ref == 'refs/heads/main'"), 'provider bootstrap must reject non-main dispatches before secret exposure');
  invariant(workflow.includes('environment: staging'), 'provider bootstrap must use protected staging Environment');
  invariant(workflow.includes('AR8C_TARGET_ENVIRONMENT: staging'), 'target environment must be staging');
  invariant(workflow.includes(`uses: ${CHECKOUT_PIN}`) && workflow.includes(`uses: ${NODE_PIN}`), 'action pins drifted');
  invariant(workflow.includes('ref: ${{ github.sha }}'), 'checkout must bind to dispatched exact main SHA');
  invariant(workflow.includes("node-version: '24.19.0'") && workflow.includes('test "$(node --version)" = "v24.19.0"'), 'Node runtime pin drifted');

  for (const binding of ['GH_BOOTSTRAP_ADMIN_TOKEN', 'CLOUDFLARE_BOOTSTRAP_TOKEN', 'CLOUDFLARE_TOKEN_ISSUER_TOKEN', 'CLOUDFLARE_RESOLVER_SECRETS_JSON', 'CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON']) {
    invariant(workflow.includes(`secrets.${binding}`), `protected bootstrap input is missing: ${binding}`);
  }
  invariant(!workflow.includes('secrets.CLOUDFLARE_API_TOKEN'), 'steady-state CLOUDFLARE_API_TOKEN must not authenticate bootstrap execution');
  for (const input of ['access_issuer:', 'google_oauth_client_id:', 'microsoft_oauth_client_id:']) {
    invariant(workflow.includes(input), `required public dispatch input is missing: ${input}`);
  }
  invariant(workflow.includes('AR8C_ACCESS_ISSUER: ${{ inputs.access_issuer }}'), 'Access issuer must flow only from the explicit public dispatch input');
  invariant(workflow.indexOf('ar8c-staging-bootstrap-preflight.mjs') < workflow.indexOf('ar8c-staging-d1-classification.mjs'), 'read-only preflight must precede D1 classification');
  invariant(workflow.indexOf('ar8c-staging-d1-classification.mjs') < workflow.indexOf('ar8c-provider-bootstrap-execution.mjs execute'), 'all read-only checks must precede provider mutation');

  invariant(!preflight.includes("githubRequest(token, '/user')"), 'GitHub bootstrap preflight must not depend on token-flavor-specific GET /user');
  invariant(preflight.includes('environments/${EXPECTED.environment}/secrets?per_page=1'), 'GitHub bootstrap preflight must verify staging Environment secret metadata capability');
  invariant(preflight.includes('environments/${EXPECTED.environment}/secrets/public-key'), 'GitHub bootstrap preflight must verify staging Environment public-key capability');
  invariant(!preflight.includes('GitHub bootstrap principal:'), 'preflight must not require or emit a user principal identity');

  invariant(execution.includes("process.env.GITHUB_REF_NAME === EXPECTED.refName") && execution.includes("process.env.GITHUB_EVENT_NAME === EXPECTED.eventName"), 'runtime accepted-main/workflow_dispatch boundary checks are missing');
  invariant(execution.includes("publicInput('AR8C_ACCESS_ISSUER')") && execution.includes(".cloudflareaccess.com"), 'Access issuer strict public-input validation is missing');
  invariant(provider.indexOf('classifyBeforeMutation') < provider.indexOf('issueR2Credential'), 'provider module must expose classification before first credential mutation');
  invariant(execution.indexOf('classifyBeforeMutation') < execution.indexOf('issueR2Credential'), 'execution must classify provider state before first mutation');
  invariant(provider.includes("/user/tokens/${existing.id}/value") && provider.includes('/access/service_tokens/${existing.id}/rotate'), 'recovery-safe token rotation surfaces are missing');
  invariant(common.includes("createHash('sha256').update(tokenValue") && common.includes('validateR2Credential'), 'R2 derivation/bounded validation contract is missing');
  invariant(provider.indexOf("cfRequest(tokenValue, '/user/tokens/verify')") < provider.indexOf('await validateR2Credential({ accountId, ...result })'), 'R2 API token must verify active before S3 credential validation');
  invariant(common.includes('R2_CREDENTIAL_VALIDATION_POLICY') && common.includes('isRetryableR2ValidationStatus'), 'bounded R2 validation retry policy is missing');
  invariant(common.includes('SignatureDoesNotMatch') && common.includes('R2_ACTIVATION_PROPAGATION_DELAY_MS'), 'bounded R2 activation propagation handling is missing');
  invariant(common.includes("gh', ['secret', 'set'") && !common.includes("'--body'") && common.includes('input: value'), 'GitHub Environment secret binding must feed exact value on stdin with --body omitted');
  invariant(common.includes('githubReadJson') && !common.includes('retryEnvironmentSecret'), 'GitHub retry helper must remain scoped to read requests, not Environment writes');
  invariant(execution.includes('verifyEnvironmentBindings') && execution.includes('FINAL_ENV_BINDINGS'), 'post-bind metadata verification is missing');
  invariant(execution.includes('CLOUDFLARE_API_TOKEN') && execution.includes('CLOUDFLARE_DEPLOY_MANIFEST_JSON'), 'steady-state handoff outputs are incomplete');
  invariant(execution.includes('buildManifest') && execution.includes('schema_version: 1'), 'canonical deploy manifest construction is missing');
  invariant(common.includes("hostname: 'staging.alegria.by'") && !JSON.stringify({ common, execution, provider }).toLowerCase().includes('part-crm-catalog-production'), 'production target leaked into provider bootstrap implementation');
  invariant(!providerCombined.includes('/access/organizations'), 'provider bootstrap must not widen the accepted bootstrap token with Access Organizations read');

  invariant(GITHUB_READ_RETRY_POLICY.maxAttempts === 5, 'GitHub read retry attempt bound drifted');
  invariant(GITHUB_READ_RETRY_POLICY.baseDelayMs === 1_000 && GITHUB_READ_RETRY_POLICY.maxDelayMs === 8_000, 'GitHub read retry backoff bounds drifted');
  for (const status of [429, 500, 503, 599]) invariant(isRetryableGitHubReadStatus(status), `GitHub HTTP ${status} must remain retryable`);
  for (const status of [400, 401, 403, 404, 409, 422]) invariant(!isRetryableGitHubReadStatus(status), `GitHub HTTP ${status} must remain fail-fast`);
  invariant(isRetryableGitHubTransportError(new TypeError('fixture')), 'GitHub transport TypeError must remain retryable');
  invariant(!isRetryableGitHubTransportError(new Error('fixture')), 'ordinary GitHub errors must remain fail-fast');
  invariant(githubReadRetryDelayMs(null, 1, 0) === 1_000 && githubReadRetryDelayMs(null, 4, 0) === 8_000, 'GitHub read retry exponential backoff drifted');
  const retryAfterHeaders = new Headers({ 'Retry-After': '30' });
  invariant(githubReadRetryDelayMs({ headers: retryAfterHeaders }, 1, 0) === 8_000, 'GitHub Retry-After must remain capped');

  let retryCalls = 0;
  const retrySleeps = [];
  const retryResult = await githubReadJson({
    token: 'fixture-token-not-a-secret', path: '/fixture', apiVersion: '2026-03-10', userAgent: 'ar8c-contract-fixture',
    fetchImpl: async () => {
      retryCalls += 1;
      if (retryCalls < 3) return new Response(JSON.stringify({ message: 'transient' }), { status: 503 });
      return new Response(JSON.stringify({ ok: true }), { status: 200 });
    },
    sleepImpl: async (ms) => { retrySleeps.push(ms); },
  });
  invariant(retryResult?.ok === true && retryCalls === 3, 'GitHub 5xx read retry loop did not recover deterministically');
  invariant(JSON.stringify(retrySleeps) === JSON.stringify([1_000, 2_000]), 'GitHub 5xx read retry backoff sequence drifted');

  let failFastCalls = 0;
  let failFastError = null;
  try {
    await githubReadJson({
      token: 'fixture-token-not-a-secret', path: '/fixture', apiVersion: '2026-03-10', userAgent: 'ar8c-contract-fixture',
      fetchImpl: async () => {
        failFastCalls += 1;
        return new Response(JSON.stringify({ message: 'forbidden' }), { status: 403 });
      },
      sleepImpl: async () => { throw new Error('non-retryable GitHub 4xx must not sleep'); },
    });
  } catch (error) {
    failFastError = error;
  }
  invariant(failFastCalls === 1 && /HTTP 403/.test(failFastError?.message ?? ''), 'GitHub non-retryable 4xx must fail on the first attempt');

  invariant(R2_CREDENTIAL_VALIDATION_POLICY.maxAttempts === 5, 'R2 validation retry attempt bound drifted');
  invariant(R2_CREDENTIAL_VALIDATION_POLICY.baseDelayMs === 1_000 && R2_CREDENTIAL_VALIDATION_POLICY.maxDelayMs === 8_000, 'R2 validation retry backoff bounds drifted');
  for (const status of [401, 429, 500, 503, 599]) invariant(isRetryableR2ValidationStatus(status), `R2 validation HTTP ${status} must remain retryable`);
  for (const status of [400, 403, 404, 409, 422]) invariant(!isRetryableR2ValidationStatus(status), `R2 validation HTTP ${status} must remain fail-fast`);
  invariant(r2CredentialValidationDelayMs(1) === 1_000 && r2CredentialValidationDelayMs(4) === 8_000, 'R2 validation exponential backoff drifted');

  let r2RetryCalls = 0;
  const r2RetrySleeps = [];
  await validateR2Credential({
    accountId: 'a'.repeat(32),
    accessKeyId: 'b'.repeat(32),
    secretAccessKey: 'c'.repeat(64),
    nowImpl: () => new Date('2026-08-17T16:00:00.000Z'),
    fetchImpl: async (url, options) => {
      r2RetryCalls += 1;
      invariant(url.includes('.r2.cloudflarestorage.com/part-crm-profile-objects-staging-d3?'), 'R2 validation target drifted');
      invariant(String(options?.headers?.Authorization ?? '').startsWith('AWS4-HMAC-SHA256 Credential='), 'R2 validation must remain SigV4 authenticated');
      if (r2RetryCalls < 3) return new Response('<Error><Code>Unauthorized</Code></Error>', { status: 401 });
      return new Response('', { status: 200 });
    },
    sleepImpl: async (ms) => { r2RetrySleeps.push(ms); },
  });
  invariant(r2RetryCalls === 3, 'R2 transient 401 validation retry loop did not recover deterministically');
  invariant(JSON.stringify(r2RetrySleeps) === JSON.stringify([1_000, 2_000]), 'R2 validation retry backoff sequence drifted');

  let r2PropagationCalls = 0;
  const r2PropagationSleeps = [];
  await validateR2Credential({
    accountId: 'a'.repeat(32),
    accessKeyId: 'b'.repeat(32),
    secretAccessKey: 'c'.repeat(64),
    nowImpl: () => new Date('2026-08-17T16:00:00.000Z'),
    fetchImpl: async () => {
      r2PropagationCalls += 1;
      if (r2PropagationCalls < 3) return new Response('<Error><Code>SignatureDoesNotMatch</Code></Error>', { status: 403 });
      return new Response('', { status: 200 });
    },
    sleepImpl: async (ms) => { r2PropagationSleeps.push(ms); },
  });
  invariant(r2PropagationCalls === 3, 'R2 post-rotation SignatureDoesNotMatch window did not recover deterministically');
  invariant(JSON.stringify(r2PropagationSleeps) === JSON.stringify([15_000, 15_000]), 'R2 activation propagation delay sequence drifted');

  let r2FailFastCalls = 0;
  let r2FailFastError = null;
  try {
    await validateR2Credential({
      accountId: 'a'.repeat(32),
      accessKeyId: 'b'.repeat(32),
      secretAccessKey: 'c'.repeat(64),
      nowImpl: () => new Date('2026-08-17T16:00:00.000Z'),
      fetchImpl: async () => {
        r2FailFastCalls += 1;
        return new Response('<Error><Code>AccessDenied</Code></Error>', { status: 403 });
      },
      sleepImpl: async () => { throw new Error('non-retryable R2 403 must not sleep'); },
    });
  } catch (error) {
    r2FailFastError = error;
  }
  invariant(r2FailFastCalls === 1 && /HTTP 403 \(AccessDenied\)/.test(r2FailFastError?.message ?? ''), 'R2 AccessDenied must fail on the first attempt with safe error code');

  for (const forbidden of ['wrangler deploy', 'wrangler secret', 'terraform', "method: 'DELETE'", '/workers/scripts/', '/workers/routes']) {
    invariant(!providerCombined.toLowerCase().includes(forbidden.toLowerCase()), `provider bootstrap contains forbidden mutable surface: ${forbidden}`);
  }
  console.log('AR-8C staging provider bootstrap execution contract: PASS');
}

await validate();