import { createHash, createHmac } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { githubReadJson } from './ar8c-github-read-retry.mjs';

export const EXPECTED = Object.freeze({
  repository: 'iamaman11/part-crm-emai-profile',
  refName: 'main',
  eventName: 'workflow_dispatch',
  environment: 'staging',
  accountName: "Pvisakp@gmail.com's Account",
  zoneName: 'alegria.by',
  hostname: 'staging.alegria.by',
  resolverD1: 'mailbox-secret-resolver-gate-b-bootstrap-20260815-033400',
  catalogD1: 'part-crm-catalog-staging-d3-20260815',
  r2Bucket: 'part-crm-profile-objects-staging-d3',
  queues: Object.freeze([
    'part-crm-generation-verification-staging-d3',
    'part-crm-integration-events-staging-d3',
    'part-crm-mailbox-jobs-staging-d3',
    'part-crm-mailbox-jobs-dlq-staging-d3',
  ]),
  resolverWorker: 'mailbox-secret-resolver-staging',
  controlPlaneWorker: 'browser-profile-control-plane-staging',
  accessApp: 'part-crm-staging-control-plane',
  accessServiceToken: 'part-crm-staging-github-service-auth',
  accessPolicy: 'part-crm-staging-service-auth',
  r2Token: 'part-crm-staging-profile-generation-r2',
  deployToken: 'part-crm-staging-github-deploy',
  googleRedirect: 'https://staging.alegria.by/api/v1/mailbox/gmail/oauth/callback',
  microsoftRedirect: 'https://staging.alegria.by/api/v1/mailbox/microsoft-graph/oauth/callback',
});

export const CONTROL_PLANE_KEYS = Object.freeze([
  'CLIENT_CONTACT_PROTECTION_KEYRING',
  'MAILBOX_RESOLVER_CALLER_AUTH_KEY',
  'R2_GENERATION_ACCESS_KEY_ID',
  'R2_GENERATION_SECRET_ACCESS_KEY',
]);
export const RESOLVER_KEYS = Object.freeze([
  'GOOGLE_OAUTH_CLIENT_SECRET',
  'MAILBOX_RESOLVER_CALLER_AUTH_KEY',
  'MAILBOX_RESOLVER_ENCRYPTION_KEYRING',
  'MAILBOX_RESOLVER_HANDLE_HMAC_KEY',
  'MICROSOFT_OAUTH_CLIENT_SECRET',
]);
export const FINAL_ENV_BINDINGS = Object.freeze([
  'CLOUDFLARE_ACCESS_CLIENT_ID',
  'CLOUDFLARE_ACCESS_CLIENT_SECRET',
  'CLOUDFLARE_API_TOKEN',
  'CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON',
  'CLOUDFLARE_DEPLOY_MANIFEST_JSON',
  'CLOUDFLARE_RESOLVER_SECRETS_JSON',
]);

export const R2_CREDENTIAL_VALIDATION_POLICY = Object.freeze({
  maxAttempts: 5,
  baseDelayMs: 1_000,
  maxDelayMs: 8_000,
});

const CF_API = 'https://api.cloudflare.com/client/v4';
const GITHUB_API_VERSION = '2026-03-10';

export function invariant(condition, message) {
  if (!condition) throw new Error(message);
}

export function secret(name) {
  const value = process.env[name];
  invariant(typeof value === 'string' && value.trim().length >= 20, `${name} is missing or unusable in protected staging`);
  return value.trim();
}

export function publicInput(name) {
  const value = process.env[name];
  invariant(typeof value === 'string' && value.trim().length >= 6, `${name} is missing`);
  const result = value.trim();
  invariant(!/(dummy|example|placeholder|changeme|todo|test-client)/i.test(result), `${name} contains a forbidden placeholder`);
  invariant(!/[\r\n\t]/.test(result), `${name} contains control whitespace`);
  return result;
}

export function safeText(value) {
  if (typeof value !== 'string' || value.length === 0) return null;
  return value.replace(/[\r\n\t]/g, ' ').slice(0, 300);
}

function sanitizeApiErrors(payload) {
  return (Array.isArray(payload?.errors) ? payload.errors : []).slice(0, 5).map((entry) => ({
    code: entry?.code ?? null,
    message: safeText(entry?.message) ?? 'unknown error',
  }));
}

export async function cfRequest(token, path, { method = 'GET', body = undefined } = {}) {
  const response = await fetch(`${CF_API}${path}`, {
    method,
    headers: {
      Authorization: `Bearer ${token}`,
      Accept: 'application/json',
      ...(body === undefined ? {} : { 'Content-Type': 'application/json' }),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(30_000),
  });
  let payload;
  try { payload = await response.json(); } catch { throw new Error(`Cloudflare ${method} ${path} returned non-JSON HTTP ${response.status}`); }
  if (!response.ok || payload?.success !== true) {
    throw new Error(`Cloudflare ${method} ${path} failed HTTP ${response.status}: ${JSON.stringify(sanitizeApiErrors(payload))}`);
  }
  return payload;
}

export async function ghRequest(token, path) {
  return githubReadJson({
    token,
    path,
    apiVersion: GITHUB_API_VERSION,
    userAgent: 'part-crm-ar8c-provider-bootstrap',
  });
}

export function parseExactBundle(raw, expectedKeys, label) {
  let value;
  try { value = JSON.parse(raw); } catch { throw new Error(`${label} is not valid JSON`); }
  invariant(value && typeof value === 'object' && !Array.isArray(value), `${label} must be a JSON object`);
  const keys = Object.keys(value).sort();
  const expected = [...expectedKeys].sort();
  invariant(JSON.stringify(keys) === JSON.stringify(expected), `${label} must contain exact key set: ${expected.join(', ')}`);
  for (const key of expected) {
    invariant(typeof value[key] === 'string' && value[key].trim().length >= 20, `${label}.${key} is missing or unusable`);
    invariant(!/(dummy|example|placeholder|changeme|todo)/i.test(value[key]), `${label}.${key} contains a forbidden placeholder`);
  }
  return value;
}

export function exactByName(items, name, projection = (item) => item?.name) {
  const matches = items.filter((item) => projection(item) === name);
  invariant(matches.length <= 1, `provider conflict: multiple resources named ${name}`);
  return matches[0] ?? null;
}

export function selectGroup(groups, aliases, requiredScope) {
  for (const alias of aliases) {
    const matches = groups.filter((group) => group?.name === alias && (!requiredScope || (group?.scopes ?? []).includes(requiredScope)));
    invariant(matches.length <= 1, `permission-group inventory returned duplicates for ${alias} / ${requiredScope}`);
    if (matches.length === 1) {
      invariant(typeof matches[0].id === 'string' && matches[0].id.length > 0, `permission group ${alias} has no id`);
      return matches[0];
    }
  }
  throw new Error(`required permission group is unavailable: ${aliases.join(' / ')} / ${requiredScope}`);
}

export function tokenPolicy(permissionGroupIds, resources) {
  return { effect: 'allow', permission_groups: permissionGroupIds.map((id) => ({ id })), resources };
}

function awsHmac(key, value) {
  return createHmac('sha256', key).update(value, 'utf8').digest();
}

export function deriveR2Secret(tokenValue) {
  invariant(typeof tokenValue === 'string' && tokenValue.length >= 40, 'R2 token value is missing');
  return createHash('sha256').update(tokenValue, 'utf8').digest('hex');
}

export function isRetryableR2ValidationStatus(status) {
  return status === 401 || status === 429 || (status >= 500 && status <= 599);
}

export function r2CredentialValidationDelayMs(attempt) {
  return Math.min(
    R2_CREDENTIAL_VALIDATION_POLICY.maxDelayMs,
    R2_CREDENTIAL_VALIDATION_POLICY.baseDelayMs * (2 ** Math.max(0, attempt - 1)),
  );
}

function isRetryableR2TransportError(error) {
  return error instanceof TypeError || ['AbortError', 'TimeoutError'].includes(error?.name);
}

function safeR2ErrorCode(body) {
  if (typeof body !== 'string') return null;
  const match = body.slice(0, 8_192).match(/<Code>([^<]{1,80})<\/Code>/i);
  if (!match) return null;
  const value = match[1].trim();
  return /^[A-Za-z0-9_.-]+$/.test(value) ? value : null;
}

async function defaultSleep(ms) {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

export async function validateR2Credential({
  accountId,
  accessKeyId,
  secretAccessKey,
  fetchImpl = fetch,
  sleepImpl = defaultSleep,
  nowImpl = () => new Date(),
}) {
  const host = `${accountId}.r2.cloudflarestorage.com`;
  const query = 'list-type=2&max-keys=1&prefix=ar8c-bootstrap-validation%2F';
  const payloadHash = createHash('sha256').update('').digest('hex');

  for (let attempt = 1; attempt <= R2_CREDENTIAL_VALIDATION_POLICY.maxAttempts; attempt += 1) {
    const now = nowImpl();
    invariant(now instanceof Date && Number.isFinite(now.getTime()), 'R2 validation clock returned an invalid date');
    const amzDate = now.toISOString().replace(/[:-]|\.\d{3}/g, '');
    const dateStamp = amzDate.slice(0, 8);
    const headers = `host:${host}\nx-amz-content-sha256:${payloadHash}\nx-amz-date:${amzDate}\n`;
    const signedHeaders = 'host;x-amz-content-sha256;x-amz-date';
    const canonical = `GET\n/${EXPECTED.r2Bucket}\n${query}\n${headers}\n${signedHeaders}\n${payloadHash}`;
    const scope = `${dateStamp}/auto/s3/aws4_request`;
    const stringToSign = `AWS4-HMAC-SHA256\n${amzDate}\n${scope}\n${createHash('sha256').update(canonical).digest('hex')}`;
    const kDate = awsHmac(Buffer.from(`AWS4${secretAccessKey}`, 'utf8'), dateStamp);
    const kRegion = awsHmac(kDate, 'auto');
    const kService = awsHmac(kRegion, 's3');
    const kSigning = awsHmac(kService, 'aws4_request');
    const signature = createHmac('sha256', kSigning).update(stringToSign, 'utf8').digest('hex');
    const authorization = `AWS4-HMAC-SHA256 Credential=${accessKeyId}/${scope}, SignedHeaders=${signedHeaders}, Signature=${signature}`;

    let response;
    try {
      response = await fetchImpl(`https://${host}/${EXPECTED.r2Bucket}?${query}`, {
        headers: { Authorization: authorization, 'x-amz-content-sha256': payloadHash, 'x-amz-date': amzDate },
        signal: AbortSignal.timeout(20_000),
      });
    } catch (error) {
      if (!isRetryableR2TransportError(error) || attempt === R2_CREDENTIAL_VALIDATION_POLICY.maxAttempts) {
        throw new Error(`new R2 credential validation transport failed after ${attempt} attempt(s)`);
      }
      await sleepImpl(r2CredentialValidationDelayMs(attempt));
      continue;
    }

    if (response.ok) return;
    let body = '';
    try { body = await response.text(); } catch { body = ''; }
    const code = safeR2ErrorCode(body);
    const detail = code ? ` (${code})` : '';
    if (!isRetryableR2ValidationStatus(response.status) || attempt === R2_CREDENTIAL_VALIDATION_POLICY.maxAttempts) {
      throw new Error(`new R2 credential failed bounded bucket validation with HTTP ${response.status}${detail} after ${attempt} attempt(s)`);
    }
    await sleepImpl(r2CredentialValidationDelayMs(attempt));
  }
  throw new Error('new R2 credential validation exhausted unexpectedly');
}

export function bindEnvironmentSecret(ghToken, name, value) {
  invariant(typeof value === 'string' && value.length > 0, `${name} has no value to bind`);
  const result = spawnSync('gh', ['secret', 'set', name, '--env', EXPECTED.environment, '--repo', EXPECTED.repository, '--body', '-'], {
    input: value,
    encoding: 'utf8',
    env: { ...process.env, GH_TOKEN: ghToken },
    maxBuffer: 1024 * 1024,
  });
  if (result.status !== 0) throw new Error(`GitHub write-only binding failed for ${name}: ${safeText(result.stderr) ?? 'gh exited non-zero'}`);
}
