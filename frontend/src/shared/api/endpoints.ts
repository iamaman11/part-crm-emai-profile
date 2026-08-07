import { newIdempotencyKey, requestJson, sha256Hex } from './client';
import type {
  ActorSession,
  ClientProjection,
  CoordinatorResponse,
  GenerationProjection,
  MailboxBindingProjection,
  MailboxJobProjection,
  MutationReceipt,
  ProfileProjection,
} from './types';

function segment(value: string): string {
  if (!value || value.includes('/') || value.includes('\\')) {
    throw new TypeError('Opaque identifiers cannot contain path separators');
  }
  return encodeURIComponent(value);
}

async function mutationBody<T extends Record<string, unknown>>(body: T): Promise<T & { requestDigest: string }> {
  return { ...body, requestDigest: await sha256Hex(body) };
}

async function mutate(
  path: string,
  tenantId: string,
  method: 'POST' | 'PUT' | 'DELETE',
  body: Record<string, unknown>,
): Promise<MutationReceipt | undefined> {
  return requestJson<MutationReceipt>(path, {
    tenantId,
    method,
    body: await mutationBody(body),
    idempotencyKey: newIdempotencyKey(),
  });
}

export function getSession(tenantId: string, signal?: AbortSignal): Promise<ActorSession | undefined> {
  return requestJson<ActorSession>('/api/v1/session', { tenantId, signal });
}

export function getClient(tenantId: string, clientId: string): Promise<ClientProjection | undefined> {
  return requestJson<ClientProjection>(`/api/v1/tenants/${segment(tenantId)}/clients/${segment(clientId)}`, { tenantId });
}

export function createClient(
  tenantId: string,
  input: { clientId: string; kind: 'PERSON' | 'ORGANIZATION'; displayName: string },
): Promise<MutationReceipt | undefined> {
  return mutate(`/api/v1/tenants/${segment(tenantId)}/clients`, tenantId, 'POST', input);
}

export function setClientGrant(
  tenantId: string,
  clientId: string,
  actorId: string,
  input: { role: 'CLIENT_VIEWER' | 'CLIENT_EDITOR'; reason: string; expectedClientVersion: number },
  revoke = false,
): Promise<MutationReceipt | undefined> {
  return mutate(
    `/api/v1/tenants/${segment(tenantId)}/clients/${segment(clientId)}/grants/${segment(actorId)}`,
    tenantId,
    revoke ? 'DELETE' : 'PUT',
    input,
  );
}

export function getProfile(tenantId: string, profileId: string): Promise<ProfileProjection | undefined> {
  return requestJson<ProfileProjection>(`/api/v1/tenants/${segment(tenantId)}/profiles/${segment(profileId)}`, { tenantId });
}

export function createProfile(tenantId: string, profileId: string): Promise<MutationReceipt | undefined> {
  return mutate(`/api/v1/tenants/${segment(tenantId)}/profiles`, tenantId, 'POST', { profileId });
}

export function assignProfile(
  tenantId: string,
  profileId: string,
  input: { assignmentId: string; clientId: string; reason: string; expectedProfileVersion: number },
): Promise<MutationReceipt | undefined> {
  return mutate(`/api/v1/tenants/${segment(tenantId)}/profiles/${segment(profileId)}/assignment`, tenantId, 'PUT', input);
}

export function setProfileGrant(
  tenantId: string,
  profileId: string,
  actorId: string,
  input: { role: 'PROFILE_VIEWER' | 'PROFILE_OPERATOR'; reason: string; expectedProfileVersion: number },
  revoke = false,
): Promise<MutationReceipt | undefined> {
  return mutate(
    `/api/v1/tenants/${segment(tenantId)}/profiles/${segment(profileId)}/grants/${segment(actorId)}`,
    tenantId,
    revoke ? 'DELETE' : 'PUT',
    input,
  );
}

export function getGeneration(
  tenantId: string,
  profileId: string,
  generationId: string,
): Promise<GenerationProjection | undefined> {
  return requestJson<GenerationProjection>(
    `/api/v1/tenants/${segment(tenantId)}/profiles/${segment(profileId)}/generations/${segment(generationId)}`,
    { tenantId },
  );
}

export function registerGeneration(
  tenantId: string,
  profileId: string,
  input: { generationId: string; objectKey: string; metadataDigest: string; containerDigest: string },
): Promise<MutationReceipt | undefined> {
  return mutate(`/api/v1/tenants/${segment(tenantId)}/profiles/${segment(profileId)}/generations`, tenantId, 'POST', input);
}

export function verifyGeneration(
  tenantId: string,
  profileId: string,
  generationId: string,
  input: { expectedGenerationVersion: number; verificationReference: string },
): Promise<MutationReceipt | undefined> {
  return mutate(
    `/api/v1/tenants/${segment(tenantId)}/profiles/${segment(profileId)}/generations/${segment(generationId)}/verify`,
    tenantId,
    'POST',
    input,
  );
}

export function changeGenerationActivation(
  tenantId: string,
  profileId: string,
  generationId: string,
  expectedProfileVersion: number,
  activate: boolean,
): Promise<MutationReceipt | undefined> {
  const action = activate ? 'activate' : 'deactivate';
  return mutate(
    `/api/v1/tenants/${segment(tenantId)}/profiles/${segment(profileId)}/generations/${segment(generationId)}/${action}`,
    tenantId,
    'POST',
    { expectedProfileVersion },
  );
}

export function quarantineGeneration(
  tenantId: string,
  profileId: string,
  generationId: string,
  expectedGenerationVersion: number,
): Promise<MutationReceipt | undefined> {
  return mutate(
    `/api/v1/tenants/${segment(tenantId)}/profiles/${segment(profileId)}/generations/${segment(generationId)}/quarantine`,
    tenantId,
    'POST',
    { expectedGenerationVersion },
  );
}

export function getMailboxBinding(tenantId: string, bindingId: string): Promise<MailboxBindingProjection | undefined> {
  return requestJson<MailboxBindingProjection>(`/api/v1/tenants/${segment(tenantId)}/mailboxes/${segment(bindingId)}`, { tenantId });
}

export function createMailboxBinding(
  tenantId: string,
  input: { bindingId: string; provider: 'GMAIL_API' | 'IMAP' | 'BROWSER_FALLBACK'; secretHandle: string },
): Promise<MutationReceipt | undefined> {
  return mutate(`/api/v1/tenants/${segment(tenantId)}/mailboxes`, tenantId, 'POST', input);
}

export function revokeMailboxBinding(
  tenantId: string,
  bindingId: string,
  expectedBindingVersion: number,
): Promise<MutationReceipt | undefined> {
  return mutate(`/api/v1/tenants/${segment(tenantId)}/mailboxes/${segment(bindingId)}/revoke`, tenantId, 'POST', { expectedBindingVersion });
}

export function getMailboxJob(
  tenantId: string,
  bindingId: string,
  jobId: string,
): Promise<MailboxJobProjection | undefined> {
  return requestJson<MailboxJobProjection>(
    `/api/v1/tenants/${segment(tenantId)}/mailboxes/${segment(bindingId)}/jobs/${segment(jobId)}`,
    { tenantId },
  );
}

export function createMailboxJob(
  tenantId: string,
  bindingId: string,
  input: { jobId: string; cursor: string | null; delayMs: number; maxAttempts: number },
): Promise<MutationReceipt | undefined> {
  return mutate(`/api/v1/tenants/${segment(tenantId)}/mailboxes/${segment(bindingId)}/jobs`, tenantId, 'POST', input);
}

export function runMailboxJob(
  tenantId: string,
  bindingId: string,
  jobId: string,
  expectedJobVersion: number,
): Promise<MutationReceipt | undefined> {
  return mutate(
    `/api/v1/tenants/${segment(tenantId)}/mailboxes/${segment(bindingId)}/jobs/${segment(jobId)}/run`,
    tenantId,
    'POST',
    { expectedJobVersion },
  );
}

export function getCoordinator(tenantId: string, profileId: string): Promise<CoordinatorResponse | undefined> {
  return requestJson<CoordinatorResponse>(`/api/v1/tenants/${segment(tenantId)}/profiles/${segment(profileId)}/coordinator`, { tenantId });
}

export function commandCoordinator(
  tenantId: string,
  profileId: string,
  input: Record<string, unknown>,
): Promise<CoordinatorResponse | undefined> {
  return requestJson<CoordinatorResponse>(
    `/api/v1/tenants/${segment(tenantId)}/profiles/${segment(profileId)}/coordinator`,
    { tenantId, method: 'POST', body: input },
  );
}
