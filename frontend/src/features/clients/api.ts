import {
  archiveClient as archiveClientOperation,
  archiveClientContact as archiveClientContactOperation,
  createClient as createClientOperation,
  getClient as getClientOperation,
  getClientHistory as getClientHistoryOperation,
  getClientMailMessage as getClientMailMessageOperation,
  grantClientAccess as grantClientAccessOperation,
  listClients as listClientsOperation,
  mergeClient as mergeClientOperation,
  revokeClientAccess as revokeClientAccessOperation,
  searchClientMail as searchClientMailOperation,
  sendClientMail as sendClientMailOperation,
  updateClient as updateClientOperation,
  upsertClientContact as upsertClientContactOperation,
} from '../../shared/api/generated/operations';
import type {
  ClientArchiveRequest,
  ClientContactArchiveRequest,
  ClientContactUpsertRequest,
  ClientCreateRequest,
  ClientGrantRequest,
  ClientHistoryProjection,
  ClientListProjection,
  ClientMailSearchInput,
  ClientMailSendReceiptDto,
  ClientMailSendRequestDto,
  ClientMergeRequest,
  ClientUpdateRequest,
  ClientView,
  MailMessageBodyDto,
  MailMessageSearchPageDto,
  MailboxMessageReferenceDto,
  MutationReceipt,
} from '../../shared/api/generated/operations';

export type CreateClientInput = ClientCreateRequest;
export type SetClientGrantInput = ClientGrantRequest;
export type UpdateClientInput = ClientUpdateRequest;
export type ArchiveClientInput = ClientArchiveRequest;
export type UpsertClientContactInput = ClientContactUpsertRequest;
export type ArchiveClientContactInput = ClientContactArchiveRequest;
export type MergeClientInput = ClientMergeRequest;
export type ClientProjection = ClientView;
export type {
  ClientHistoryProjection,
  ClientListProjection,
  ClientMailSearchInput,
  ClientMailSendReceiptDto,
  ClientMailSendRequestDto,
  MailMessageBodyDto,
  MailMessageSearchPageDto,
  MailboxMessageReferenceDto,
};

export function listClients(tenantId: string, signal?: AbortSignal): Promise<ClientListProjection> {
  return listClientsOperation({
    tenantId,
    ...(signal === undefined ? {} : { signal }),
  });
}

export function getClient(tenantId: string, clientId: string): Promise<ClientProjection> {
  return getClientOperation({ tenantId, clientId });
}

export function getClientHistory(
  tenantId: string,
  clientId: string,
  signal?: AbortSignal,
): Promise<ClientHistoryProjection> {
  return getClientHistoryOperation({
    tenantId,
    clientId,
    ...(signal === undefined ? {} : { signal }),
  });
}

export function createClient(tenantId: string, input: CreateClientInput, idempotencyKey: string): Promise<MutationReceipt> {
  return createClientOperation({
    tenantId,
    body: input,
    idempotencyKey,
  });
}

export function updateClient(
  tenantId: string,
  clientId: string,
  input: UpdateClientInput,
  idempotencyKey: string,
): Promise<MutationReceipt> {
  return updateClientOperation({
    tenantId,
    clientId,
    body: input,
    idempotencyKey,
  });
}

export function archiveClient(
  tenantId: string,
  clientId: string,
  input: ArchiveClientInput,
  idempotencyKey: string,
): Promise<MutationReceipt> {
  return archiveClientOperation({
    tenantId,
    clientId,
    body: input,
    idempotencyKey,
  });
}

export function upsertClientContact(
  tenantId: string,
  clientId: string,
  contactPointId: string,
  input: UpsertClientContactInput,
  idempotencyKey: string,
): Promise<MutationReceipt> {
  return upsertClientContactOperation({
    tenantId,
    clientId,
    contactPointId,
    body: input,
    idempotencyKey,
  });
}

export function archiveClientContact(
  tenantId: string,
  clientId: string,
  contactPointId: string,
  input: ArchiveClientContactInput,
  idempotencyKey: string,
): Promise<MutationReceipt> {
  return archiveClientContactOperation({
    tenantId,
    clientId,
    contactPointId,
    body: input,
    idempotencyKey,
  });
}

export function mergeClient(
  tenantId: string,
  sourceClientId: string,
  input: MergeClientInput,
  idempotencyKey: string,
): Promise<MutationReceipt> {
  return mergeClientOperation({
    tenantId,
    clientId: sourceClientId,
    body: input,
    idempotencyKey,
  });
}

export function setClientGrant(
  tenantId: string,
  clientId: string,
  actorId: string,
  input: SetClientGrantInput,
  idempotencyKey: string,
  revoke = false,
): Promise<MutationReceipt | undefined> {
  const command = {
    tenantId,
    clientId,
    actorId,
    body: input,
    idempotencyKey,
  };
  return revoke ? revokeClientAccessOperation(command) : grantClientAccessOperation(command);
}

export function searchClientMail(
  tenantId: string,
  clientId: string,
  input: ClientMailSearchInput,
  signal?: AbortSignal,
): Promise<MailMessageSearchPageDto> {
  return searchClientMailOperation({
    tenantId,
    clientId,
    body: input,
    ...(signal === undefined ? {} : { signal }),
  });
}

export function getClientMailMessage(
  tenantId: string,
  clientId: string,
  reference: MailboxMessageReferenceDto,
  signal?: AbortSignal,
): Promise<MailMessageBodyDto> {
  return getClientMailMessageOperation({
    tenantId,
    clientId,
    body: reference,
    ...(signal === undefined ? {} : { signal }),
  });
}

export function sendClientMail(
  tenantId: string,
  clientId: string,
  input: ClientMailSendRequestDto,
  idempotencyKey: string,
  signal?: AbortSignal,
): Promise<ClientMailSendReceiptDto> {
  return sendClientMailOperation({
    tenantId,
    clientId,
    body: input,
    idempotencyKey,
    ...(signal === undefined ? {} : { signal }),
  });
}
