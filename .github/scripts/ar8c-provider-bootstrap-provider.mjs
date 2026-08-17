import {
  EXPECTED, cfRequest, deriveR2Secret, exactByName, invariant, safeText,
  selectGroup, tokenPolicy, validateR2Credential,
} from './ar8c-provider-bootstrap-common.mjs';

export async function discoverIdentity(token) {
  const params = new URLSearchParams({ name: EXPECTED.zoneName, per_page: '50' });
  const payload = await cfRequest(token, `/zones?${params}`);
  const zones = Array.isArray(payload?.result) ? payload.result : [];
  const matches = zones.filter((zone) => zone?.name === EXPECTED.zoneName && zone?.account?.name === EXPECTED.accountName && zone?.status === 'active');
  invariant(matches.length === 1, `expected exactly one active ${EXPECTED.zoneName} zone in accepted account; got ${matches.length}`);
  const zone = matches[0];
  invariant(/^[0-9a-f]{32}$/.test(zone.id ?? ''), 'zone id shape invalid');
  invariant(/^[0-9a-f]{32}$/.test(zone.account?.id ?? ''), 'account id shape invalid');
  return { accountId: zone.account.id, zoneId: zone.id };
}

export async function acceptedPrerequisites(token, accountId) {
  const [d1Payload, queuesPayload, r2Payload] = await Promise.all([
    cfRequest(token, `/accounts/${accountId}/d1/database?per_page=10000`),
    cfRequest(token, `/accounts/${accountId}/queues`),
    cfRequest(token, `/accounts/${accountId}/r2/buckets?per_page=100`),
  ]);
  const d1 = Array.isArray(d1Payload?.result) ? d1Payload.result : [];
  const queues = Array.isArray(queuesPayload?.result) ? queuesPayload.result : [];
  const buckets = Array.isArray(r2Payload?.result?.buckets) ? r2Payload.result.buckets : (Array.isArray(r2Payload?.result) ? r2Payload.result : []);
  const resolver = exactByName(d1, EXPECTED.resolverD1);
  const catalog = exactByName(d1, EXPECTED.catalogD1);
  const bucket = exactByName(buckets, EXPECTED.r2Bucket);
  invariant(resolver?.uuid, `accepted resolver D1 ${EXPECTED.resolverD1} is missing`);
  invariant(catalog?.uuid, `accepted catalog D1 ${EXPECTED.catalogD1} is missing`);
  invariant(bucket, `accepted R2 bucket ${EXPECTED.r2Bucket} is missing`);
  for (const queueName of EXPECTED.queues) {
    invariant(exactByName(queues, queueName, (item) => item?.queue_name ?? item?.name), `accepted queue ${queueName} is missing`);
  }
  return { resolverId: resolver.uuid, catalogId: catalog.uuid, bucket };
}

async function listUserTokens(issuerToken) {
  const payload = await cfRequest(issuerToken, '/user/tokens?per_page=100');
  return Array.isArray(payload?.result) ? payload.result : [];
}

export async function permissionGroups(issuerToken) {
  const payload = await cfRequest(issuerToken, '/user/tokens/permission_groups?per_page=1000');
  const groups = Array.isArray(payload?.result) ? payload.result : [];
  invariant(groups.length > 0, 'Cloudflare token permission-group inventory is empty');
  return groups;
}

function normalizePolicy(policy) {
  return {
    effect: policy?.effect ?? null,
    permission_groups: (Array.isArray(policy?.permission_groups) ? policy.permission_groups : []).map((group) => group?.id).filter(Boolean).sort(),
    resources: policy?.resources ?? {},
  };
}

function policyFingerprint(policies) {
  return JSON.stringify((Array.isArray(policies) ? policies : []).map(normalizePolicy).sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b), 'en')));
}

function r2Policies(groups, accountId, bucket) {
  const group = selectGroup(groups, ['Workers R2 Storage Bucket Item Write'], 'com.cloudflare.edge.r2.bucket');
  const jurisdiction = safeText(bucket?.jurisdiction) ?? 'default';
  invariant(/^(default|eu|fedramp)$/.test(jurisdiction), `unsupported R2 jurisdiction ${jurisdiction}`);
  return [tokenPolicy([group.id], { [`com.cloudflare.edge.r2.bucket.${accountId}_${jurisdiction}_${EXPECTED.r2Bucket}`]: '*' })];
}

function deployPolicies(groups, accountId, zoneId) {
  const accountScope = 'com.cloudflare.api.account';
  const zoneScope = 'com.cloudflare.api.account.zone';
  const accountGroups = [
    selectGroup(groups, ['Workers Scripts Edit', 'Workers Scripts Write'], accountScope),
    selectGroup(groups, ['D1 Read'], accountScope),
    selectGroup(groups, ['Workers R2 Storage Read'], accountScope),
    selectGroup(groups, ['Queues Edit', 'Queues Write'], accountScope),
  ];
  const zoneGroups = [
    selectGroup(groups, ['Workers Routes Edit', 'Workers Routes Write'], zoneScope),
    selectGroup(groups, ['Zone Read'], zoneScope),
  ];
  return [
    tokenPolicy(accountGroups.map((group) => group.id), { [`com.cloudflare.api.account.${accountId}`]: '*' }),
    tokenPolicy(zoneGroups.map((group) => group.id), { [`com.cloudflare.api.account.zone.${zoneId}`]: '*' }),
  ];
}

export async function classifyBeforeMutation({ bootstrapToken, issuerToken, groups, accountId, zoneId, bucket }) {
  const [userTokens, accessTokensPayload, policiesPayload, appsPayload] = await Promise.all([
    listUserTokens(issuerToken),
    cfRequest(bootstrapToken, `/accounts/${accountId}/access/service_tokens?per_page=100`),
    cfRequest(bootstrapToken, `/accounts/${accountId}/access/policies?per_page=100`),
    cfRequest(bootstrapToken, `/accounts/${accountId}/access/apps?per_page=100`),
  ]);
  const accessTokens = Array.isArray(accessTokensPayload?.result) ? accessTokensPayload.result : [];
  const policies = Array.isArray(policiesPayload?.result) ? policiesPayload.result : [];
  const apps = Array.isArray(appsPayload?.result) ? appsPayload.result : [];
  const r2Token = exactByName(userTokens, EXPECTED.r2Token);
  const deployToken = exactByName(userTokens, EXPECTED.deployToken);
  if (r2Token) invariant(policyFingerprint(r2Token.policies) === policyFingerprint(r2Policies(groups, accountId, bucket)), `${EXPECTED.r2Token} exists with conflicting scope`);
  if (deployToken) invariant(policyFingerprint(deployToken.policies) === policyFingerprint(deployPolicies(groups, accountId, zoneId)), `${EXPECTED.deployToken} exists with conflicting scope`);

  const serviceToken = exactByName(accessTokens, EXPECTED.accessServiceToken);
  let policy = exactByName(policies, EXPECTED.accessPolicy);
  let app = exactByName(apps, EXPECTED.accessApp);
  if (policy?.id) policy = (await cfRequest(bootstrapToken, `/accounts/${accountId}/access/policies/${policy.id}`)).result;
  if (app?.id) app = (await cfRequest(bootstrapToken, `/accounts/${accountId}/access/apps/${app.id}`)).result;
  invariant(apps.every((candidate) => candidate?.domain !== EXPECTED.hostname || candidate?.name === EXPECTED.accessApp), `Access domain ${EXPECTED.hostname} has conflicting owner`);
  if (!serviceToken) invariant(!policy && !app, 'Access policy/application exists while accepted service token is missing');
  if (policy) {
    invariant(serviceToken && policy.decision === 'non_identity', `Access policy ${EXPECTED.accessPolicy} conflicts with accepted service-auth policy`);
    const ids = (Array.isArray(policy.include) ? policy.include : []).map((rule) => rule?.service_token?.token_id).filter(Boolean);
    invariant(ids.length === 1 && ids[0] === serviceToken.id, `Access policy ${EXPECTED.accessPolicy} has conflicting include rules`);
  }
  if (app) {
    invariant(policy && app.type === 'self_hosted' && app.domain === EXPECTED.hostname, `Access app ${EXPECTED.accessApp} conflicts with accepted graph`);
    const ids = (Array.isArray(app.policies) ? app.policies : []).map((entry) => typeof entry === 'string' ? entry : entry?.id).filter(Boolean);
    invariant(ids.includes(policy.id), `Access app ${EXPECTED.accessApp} does not link accepted policy`);
  }
}

export async function issueR2Credential({ issuerToken, groups, accountId, bucket }) {
  const tokens = await listUserTokens(issuerToken);
  const existing = exactByName(tokens, EXPECTED.r2Token);
  let tokenId;
  let tokenValue;
  if (existing) {
    invariant(existing.id, 'existing R2 token has no id');
    const rolled = await cfRequest(issuerToken, `/user/tokens/${existing.id}/value`, { method: 'PUT', body: {} });
    tokenId = existing.id;
    tokenValue = rolled.result;
  } else {
    const created = await cfRequest(issuerToken, '/user/tokens', { method: 'POST', body: { name: EXPECTED.r2Token, policies: r2Policies(groups, accountId, bucket) } });
    tokenId = created?.result?.id;
    tokenValue = created?.result?.value;
  }
  invariant(typeof tokenId === 'string' && typeof tokenValue === 'string' && tokenValue.length >= 40, 'R2 token issuance produced incomplete result');
  const result = { accessKeyId: tokenId, secretAccessKey: deriveR2Secret(tokenValue), tokenId };
  await validateR2Credential({ accountId, ...result });
  return result;
}

export async function reconcileAccess(bootstrapToken, accountId) {
  const tokensPayload = await cfRequest(bootstrapToken, `/accounts/${accountId}/access/service_tokens?per_page=100`);
  const tokens = Array.isArray(tokensPayload?.result) ? tokensPayload.result : [];
  const existing = exactByName(tokens, EXPECTED.accessServiceToken);
  const issued = existing
    ? await cfRequest(bootstrapToken, `/accounts/${accountId}/access/service_tokens/${existing.id}/rotate`, { method: 'POST', body: {} })
    : await cfRequest(bootstrapToken, `/accounts/${accountId}/access/service_tokens`, { method: 'POST', body: { name: EXPECTED.accessServiceToken, duration: '8760h' } });
  const serviceToken = issued.result;
  invariant(serviceToken?.id && serviceToken?.client_id && serviceToken?.client_secret, 'Access service-token issuance returned incomplete result');

  const policiesPayload = await cfRequest(bootstrapToken, `/accounts/${accountId}/access/policies?per_page=100`);
  const policies = Array.isArray(policiesPayload?.result) ? policiesPayload.result : [];
  let policy = exactByName(policies, EXPECTED.accessPolicy);
  if (!policy) policy = (await cfRequest(bootstrapToken, `/accounts/${accountId}/access/policies`, {
    method: 'POST', body: { name: EXPECTED.accessPolicy, decision: 'non_identity', include: [{ service_token: { token_id: serviceToken.id } }] },
  })).result;
  invariant(policy?.id, 'Access policy has no id');

  const appsPayload = await cfRequest(bootstrapToken, `/accounts/${accountId}/access/apps?per_page=100`);
  const apps = Array.isArray(appsPayload?.result) ? appsPayload.result : [];
  let app = exactByName(apps, EXPECTED.accessApp);
  if (!app) app = (await cfRequest(bootstrapToken, `/accounts/${accountId}/access/apps`, {
    method: 'POST',
    body: { name: EXPECTED.accessApp, type: 'self_hosted', domain: EXPECTED.hostname, app_launcher_visible: false, session_duration: '24h', policies: [{ id: policy.id, precedence: 1 }] },
  })).result;
  invariant(app?.id && typeof app?.aud === 'string' && app.aud.length > 0, 'Access app has no id/audience');
  const org = await cfRequest(bootstrapToken, `/accounts/${accountId}/access/organizations`);
  const authDomain = safeText(org?.result?.auth_domain);
  invariant(authDomain && /^[a-z0-9.-]+$/i.test(authDomain), 'Access organization auth_domain is missing or malformed');
  return { appId: app.id, policyId: policy.id, serviceTokenId: serviceToken.id, clientId: serviceToken.client_id, clientSecret: serviceToken.client_secret, audience: app.aud, issuer: `https://${authDomain}` };
}

export async function issueDeployToken({ issuerToken, groups, accountId, zoneId }) {
  const tokens = await listUserTokens(issuerToken);
  const existing = exactByName(tokens, EXPECTED.deployToken);
  const issued = existing
    ? await cfRequest(issuerToken, `/user/tokens/${existing.id}/value`, { method: 'PUT', body: {} })
    : await cfRequest(issuerToken, '/user/tokens', { method: 'POST', body: { name: EXPECTED.deployToken, policies: deployPolicies(groups, accountId, zoneId) } });
  const id = existing?.id ?? issued?.result?.id;
  const value = existing ? issued?.result : issued?.result?.value;
  invariant(id && typeof value === 'string' && value.length >= 40, 'steady-state deploy token issuance produced incomplete result');
  const verified = await cfRequest(value, '/user/tokens/verify');
  invariant(verified?.result?.status === 'active', 'new steady-state deploy token is not active');
  return { id, value };
}
