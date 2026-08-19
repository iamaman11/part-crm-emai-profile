import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const files = {
  capabilityContext: 'frontend/src/app/CapabilityContext.tsx',
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
const router = read(files.router);
const workspace = read(files.clientsWorkspace);
const clientMail = read(files.clientMail);
const main = read(files.main);
const workerGate = read(files.workerGate);
const worker = read(files.worker);

for (const marker of [
  "response.headers.get('x-release-profile')",
  "response.headers.get('x-release-profile-digest')",
  "response.headers.get('x-effective-capabilities')",
  "KNOWN_ACTIVATION_UNITS",
  "Capability projection headers are missing",
]) {
  if (!context.includes(marker)) fail(`frontend session projection marker missing: ${marker}`);
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
  'X-Release-Profile',
  'X-Release-Profile-Digest',
  'X-Effective-Capabilities',
  'ProfileSelectionError::ProductionNotAuthorized',
]) {
  if (!workerGate.includes(marker)) fail(`backend projection/fail-closed marker missing: ${marker}`);
}
if (!worker.includes('capability_session_response') || !worker.includes('capability_gate::route_enabled')) {
  fail('backend session projection or pre-dispatch security gate is missing');
}

const frontendSources = [context, router, workspace, clientMail, main].join('\n');
for (const forbidden of ['VITE_ENABLE_', 'VITE_FEATURE_', 'FEATURE_MAIL=', 'SHOW_MAILBOXES=']) {
  if (frontendSources.includes(forbidden)) fail(`independent frontend capability authority found: ${forbidden}`);
}

console.log('AR-11 frontend capability projection is backend-derived and read/send activation remains isolated.');
