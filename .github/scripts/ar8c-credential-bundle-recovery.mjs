import { randomBytes } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { appendFile } from 'node:fs/promises';
import {
  CONTROL_PLANE_KEYS,
  EXPECTED,
  RESOLVER_KEYS,
  bindEnvironmentSecret,
  ghRequest,
  invariant,
  parseExactBundle,
  secret,
} from './ar8c-provider-bootstrap-common.mjs';
import {
  acceptedPrerequisites,
  classifyBeforeMutation,
  discoverIdentity,
  issueR2Credential,
  permissionGroups,
} from './ar8c-provider-bootstrap-provider.mjs';

const ASSESSMENT_SCRIPT = '.github/scripts/ar8c-credential-recovery-assessment.mjs';
const RECOVERY_INPUT_BINDINGS = Object.freeze([
  'GOOGLE_OAUTH_CLIENT_SECRET',
  'MICROSOFT_OAUTH_CLIENT_SECRET',
]);
const FINAL_RECOVERY_BINDINGS = Object.freeze([
  'CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON',
  'CLOUDFLARE_RESOLVER_SECRETS_JSON',
]);

function exactKeys(value, expected) {
  return JSON.stringify(Object.keys(value).sort()) === JSON.stringify([...expected].sort());
}

function isHex32(value) {
  return typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);
}

function validateContactKeyring(serialized) {
  let value;
  try { value = JSON.parse(serialized); } catch { throw new Error('generated client contact keyring is not valid JSON'); }
  invariant(value && typeof value === 'object' && !Array.isArray(value), 'generated client contact keyring must be an object');
  invariant(exactKeys(value, ['encryption', 'lookup']), 'generated client contact keyring key set drifted');
  for (const family of ['encryption', 'lookup']) {
    invariant(Array.isArray(value[family]) && value[family].length === 1, `generated client contact ${family} keyring must contain exactly one initial key`);
    const entry = value[family][0];
    invariant(entry && exactKeys(entry, ['version', 'keyHex']), `generated client contact ${family} entry shape drifted`);
    invariant(entry.version === 1 && isHex32(entry.keyHex), `generated client contact ${family} entry is invalid`);
  }
  return serialized;
}

function validateResolverEncryptionKeyring(serialized) {
  let value;
  try { value = JSON.parse(serialized); } catch { throw new Error('generated resolver encryption keyring is not valid JSON'); }
  invariant(value && typeof value === 'object' && !Array.isArray(value), 'generated resolver encryption keyring must be an object');
  invariant(exactKeys(value, ['activeVersion', 'keys']), 'generated resolver encryption keyring key set drifted');
  invariant(value.activeVersion === 1, 'generated resolver encryption active version must start at 1');
  invariant(Array.isArray(value.keys) && value.keys.length === 1, 'generated resolver encryption keyring must contain exactly one initial key');
  const entry = value.keys[0];
  invariant(entry && exactKeys(entry, ['version', 'keyHex']), 'generated resolver encryption entry shape drifted');
  invariant(entry.version === 1 && isHex32(entry.keyHex), 'generated resolver encryption entry is invalid');
  return serialized;
}

function generateProjectMaterial(randomBytesImpl = randomBytes) {
  const nextHex = () => randomBytesImpl(32).toString('hex');
  const contactEncryption = nextHex();
  const contactLookup = nextHex();
  const callerAuth = nextHex();
  const resolverEncryption = nextHex();
  const handleHmac = nextHex();
  const distinct = new Set([
    contactEncryption,
    contactLookup,
    callerAuth,
    resolverEncryption,
    handleHmac,
  ]);
  invariant(distinct.size === 5, 'project-generated recovery material must be independently random');

  const contactKeyring = validateContactKeyring(JSON.stringify({
    encryption: [{ version: 1, keyHex: contactEncryption }],
    lookup: [{ version: 1, keyHex: contactLookup }],
  }));
  const resolverKeyring = validateResolverEncryptionKeyring(JSON.stringify({
    activeVersion: 1,
    keys: [{ version: 1, keyHex: resolverEncryption }],
  }));
  invariant(isHex32(callerAuth), 'generated resolver caller-auth key is invalid');
  invariant(isHex32(handleHmac), 'generated resolver handle-HMAC key is invalid');
  return { contactKeyring, callerAuth, resolverKeyring, handleHmac };
}

function providerOwnedSecret(name) {
  invariant(RECOVERY_INPUT_BINDINGS.includes(name), `refusing non-canonical OAuth recovery input ${name}`);
  const value = secret(name);
  invariant(!/[\r\n\t]/.test(value), `${name} contains control whitespace`);
  invariant(!/(dummy|example|placeholder|changeme|todo)/i.test(value), `${name} contains a forbidden placeholder`);
  return value;
}

function runCanonicalRecoveryAssessment() {
  const result = spawnSync(process.execPath, [ASSESSMENT_SCRIPT, 'recovery-assess'], {
    encoding: 'utf8',
    env: process.env,
    maxBuffer: 1024 * 1024,
  });
  invariant(result.status === 0, 'canonical read-only credential recovery assessment failed');
  const prefix = 'AR8C_CREDENTIAL_RECOVERY_ASSESSMENT_JSON=';
  const lines = String(result.stdout ?? '').split(/\r?\n/);
  const matches = lines.filter((line) => line.startsWith(prefix));
  invariant(matches.length === 1, 'canonical recovery assessment did not emit exactly one evidence record');
  let evidence;
  try { evidence = JSON.parse(matches[0].slice(prefix.length)); } catch { throw new Error('canonical recovery assessment emitted malformed evidence'); }
  invariant(evidence?.classification === 'FRESH_PROJECT_SECRET_ISSUANCE_SAFE', 'fresh project-secret issuance is not safe; recovery refuses mutation');
  invariant(evidence?.external_oauth_secret_authority === 'PROVIDER_OWNED_INPUT_REQUIRED', 'OAuth secret authority classification drifted');
  invariant(evidence?.resolver_key_material_dependency_count === 0, 'resolver key-preservation dependency appeared before recovery');
  invariant(evidence?.catalog_resolver_reference_count === 0, 'catalog resolver reference appeared before recovery');
  invariant(evidence?.contact_key_material_dependency_count === 0, 'contact key-preservation dependency appeared before recovery');
  return evidence;
}

async function environmentSecretMetadata(ghToken) {
  const payload = await ghRequest(
    ghToken,
    `/repos/${EXPECTED.repository}/environments/${EXPECTED.environment}/secrets?per_page=100`,
  );
  const secrets = Array.isArray(payload?.secrets) ? payload.secrets : [];
  return new Map(secrets.map((entry) => [entry?.name, entry]).filter(([name]) => typeof name === 'string'));
}

function deleteRecoveryInputBinding(ghToken, name) {
  invariant(RECOVERY_INPUT_BINDINGS.includes(name), `refusing to delete non-recovery binding ${name}`);
  const result = spawnSync('gh', ['secret', 'delete', name, '--env', EXPECTED.environment, '--repo', EXPECTED.repository], {
    encoding: 'utf8',
    env: { ...process.env, GH_TOKEN: ghToken },
    maxBuffer: 1024 * 1024,
  });
  invariant(result.status === 0, `failed to remove consumed recovery input binding ${name}`);
}

function assertExecutionBoundary() {
  invariant(process.env.GITHUB_REPOSITORY === EXPECTED.repository, `must run in ${EXPECTED.repository}`);
  invariant(process.env.GITHUB_REF_NAME === EXPECTED.refName, 'credential bundle recovery must run from main');
  invariant(process.env.GITHUB_EVENT_NAME === EXPECTED.eventName, 'credential bundle recovery must be workflow_dispatch only');
  invariant(process.env.AR8C_TARGET_ENVIRONMENT === EXPECTED.environment, 'credential bundle recovery target must be staging');
}

async function writeSummary({ accountId, r2TokenId, evidence }) {
  const path = process.env.GITHUB_STEP_SUMMARY;
  if (!path) return;
  await appendFile(path, [
    '## AR-8C staging credential bundle recovery',
    '',
    '- Result: **PASS**',
    `- Recovery classification: \`${evidence.classification}\``,
    `- Account ID: \`${accountId}\``,
    `- R2 credential token ID: \`${r2TokenId}\``,
    `- Restored final bindings: \`${FINAL_RECOVERY_BINDINGS.join(', ')}\``,
    `- Consumed temporary OAuth bindings removed: \`${RECOVERY_INPUT_BINDINGS.join(', ')}\``,
    '- Access credential mutation: **no**',
    '- Deploy credential mutation: **no**',
    '- Worker deploy/secret mutation: **no**',
    '- D1 application-data mutation: **no**',
    '- Production mutation: **no**',
    '- Secret values emitted: **no**',
    '- Next authority: **AR-8C Staging Provider Bootstrap Execution**',
    '',
  ].join('\n'), 'utf8');
}

async function execute() {
  assertExecutionBoundary();
  const bootstrapToken = secret('CLOUDFLARE_BOOTSTRAP_TOKEN');
  const issuerToken = secret('CLOUDFLARE_TOKEN_ISSUER_TOKEN');
  const ghToken = secret('GH_BOOTSTRAP_ADMIN_TOKEN');
  invariant(bootstrapToken !== issuerToken, 'Cloudflare bootstrap and token-issuer credentials must be distinct');

  const googleOAuthSecret = providerOwnedSecret(RECOVERY_INPUT_BINDINGS[0]);
  const microsoftOAuthSecret = providerOwnedSecret(RECOVERY_INPUT_BINDINGS[1]);
  invariant(googleOAuthSecret !== microsoftOAuthSecret, 'Google and Microsoft recovery secrets must be distinct');
  invariant(![bootstrapToken, issuerToken, ghToken].includes(googleOAuthSecret), 'Google OAuth recovery secret collides with an operational credential');
  invariant(![bootstrapToken, issuerToken, ghToken].includes(microsoftOAuthSecret), 'Microsoft OAuth recovery secret collides with an operational credential');

  const before = await environmentSecretMetadata(ghToken);
  for (const name of [...RECOVERY_INPUT_BINDINGS, ...FINAL_RECOVERY_BINDINGS]) {
    invariant(before.has(name), `protected staging Environment is missing required recovery binding ${name}`);
  }

  const evidence = runCanonicalRecoveryAssessment();
  const { accountId, zoneId } = await discoverIdentity(bootstrapToken);
  const prerequisites = await acceptedPrerequisites(bootstrapToken, accountId);
  const groups = await permissionGroups(issuerToken);
  await classifyBeforeMutation({
    bootstrapToken,
    issuerToken,
    groups,
    accountId,
    zoneId,
    bucket: prerequisites.bucket,
  });

  const material = generateProjectMaterial();
  const r2 = await issueR2Credential({ issuerToken, groups, accountId, bucket: prerequisites.bucket });
  const controlBundle = {
    CLIENT_CONTACT_PROTECTION_KEYRING: material.contactKeyring,
    MAILBOX_RESOLVER_CALLER_AUTH_KEY: material.callerAuth,
    R2_GENERATION_ACCESS_KEY_ID: r2.accessKeyId,
    R2_GENERATION_SECRET_ACCESS_KEY: r2.secretAccessKey,
  };
  const resolverBundle = {
    GOOGLE_OAUTH_CLIENT_SECRET: googleOAuthSecret,
    MAILBOX_RESOLVER_CALLER_AUTH_KEY: material.callerAuth,
    MAILBOX_RESOLVER_ENCRYPTION_KEYRING: material.resolverKeyring,
    MAILBOX_RESOLVER_HANDLE_HMAC_KEY: material.handleHmac,
    MICROSOFT_OAUTH_CLIENT_SECRET: microsoftOAuthSecret,
  };
  parseExactBundle(JSON.stringify(controlBundle), CONTROL_PLANE_KEYS, 'generated recovery control-plane bundle');
  parseExactBundle(JSON.stringify(resolverBundle), RESOLVER_KEYS, 'generated recovery resolver bundle');

  bindEnvironmentSecret(ghToken, FINAL_RECOVERY_BINDINGS[0], JSON.stringify(controlBundle));
  bindEnvironmentSecret(ghToken, FINAL_RECOVERY_BINDINGS[1], JSON.stringify(resolverBundle));

  const afterWrite = await environmentSecretMetadata(ghToken);
  for (const name of FINAL_RECOVERY_BINDINGS) {
    const previous = before.get(name)?.updated_at;
    const current = afterWrite.get(name)?.updated_at;
    invariant(typeof current === 'string' && current.length > 0, `restored binding ${name} has no metadata timestamp`);
    invariant(current !== previous, `restored binding ${name} metadata timestamp did not change`);
  }

  for (const name of RECOVERY_INPUT_BINDINGS) deleteRecoveryInputBinding(ghToken, name);
  const afterCleanup = await environmentSecretMetadata(ghToken);
  for (const name of RECOVERY_INPUT_BINDINGS) {
    invariant(!afterCleanup.has(name), `consumed temporary OAuth binding ${name} still exists after cleanup`);
  }
  for (const name of FINAL_RECOVERY_BINDINGS) {
    invariant(afterCleanup.has(name), `restored final binding ${name} disappeared during cleanup`);
  }

  await writeSummary({ accountId, r2TokenId: r2.tokenId, evidence });
  console.log('AR-8C staging credential bundle recovery: PASS (two final bundles restored; temporary OAuth bindings removed; no deploy, Access, D1 application-data, or production mutation)');
}

function selfTest() {
  invariant(EXPECTED.environment === 'staging', 'recovery environment drifted');
  invariant(!JSON.stringify(EXPECTED).includes('production'), 'production authority leaked into recovery executor');
  invariant(JSON.stringify(FINAL_RECOVERY_BINDINGS) === JSON.stringify([
    'CLOUDFLARE_CONTROL_PLANE_SECRETS_JSON',
    'CLOUDFLARE_RESOLVER_SECRETS_JSON',
  ]), 'final recovery binding set drifted');
  invariant(JSON.stringify(RECOVERY_INPUT_BINDINGS) === JSON.stringify([
    'GOOGLE_OAUTH_CLIENT_SECRET',
    'MICROSOFT_OAUTH_CLIENT_SECRET',
  ]), 'canonical OAuth recovery input binding set drifted');

  let counter = 0;
  const fixtureRandom = (size) => {
    invariant(size === 32, 'fixture random size drifted');
    counter += 1;
    return Buffer.alloc(32, counter);
  };
  const material = generateProjectMaterial(fixtureRandom);
  invariant(counter === 5, 'project material generator must request five independent 32-byte values');
  validateContactKeyring(material.contactKeyring);
  validateResolverEncryptionKeyring(material.resolverKeyring);
  invariant(isHex32(material.callerAuth) && isHex32(material.handleHmac), 'generated caller/HMAC fixture shape drifted');

  let malformedRejected = false;
  try { validateResolverEncryptionKeyring('{"activeVersion":1,"keys":[]}'); } catch { malformedRejected = true; }
  invariant(malformedRejected, 'empty resolver keyring fixture must fail closed');
  console.log('AR-8C credential bundle recovery self-test: PASS');
}

const command = process.argv[2] ?? 'execute';
if (command === 'self-test') selfTest();
else if (command === 'execute') await execute();
else throw new Error(`Unknown command: ${command}`);
