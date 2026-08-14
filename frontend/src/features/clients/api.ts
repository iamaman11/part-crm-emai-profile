import { requestJson } from '../../shared/api/client';
import { mutate, segment } from '../../shared/api/endpoint';
import type {
  ClientCreateRequest,
  ClientGrantRequest,
  ClientProjection,
  MutationReceipt,
} from '../../shared/api/generated/control-plane';
import type {
  ClientArchiveRequest,
  ClientContactArchiveRequest,
  ClientContactUpsertRequest,
  ClientHistoryProjection,
  ClientListProjection,
  ClientMergeRequest,
  ClientUpdateRequest,
} from '../../shared/api/generated/client-registry';
import type {
  ClientMailSendReceiptDto,
  ClientMailSendRequestDto,
} from '../../shared/api/generated/client-mail-send';
import type {
  ClientMailSearchInput,
  MailMessageBodyDto,
  MailMessageSearchPageDto,
  MailboxMessageReferenceDto,
} from '../../shared/api/generated/query-mail';

export type CreateClientInput = Omit<ClientCreateRequest, 'requestDigest'>;
export type SetClientGrantInput = Omit<ClientGrantRequest, 'requestDigest'>;
export type UpdateClientInput = Omit<ClientUpdateRequest, 'requestDigest'>;
export type ArchiveClientInput = Omit<ClientArchiveRequest, 'requestDigest'>;
export type UpsertClientContactInput = Omit<ClientContactUpsertRequest, 'requestDigest'>;
export type ArchiveClientContactInput = Omit<ClientContactArchiveRequest, 'requestDigest'>;
export type MergeClientInput = Omit<ClientMergeRequest, 'requestDigest'>;
export type { ClientProjection } from '../../shared/api/generated/control-plane';
export type { ClientHistoryProjection, ClientListProjection } from '../../shared/api/generated/client-registry';
export type {
  ClientMailSendReceiptDto,
  ClientMailSendRequestDto,
} from '../../shared/api/generated/client-mail-send';
export type { ClientMailSearchInput, MailMessageBodyDto, MailMessageSearchPageDto, MailboxMessageReferenceDto } from '../../shared/api/generated/query-mail';

export function listClients(tenantId: string, signal?: AbortSignal): Promise<ClientListProjection | undefined> {
  return requestJson<ClientListProjection>(`/api/v1/tenants/${segment(tenantId)}/clients`, { tenantId, signal });
}

export function getClient(tenantId: string, clientId: string): Promise<ClientProjection | undefined> {
  return requestJson<ClientProjection>(`/api/v1/tenants/${segment(tenantId)}/clients/${segment(clientId)}`, { tenantId });
}

export function getClientHistory(
  tenantId: string,
  clientId: string,
  signal?: AbortSignal,
): Promise<ClientHistoryProjection | undefined> {
  return requestJson<ClientHistoryProjection>(
    `/api/v1/tenants/${segment(tenantId)}/clients/${segment(clientId)}/history`,
    { tenantId, signal },
  );
}

export function createClient(tenantId: string, input: CreateClientInput): Promise<MutationReceipt | undefined> {
  return mutate(`/api/v1/tenants/${segment(tenantId)}/clients`, tenantId, 'POST', input);
}

export function updateClient(
  tenantId: string,
  clientId: string,
  input: UpdateClientInput,
): Promise<MutationReceipt | undefined> {
  return mutate(`/api/v1/tenants/${segment(tenantId)}/clients/${segment(clientId)}`, tenantId, 'PATCH', input);
}

export function archiveClient(
  tenantId: string,
  clientId: string,
  input: ArchiveClientInput,
): Promise<MutationReceipt | undefined> {
  return mutate(`/api/v1/tenants/${segment(tenantId)}/clients/${segment(clientId)}/archive`, tenantId, 'POST', input);
}

export function upsertClientContact(
  tenantId: string,
  clientId: string,
  contactPointId: string,
  input: UpsertClientContactInput,
): Promise<MutationReceipt | undefined> {
  return mutate(
    `/api/v1/tenants/${segment(tenantId)}/clients/${segment(clientId)}/contacts/${segment(contactPointId)}`,
    tenantId,
    'PUT',
    input,
  );
}

export function archiveClientContact(
  tenantId: string,
  clientId: string,
  contactPointId: string,
  input: ArchiveClientContactInput,
): Promise<MutationReceipt | undefined> {
  return mutate(
    `/api/v1/tenants/${segment(tenantId)}/clients/${segment(clientId)}/contacts/${segment(contactPointId)}`,
    tenantId,
    'DELETE',
    input,
  );
}

export function mergeClient(
  tenantId: string,
  sourceClientId: string,
  input: MergeClientInput,
): Promise<MutationReceipt | undefined> {
  return mutate(
    `/api/v1/tenants/${segment(tenantId)}/clients/${segment(sourceClientId)}/merge`,
    tenantId,
    'POST',
    input,
  );
}

export function setClientGrant(
  tenantId: string,
  clientId: string,
  actorId: string,
  input: SetClientGrantInput,
  revoke = false,
): Promise<MutationReceipt | undefined> {
  return mutate(
    `/api/v1/tenants/${segment(tenantId)}/clients/${segment(clientId)}/grants/${segment(actorId)}`,
    tenantId,
    revoke ? 'DELETE' : 'PUT',
    input,
  );
}

export function searchClientMail(
  tenantId: string,
  clientId: string,
  input: ClientMailSearchInput,
  signal?: AbortSignal,
): Promise<MailMessageSearchPageDto | undefined> {
  return requestJson<MailMessageSearchPageDto>(
    `/api/v1/tenants/${segment(tenantId)}/clients/${segment(clientId)}/mail/search`,
    { tenantId, method: 'POST', body: input, signal },
  );
}

export function getClientMailMessage(
  tenantId: string,
  clientId: string,
  reference: MailboxMessageReferenceDto,
  signal?: AbortSignal,
): Promise<MailMessageBodyDto | undefined> {
  return requestJson<MailMessageBodyDto>(
    `/api/v1/tenants/${segment(tenantId)}/clients/${segment(clientId)}/mail/message`,
    { tenantId, method: 'POST', body: reference, signal },
  );
}

export function sendClientMail(
  tenantId: string,
  clientId: string,
  input: ClientMailSendRequestDto,
  idempotencyKey: string,
  signal?: AbortSignal,
): Promise<ClientMailSendReceiptDto | undefined> {
  return requestJson<ClientMailSendReceiptDto>(
    `/api/v1/tenants/${segment(tenantId)}/clients/${segment(clientId)}/mail/send`,
    { tenantId, method: 'POST', body: input, idempotencyKey, signal },
  );
}
