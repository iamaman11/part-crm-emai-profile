import { appendFile } from 'node:fs/promises';
import {
  CONTROL_PLANE_KEYS, EXPECTED, FINAL_ENV_BINDINGS, RESOLVER_KEYS,
  bindEnvironmentSecret, ghRequest, invariant, parseExactBundle, publicInput, secret,
} from './ar8c-provider-bootstrap-common.mjs';
import {
  acceptedPrerequisites, classifyBeforeMutation, discoverIdentity, issueDeployToken,
  issueR2Credential, permissionGroups, reconcileAccess,
} from './ar8c-provider-bootstrap-provider.mjs';

function buildManifest({ accountId, access, prerequisites, googleClientId, microsoftClientId }) {
  return {
    schema_version: 1,
    environment: EXPECTED.environment,
    control_plane: {
      worker_name: EXPECTED.controlPlaneWorker,
      account_id: accountId,
      custom_domain: EXPECTED.hostname,
      access_issuer: access.issuer,
      access_audience: access.audience,
      d1_database_name: EXPECTED.catalogD1,
      d1_database_id: prerequisites.catalogId,
      r2_bucket_name: EXPECTED.r2Bucket,
      generation_verification_queue: EXPECTED.queues[0],
      integration_events_queue: EXPECTED.queues[1],
      mailbox_jobs_queue: EXPECTED.queues[2],
      mailbox_jobs_dlq: EXPECTED.queues[3],
      mailbox_secret_resolver_service: EXPECTED.resolverWorker,
    },
    resolver: {
      worker_name: EXPECTED.resolverWorker,
      account_id: accountId,
      d1_database_name: EXPECTED.resolverD1,
      d1_database_id: prerequisites.resolverId,
      google_oauth_client_id: googleClientId,
      google_oauth_redirect_uri: EXPECTED.googleRedirect,
      microsoft_oauth_client_id: microsoftClientId,
      microsoft_oauth_redirect_uri: EXPECTED.microsoftRedirect,
    },
  };
}

async function verifyEnvironmentBindings(ghToken) {
  const payload = await ghRequest(ghToken, `/repos/${EXPECTED.repository}/environments/${EXPECTED.environment}/secrets?per_page=100`);
  const names = new Set((Array.isArray(payload?.secrets) ? payload.secrets : []).map((item) => item?.name).filter(Boolean));
  const missing = FINAL_ENV_BINDINGS.filter((name) => !names.has(name));
  invariant(missing.length === 0, `staging Environment is missing post-bootstrap bindings: ${missing.join(', ')}`);
}

async function writeSummary({ accountId, zoneId, access, r2TokenId, deployTokenId }) {
  const path = process.env.GITHUB_STEP_SUMMARY;
  if (!path) return;
  await appendFile(path, [
    '## AR-8C staging provider bootstrap handoff',
    '',
    '- Result: **PASS**',
    `- Account ID: \`${accountId}\``,
    `- Zone ID: \`${zoneId}\``,
    `- R2 credential token ID: \`${r2TokenId}\``,
    `- Steady-state deploy token ID: \`${deployTokenId}\``,
    `- Access application ID: \`${access.appId}\``,
    `- Access policy ID: \`${access.policyId}\``,
    `- Access service-token ID: \`${access.serviceTokenId}\``,
    `- GitHub staging bindings verified: \`${FINAL_ENV_BINDINGS.join(', ')}\``,
    '- Worker deploy performed here: **no**',
    '- Worker secret mutation performed here: **no**',
    '- Production mutation: **no**',
    '- Secret values emitted: **no**',
    '- Next authority: **Mailbox Secret Resolver Promotion**',
    '',
  ].join('\n'), 'utf8');
}

function selfTest() {
  invariant(EXPECTED.environment === 'staging', 'environment drifted');
  invariant(EXPECTED.hostname === 'staging.alegria.by', 'hostname drifted');
  invariant(!JSON.stringify(EXPECTED).includes('production'), 'production authority leaked into bootstrap executor');
  invariant(CONTROL_PLANE_KEYS.length === 4, 'control-plane bundle key set drifted');
  invariant(RESOLVER_KEYS.length === 5, 'resolver bundle key set drifted');
  invariant(FINAL_ENV_BINDINGS.length === 6, 'final Environment binding set drifted');
  const manifest = buildManifest({
    accountId: '0123456789abcdef0123456789abcdef',
    access: { issuer: 'https://example.cloudflareaccess.com', audience: 'aud' },
    prerequisites: { catalogId: 'catalog-id', resolverId: 'resolver-id' },
    googleClientId: 'google-client-id.apps.googleusercontent.com',
    microsoftClientId: '00000000-0000-4000-8000-000000000000',
  });
  invariant(manifest.schema_version === 1 && manifest.environment === 'staging', 'manifest envelope drifted');
  invariant(manifest.resolver.google_oauth_redirect_uri === EXPECTED.googleRedirect, 'Google redirect drifted');
  invariant(manifest.resolver.microsoft_oauth_redirect_uri === EXPECTED.microsoftRedirect, 'Microsoft redirect drifted');
  console.log('AR-8C staging provider bootstrap execution self-test: PASS');
}

async function execute() {
  invariant(process.env.GITHUB_REPOSITORY === EXPECTED.repository, `must run in ${EXPECTED.repository}`);
  invariant(process.env.GITHUB_REF_NAME === EXPECTED.refName, 'provider bootstrap execution must run from main');
  invariant(process.env.GITHUB_EVENT_NAME === EXPECTED.eventName, 'provider bootstrap execution must be workflow_dispatch only');
  invariant(process.env.AR8C_TARGET_ENVIRONMENT === EXPECTED.environment, 'target environment must be staging');

  const bootstrapToken = secret('CLOUDFLARE_BOOTSTRAP_TOKEN');
  const issuerToken = secret('CLOUDFLARE_TOKEN_ISSUER_TOKEN');
  const ghToken = secret('GH_BOOTSTRAP_ADMIN_TOKEN');
  invariant(bootstrapToken !== issuerToken, 'Cloudflare bootstrap and token-issuer credentials must be distinct');

  const resolverBundleRaw = secret('CLOUDFLARE_RESOLVER_SECRETS_JSON');
  const controlBundleRaw = secret('CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON');
  const resolverBundle = parseExactBundle(resolverBundleRaw, RESOLVER_KEYS, 'CLOUDFLARE_RESOLVER_SECRETS_JSON');
  const controlBundle = parseExactBundle(controlBundleRaw, CONTROL_PLANE_KEYS, 'CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON');
  const googleClientId = publicInput('AR8C_GOOGLE_OAUTH_CLIENT_ID');
  const microsoftClientId = publicInput('AR8C_MICROSOFT_OAUTH_CLIENT_ID');

  const { accountId, zoneId } = await discoverIdentity(bootstrapToken);
  const prerequisites = await acceptedPrerequisites(bootstrapToken, accountId);
  const groups = await permissionGroups(issuerToken);
  await classifyBeforeMutation({ bootstrapToken, issuerToken, groups, accountId, zoneId, bucket: prerequisites.bucket });

  const r2 = await issueR2Credential({ issuerToken, groups, accountId, bucket: prerequisites.bucket });
  const nextControlBundle = {
    ...controlBundle,
    R2_GENERATION_ACCESS_KEY_ID: r2.accessKeyId,
    R2_GENERATION_SECRET_ACCESS_KEY: r2.secretAccessKey,
  };
  parseExactBundle(JSON.stringify(nextControlBundle), CONTROL_PLANE_KEYS, 'generated control-plane bundle');

  const access = await reconcileAccess(bootstrapToken, accountId);
  const deploy = await issueDeployToken({ issuerToken, groups, accountId, zoneId });
  invariant(deploy.value !== bootstrapToken && deploy.value !== issuerToken, 'steady-state deploy credential must differ from both bootstrap credentials');

  const manifest = buildManifest({ accountId, access, prerequisites, googleClientId, microsoftClientId });
  const manifestJson = JSON.stringify(manifest);
  invariant(!/(client_secret|secret_access_key|authorization|api_token)/i.test(manifestJson), 'deploy manifest contains a forbidden secret-bearing field');

  bindEnvironmentSecret(ghToken, 'CLOUDFLARE_ACCESS_CLIENT_ID', access.clientId);
  bindEnvironmentSecret(ghToken, 'CLOUDFLARE_ACCESS_CLIENT_SECRET', access.clientSecret);
  bindEnvironmentSecret(ghToken, 'CLOUDFLARE_API_TOKEN', deploy.value);
  bindEnvironmentSecret(ghToken, 'CLOUDFLARE_DEPLOY_MANIFEST_JSON', manifestJson);
  bindEnvironmentSecret(ghToken, 'CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON', JSON.stringify(nextControlBundle));
  bindEnvironmentSecret(ghToken, 'CLOUDFLARE_RESOLVER_SECRETS_JSON', JSON.stringify(resolverBundle));

  await verifyEnvironmentBindings(ghToken);
  await writeSummary({ accountId, zoneId, access, r2TokenId: r2.tokenId, deployTokenId: deploy.id });
  console.log('AR-8C staging provider bootstrap execution: PASS (provider credentials/config handoff only; no Worker deploy, Worker secret mutation, or production mutation)');
}

const command = process.argv[2] ?? 'execute';
if (command === 'self-test') selfTest();
else if (command === 'execute') await execute();
else throw new Error(`Unknown command: ${command}`);
