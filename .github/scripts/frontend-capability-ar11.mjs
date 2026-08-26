import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const files = {
  capabilityContext: 'frontend/src/app/CapabilityContext.tsx',
  sessionApi: 'frontend/src/features/session/api.ts',
  router: 'frontend/src/app/router.tsx',
  clientsWorkspace: 'frontend/src/features/clients/ClientsWorkspace.tsx',
  clientMail: 'frontend/src/features/clients/ClientMailPanel.tsx',
  main: 'frontend/src/main.tsx',
  workerGate: 'apps/control-plane-worker/src/capability_gate.rs',
  worker: 'apps/control-plane-worker/src/lib.rs',
};

function read(relative) {
  return fs.readFileSync(path.join(root, relative), 'utf8');
}

function fail(message) {
  throw new Error(`AR-11 frontend capability gate: ${message}`);
}

const context = read(files.capabilityContext);
const sessionApi = read(files.sessionApi);
const router = read(files.router);
const workspace = read(files.clientsWorkspace);
const clientMail = read(files.clientMail);
const main = read(files.main);
const workerGate = read(files.workerGate);
const worker = read(files.worker);

for (const marker of [
  "import { getSession, type ActivationUnit } from '../features/session/api'",
  'const session = await getSession(tenantId, controller.signal);',
  'new Set<ActivationUnit>(session.capabilities)',
  'setProfileId(session.profileId);',
  'setProfileDigest(session.profileDigest);',
]) {
  if (!context.includes(marker)) fail(`frontend session projection marker missing: ${marker}`);
}

for (const marker of [
  "getAuthenticatedSession as getSessionOperation",
  'return getSessionOperation({',
  '...(signal === undefined ? {} : { signal })',
]) {
  if (!sessionApi.includes(marker)) fail(`generated session operation marker missing: ${marker}`);
}

if (!main.includes('<CapabilityProvider>') || !main.includes('<TenantProvider>')) {
  fail('CapabilityProvider is not installed below TenantProvider');
}

for (const marker of [
  "enabled('mailbox_admin') ? <Link to=\"/mailboxes\">",
  '<CapabilityBoundary unit="mailbox_admin">',
  "enabled('mailbox_read')",
  "outboundMailEnabled={enabled('outbound_mail')}",
]) {
  if (!`${router}\n${workspace}`.includes(marker)) fail(`capability-aware UI marker missing: ${marker}`);
}

for (const marker of [
  'outboundMailEnabled: boolean',
  'if (!outboundMailEnabled) return;',
  'Outbound mail is disabled by the active release profile.',
  'outboundMailEnabled && composer',
]) {
  if (!clientMail.includes(marker)) fail(`Client Mail send isolation marker missing: ${marker}`);
}

for (const marker of [
  'ProfileSelectionError::ProductionNotAuthorized',
]) {
  if (!workerGate.includes(marker)) fail(`backend fail-closed marker missing: ${marker}`);
}
if (!worker.includes('capability_session_response') || !worker.includes('capability_gate::route_enabled')) {
  fail('backend session projection or pre-dispatch security gate is missing');
}

const frontendSources = [context, router, workspace, clientMail, main].join('\n');
for (const forbidden of ['VITE_ENABLE_', 'VITE_FEATURE_', 'FEATURE_MAIL=', 'SHOW_MAILBOXES=']) {
  if (frontendSources.includes(forbidden)) fail(`independent frontend capability authority found: ${forbidden}`);
}

console.log('AR-11 frontend capability projection is backend-derived and read/send activation remains isolated.');
