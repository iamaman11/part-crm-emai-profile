import { appendFile } from 'node:fs/promises';
import {
  GITHUB_READ_RETRY_POLICY,
  githubReadJson,
  githubReadRetryDelayMs,
  isRetryableGitHubReadStatus,
  isRetryableGitHubTransportError,
} from './ar8c-github-read-retry.mjs';

const EXPECTED = Object.freeze({
  repository: 'iamaman11/part-crm-emai-profile',
  refName: 'main',
  eventName: 'workflow_dispatch',
  environment: 'staging',
  accountName: "Pvisakp@gmail.com's Account",
  zoneName: 'alegria.by',
  stagingHostname: 'staging.alegria.by',
});

const CLOUDFLARE_API = 'https://api.cloudflare.com/client/v4';
const GITHUB_API_VERSION = '2026-03-10';

function invariant(condition, message) {
  if (!condition) throw new Error(message);
}

function secret(name) {
  const value = process.env[name];
  invariant(typeof value === 'string' && value.trim().length >= 20, `${name} is missing or unusable in the protected staging environment`);
  return value.trim();
}

function safeText(value) {
  if (typeof value !== 'string' || value.length === 0) return null;
  return value.replace(/[\r\n\t]/g, ' ').slice(0, 300);
}

function sanitizeApiErrors(payload) {
  const errors = Array.isArray(payload?.errors) ? payload.errors : [];
  return errors.slice(0, 5).map((entry) => ({
    code: entry?.code ?? null,
    message: typeof entry?.message === 'string' ? entry.message.slice(0, 300) : 'unknown error',
  }));
}

async function cloudflareRequest(token, path) {
  const response = await fetch(`${CLOUDFLARE_API}${path}`, {
    headers: {
      Authorization: `Bearer ${token}`,
      Accept: 'application/json',
    },
    signal: AbortSignal.timeout(20_000),
  });

  let payload;
  try {
    payload = await response.json();
  } catch {
    throw new Error(`Cloudflare ${path} returned non-JSON HTTP ${response.status}`);
  }

  if (!response.ok || payload?.success !== true) {
    throw new Error(`Cloudflare ${path} failed with HTTP ${response.status}: ${JSON.stringify(sanitizeApiErrors(payload))}`);
  }
  return payload;
}

async function githubRequest(token, path) {
  return githubReadJson({
    token,
    path,
    apiVersion: GITHUB_API_VERSION,
    userAgent: 'part-crm-ar8c-bootstrap-preflight',
  });
}

async function verifyCloudflareToken(token, label) {
  const payload = await cloudflareRequest(token, '/user/tokens/verify');
  invariant(payload?.result?.status === 'active', `${label} is not active`);
  invariant(typeof payload?.result?.id === 'string' && payload.result.id.length > 0, `${label} verify response has no token id`);
  return payload.result.id;
}

async function discoverExactZone(token) {
  const params = new URLSearchParams({ name: EXPECTED.zoneName, per_page: '50' });
  let payload;
  try {
    payload = await cloudflareRequest(token, `/zones?${params.toString()}`);
  } catch (error) {
    throw new Error(`${error.message}. Exact zone/account discovery requires Cloudflare Zone / Zone / Read on alegria.by; do not guess IDs.`);
  }

  const zones = Array.isArray(payload?.result) ? payload.result : [];
  const matches = zones.filter((zone) => zone?.name === EXPECTED.zoneName && zone?.account?.name === EXPECTED.accountName);
  invariant(matches.length === 1, `Expected exactly one active ${EXPECTED.zoneName} zone in ${EXPECTED.accountName}; got ${matches.length}`);

  const zone = matches[0];
  invariant(zone?.status === 'active', `${EXPECTED.zoneName} exists but is not active`);
  invariant(/^[0-9a-f]{32}$/.test(zone?.id ?? ''), 'Discovered Cloudflare zone id has an invalid shape');
  invariant(/^[0-9a-f]{32}$/.test(zone?.account?.id ?? ''), 'Discovered Cloudflare account id has an invalid shape');
  return { zoneId: zone.id, accountId: zone.account.id };
}

function normalizedArray(payload, nestedKey = null) {
  if (nestedKey && Array.isArray(payload?.result?.[nestedKey])) return payload.result[nestedKey];
  return Array.isArray(payload?.result) ? payload.result : [];
}

function sortByStableIdentity(items) {
  return items.sort((left, right) => {
    const leftKey = `${left.name ?? ''}\u0000${left.id ?? ''}\u0000${left.pattern ?? ''}`;
    const rightKey = `${right.name ?? ''}\u0000${right.id ?? ''}\u0000${right.pattern ?? ''}`;
    return leftKey.localeCompare(rightKey, 'en');
  });
}

async function listR2Buckets(token, accountId) {
  const buckets = [];
  let cursor = null;
  const seenCursors = new Set();

  for (let page = 0; page < 50; page += 1) {
    const params = new URLSearchParams({ per_page: '100', order: 'name', direction: 'asc' });
    if (cursor) params.set('cursor', cursor);
    const payload = await cloudflareRequest(token, `/accounts/${accountId}/r2/buckets?${params.toString()}`);
    buckets.push(...normalizedArray(payload, 'buckets'));
    const nextCursor = safeText(payload?.result_info?.cursor);
    if (!nextCursor) break;
    invariant(!seenCursors.has(nextCursor), 'R2 inventory pagination returned a repeated cursor');
    seenCursors.add(nextCursor);
    cursor = nextCursor;
    invariant(page < 49, 'R2 inventory exceeded the bounded 50-page discovery limit');
  }

  return buckets;
}

function normalizeProviderInventory({ accountId, zoneId, workers, d1, queues, r2, accessApps, serviceTokens, routes }) {
  const inventory = {
    schema_version: 1,
    environment: EXPECTED.environment,
    account: { name: EXPECTED.accountName, id: accountId },
    zone: { name: EXPECTED.zoneName, id: zoneId },
    staging_hostname_authority: EXPECTED.stagingHostname,
    workers_scripts: sortByStableIdentity(workers.map((item) => ({
      name: safeText(item?.id),
      created_on: safeText(item?.created_on),
      modified_on: safeText(item?.modified_on),
    })).filter((item) => item.name)),
    d1_databases: sortByStableIdentity(d1.map((item) => ({
      name: safeText(item?.name),
      id: safeText(item?.uuid),
      jurisdiction: safeText(item?.jurisdiction),
      created_at: safeText(item?.created_at),
    })).filter((item) => item.name || item.id)),
    queues: sortByStableIdentity(queues.map((item) => ({
      name: safeText(item?.queue_name ?? item?.name),
      id: safeText(item?.queue_id ?? item?.id),
      created_on: safeText(item?.created_on),
      modified_on: safeText(item?.modified_on),
    })).filter((item) => item.name || item.id)),
    r2_buckets: sortByStableIdentity(r2.map((item) => ({
      name: safeText(item?.name),
      location: safeText(item?.location),
      storage_class: safeText(item?.storage_class),
      creation_date: safeText(item?.creation_date),
    })).filter((item) => item.name)),
    access_applications: sortByStableIdentity(accessApps.map((item) => ({
      name: safeText(item?.name),
      id: safeText(item?.id),
      domain: safeText(item?.domain),
      aud: safeText(item?.aud),
      type: safeText(item?.type),
    })).filter((item) => item.name || item.id || item.domain)),
    access_service_tokens: sortByStableIdentity(serviceTokens.map((item) => ({
      name: safeText(item?.name),
      id: safeText(item?.id),
      expires_at: safeText(item?.expires_at),
    })).filter((item) => item.name || item.id)),
    workers_routes: sortByStableIdentity(routes.map((item) => ({
      id: safeText(item?.id),
      pattern: safeText(item?.pattern),
      script: safeText(item?.script),
    })).filter((item) => item.id || item.pattern || item.script)),
  };

  const serialized = JSON.stringify(inventory);
  invariant(!/(client_secret|api_token|authorization|secret_access_key|caller_auth_key|encryption_keyring|hmac_key)/i.test(serialized), 'provider inventory contains a forbidden secret-bearing field name');
  return inventory;
}

async function discoverProviderInventory(token, { accountId, zoneId }) {
  let payloads;
  try {
    payloads = await Promise.all([
      cloudflareRequest(token, `/accounts/${accountId}/workers/scripts`),
      cloudflareRequest(token, `/accounts/${accountId}/d1/database?per_page=10000`),
      cloudflareRequest(token, `/accounts/${accountId}/queues`),
      listR2Buckets(token, accountId),
      cloudflareRequest(token, `/accounts/${accountId}/access/apps?per_page=100`),
      cloudflareRequest(token, `/accounts/${accountId}/access/service_tokens?per_page=100`),
      cloudflareRequest(token, `/zones/${zoneId}/workers/routes`),
    ]);
  } catch (error) {
    throw new Error(`Cloudflare read-only inventory discovery failed: ${error.message}`);
  }

  const [workersPayload, d1Payload, queuesPayload, r2Buckets, accessAppsPayload, serviceTokensPayload, routesPayload] = payloads;
  return normalizeProviderInventory({
    accountId,
    zoneId,
    workers: normalizedArray(workersPayload),
    d1: normalizedArray(d1Payload),
    queues: normalizedArray(queuesPayload),
    r2: r2Buckets,
    accessApps: normalizedArray(accessAppsPayload),
    serviceTokens: normalizedArray(serviceTokensPayload),
    routes: normalizedArray(routesPayload),
  });
}

async function verifyGitHubBootstrapToken(token) {
  const user = await githubRequest(token, '/user');
  invariant(typeof user?.login === 'string' && user.login.length > 0, 'GH_BOOTSTRAP_ADMIN_TOKEN did not resolve an authenticated GitHub user');

  const publicKey = await githubRequest(token, `/repos/${EXPECTED.repository}/environments/${EXPECTED.environment}/secrets/public-key`);
  invariant(typeof publicKey?.key_id === 'string' && publicKey.key_id.length > 0, 'GitHub staging environment public key is unavailable');
  invariant(typeof publicKey?.key === 'string' && publicKey.key.length > 0, 'GitHub staging environment encryption key is unavailable');

  const workflows = await githubRequest(token, `/repos/${EXPECTED.repository}/actions/workflows?per_page=1`);
  invariant(Number.isInteger(workflows?.total_count), 'GH_BOOTSTRAP_ADMIN_TOKEN cannot read Actions workflow metadata');
  return user.login;
}

async function writeSummary({ accountId, zoneId, githubLogin, inventory }) {
  const summaryPath = process.env.GITHUB_STEP_SUMMARY;
  if (!summaryPath) return;
  const text = [
    '## AR-8C staging bootstrap preflight',
    '',
    '- Result: **PASS**',
    `- Repository: \`${EXPECTED.repository}\``,
    `- Environment: \`${EXPECTED.environment}\``,
    `- Cloudflare account: \`${EXPECTED.accountName}\``,
    `- Account ID: \`${accountId}\``,
    `- Zone: \`${EXPECTED.zoneName}\``,
    `- Zone ID: \`${zoneId}\``,
    `- Staging hostname authority: \`${EXPECTED.stagingHostname}\``,
    `- GitHub bootstrap principal: \`${githubLogin}\``,
    '- Mutation performed: **none**',
    '- Secret values emitted: **none**',
    '',
    '### Non-secret provider inventory',
    '',
    '```json',
    JSON.stringify(inventory, null, 2),
    '```',
    '',
  ].join('\n');
  await appendFile(summaryPath, text, { encoding: 'utf8' });
}

function selfTest() {
  invariant(EXPECTED.environment === 'staging', 'preflight environment drifted');
  invariant(EXPECTED.repository === 'iamaman11/part-crm-emai-profile', 'repository drifted');
  invariant(EXPECTED.accountName === "Pvisakp@gmail.com's Account", 'Cloudflare account authority drifted');
  invariant(EXPECTED.zoneName === 'alegria.by', 'Cloudflare zone authority drifted');
  invariant(EXPECTED.stagingHostname === 'staging.alegria.by', 'staging hostname authority drifted');
  invariant(!JSON.stringify(EXPECTED).includes('production'), 'production authority must not be present in AR-8C staging preflight');

  invariant(GITHUB_READ_RETRY_POLICY.maxAttempts === 5, 'GitHub read retry attempt bound drifted');
  invariant(GITHUB_READ_RETRY_POLICY.maxDelayMs === 8_000, 'GitHub read retry delay cap drifted');
  for (const status of [429, 500, 503, 599]) invariant(isRetryableGitHubReadStatus(status), `GitHub HTTP ${status} must remain retryable`);
  for (const status of [400, 401, 403, 404, 409, 422]) invariant(!isRetryableGitHubReadStatus(status), `GitHub HTTP ${status} must remain fail-fast`);
  invariant(isRetryableGitHubTransportError(new TypeError('fixture')), 'GitHub transport TypeError must remain retryable');
  invariant(!isRetryableGitHubTransportError(new Error('fixture')), 'ordinary GitHub errors must remain fail-fast');
  invariant(githubReadRetryDelayMs(null, 1, 0) === 1_000 && githubReadRetryDelayMs(null, 4, 0) === 8_000, 'GitHub retry backoff drifted');

  const fixture = normalizeProviderInventory({
    accountId: '0123456789abcdef0123456789abcdef',
    zoneId: 'fedcba9876543210fedcba9876543210',
    workers: [{ id: 'worker-staging', created_on: '2026-01-01T00:00:00Z' }],
    d1: [{ name: 'catalog-staging', uuid: '00000000-0000-4000-8000-000000000001' }],
    queues: [{ queue_name: 'jobs-staging', queue_id: 'queue-id' }],
    r2: [{ name: 'objects-staging', location: 'eeur' }],
    accessApps: [{ name: 'app-staging', id: 'app-id', domain: 'staging.alegria.by', aud: 'aud-id', type: 'self_hosted' }],
    serviceTokens: [{ name: 'service-staging', id: 'service-id', expires_at: '2027-01-01T00:00:00Z', client_secret: 'must-not-be-projected' }],
    routes: [{ id: 'route-id', pattern: 'staging.alegria.by/*', script: 'worker-staging' }],
  });
  invariant(fixture.access_service_tokens[0]?.name === 'service-staging', 'service token metadata projection drifted');
  invariant(!JSON.stringify(fixture).includes('must-not-be-projected'), 'secret-shaped fixture value leaked into provider inventory');
  invariant(fixture.workers_scripts.length === 1 && fixture.d1_databases.length === 1, 'provider inventory projection drifted');
  console.log('AR-8C staging bootstrap preflight self-test: PASS');
}

async function preflight() {
  invariant(process.env.GITHUB_REPOSITORY === EXPECTED.repository, `must run in ${EXPECTED.repository}`);
  invariant(process.env.GITHUB_REF_NAME === EXPECTED.refName, 'protected bootstrap preflight must run from main');
  invariant(process.env.GITHUB_EVENT_NAME === EXPECTED.eventName, 'protected bootstrap preflight must be manually dispatched');
  invariant(process.env.AR8C_TARGET_ENVIRONMENT === EXPECTED.environment, 'AR8C_TARGET_ENVIRONMENT must be staging');

  const cloudflareBootstrapToken = secret('CLOUDFLARE_BOOTSTRAP_TOKEN');
  const cloudflareIssuerToken = secret('CLOUDFLARE_TOKEN_ISSUER_TOKEN');
  const githubBootstrapToken = secret('GH_BOOTSTRAP_ADMIN_TOKEN');

  await verifyCloudflareToken(cloudflareBootstrapToken, 'CLOUDFLARE_BOOTSTRAP_TOKEN');
  await verifyCloudflareToken(cloudflareIssuerToken, 'CLOUDFLARE_TOKEN_ISSUER_TOKEN');
  const githubLogin = await verifyGitHubBootstrapToken(githubBootstrapToken);
  const providerIdentity = await discoverExactZone(cloudflareBootstrapToken);
  const inventory = await discoverProviderInventory(cloudflareBootstrapToken, providerIdentity);
  await writeSummary({ ...providerIdentity, githubLogin, inventory });
  console.log(`AR8C_PROVIDER_INVENTORY_JSON=${JSON.stringify(inventory)}`);
  console.log('AR-8C staging bootstrap preflight: PASS (read-only; no provider or GitHub mutation performed)');
}

const command = process.argv[2] ?? 'preflight';
if (command === 'self-test') {
  selfTest();
} else if (command === 'preflight') {
  await preflight();
} else {
  throw new Error(`Unknown command: ${command}`);
}
