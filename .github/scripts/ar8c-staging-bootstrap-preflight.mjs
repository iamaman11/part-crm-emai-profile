import { appendFile } from 'node:fs/promises';

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
const GITHUB_API = 'https://api.github.com';
const GITHUB_API_VERSION = '2026-03-10';

function invariant(condition, message) {
  if (!condition) throw new Error(message);
}

function secret(name) {
  const value = process.env[name];
  invariant(typeof value === 'string' && value.trim().length >= 20, `${name} is missing or unusable in the protected staging environment`);
  return value.trim();
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
  const response = await fetch(`${GITHUB_API}${path}`, {
    headers: {
      Authorization: `Bearer ${token}`,
      Accept: 'application/vnd.github+json',
      'X-GitHub-Api-Version': GITHUB_API_VERSION,
      'User-Agent': 'part-crm-ar8c-bootstrap-preflight',
    },
    signal: AbortSignal.timeout(20_000),
  });

  let payload = null;
  const text = await response.text();
  if (text) {
    try {
      payload = JSON.parse(text);
    } catch {
      throw new Error(`GitHub ${path} returned non-JSON HTTP ${response.status}`);
    }
  }

  if (!response.ok) {
    const message = typeof payload?.message === 'string' ? payload.message.slice(0, 300) : 'unknown error';
    throw new Error(`GitHub ${path} failed with HTTP ${response.status}: ${message}`);
  }
  return payload;
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

async function verifyBootstrapReadSurface(token, { accountId, zoneId }) {
  const checks = [
    ['Workers Scripts', `/accounts/${accountId}/workers/scripts`],
    ['D1', `/accounts/${accountId}/d1/database?per_page=10`],
    ['Queues', `/accounts/${accountId}/queues`],
    ['R2', `/accounts/${accountId}/r2/buckets?per_page=10`],
    ['Access Apps', `/accounts/${accountId}/access/apps?per_page=10`],
    ['Access Service Tokens', `/accounts/${accountId}/access/service_tokens?per_page=10`],
    ['Workers Routes', `/zones/${zoneId}/workers/routes`],
  ];

  for (const [label, path] of checks) {
    try {
      await cloudflareRequest(token, path);
    } catch (error) {
      throw new Error(`${label} read-only preflight failed: ${error.message}`);
    }
  }
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

async function writeSummary({ accountId, zoneId, githubLogin }) {
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
  await verifyBootstrapReadSurface(cloudflareBootstrapToken, providerIdentity);
  await writeSummary({ ...providerIdentity, githubLogin });
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
