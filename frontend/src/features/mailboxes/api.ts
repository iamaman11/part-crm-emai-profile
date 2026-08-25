import {
  changeMailboxClientAssociation as changeMailboxClientAssociationOperation,
  createMailboxBinding as createMailboxBindingOperation,
  createMailboxJob as createMailboxJobOperation,
  getMailboxBinding as getMailboxBindingOperation,
  getMailboxClientAssociation as getMailboxClientAssociationOperation,
  getMailboxJob as getMailboxJobOperation,
  listMailboxes as listMailboxesOperation,
  revokeMailboxBinding as revokeMailboxBindingOperation,
  runMailboxJob as runMailboxJobOperation,
} from '../../shared/api/generated/operations';
import type {
  ChangeMailboxClientAssociationRequestDto,
  CreateMailboxBindingRequest,
  CreateMailboxJobRequest,
  MailboxBindingResponse,
  MailboxClientAssociationMutationReceiptDto,
  MailboxClientAssociationProjectionDto,
  MailboxJobResponse,
  MailboxListItemDto,
  MailboxListPageDto,
  MutationReceipt,
  RevokeMailboxBindingRequest,
  RunMailboxJobRequest,
} from '../../shared/api/generated/operations';
import { newIdempotencyKey } from '../../shared/api/idempotency';

export type CreateMailboxBindingInput = CreateMailboxBindingRequest;
export type RevokeMailboxBindingInput = RevokeMailboxBindingRequest;
export type CreateMailboxJobInput = CreateMailboxJobRequest;
export type RunMailboxJobInput = RunMailboxJobRequest;
export type ChangeMailboxClientAssociationInput = ChangeMailboxClientAssociationRequestDto;
export type MailboxBindingProjection = MailboxBindingResponse;
export type MailboxJobProjection = MailboxJobResponse;
export type MailboxClientAssociationProjection = MailboxClientAssociationProjectionDto;
export type MailboxClientAssociationMutationReceipt = MailboxClientAssociationMutationReceiptDto;
export type { MailboxListItemDto, MailboxListPageDto };

export interface MailboxRelationshipOverviewItem {
  mailbox: MailboxListItemDto;
  association: MailboxClientAssociationProjection;
}

export interface MailboxRelationshipOverviewPage {
  items: ReadonlyArray<MailboxRelationshipOverviewItem>;
  nextCursor: string | null;
}

export function listMailboxes(
  tenantId: string,
  signal?: AbortSignal,
  cursor?: string | null,
  limit = 50,
): Promise<MailboxListPageDto> {
  return listMailboxesOperation({
    tenantId,
    limit,
    ...(cursor === undefined || cursor === null ? {} : { cursor }),
    ...(signal === undefined ? {} : { signal }),
  });
}

export async function listMailboxRelationshipOverview(
  tenantId: string,
  cursor?: string | null,
  limit = 25,
): Promise<MailboxRelationshipOverviewPage> {
  const page = await listMailboxes(tenantId, undefined, cursor, limit);
  const items = await Promise.all(
    page.mailboxes.map(async (mailbox) => ({
      mailbox,
      association: await getMailboxClientAssociation(tenantId, mailbox.bindingId),
    })),
  );
  return { items, nextCursor: page.nextCursor };
}

export function getMailboxBinding(
  tenantId: string,
  bindingId: string,
): Promise<MailboxBindingProjection> {
  return getMailboxBindingOperation({ tenantId, bindingId });
}

export function createMailboxBinding(
  tenantId: string,
  input: CreateMailboxBindingInput,
): Promise<MutationReceipt> {
  return createMailboxBindingOperation({
    tenantId,
    body: input,
    idempotencyKey: newIdempotencyKey(),
  });
}

export function revokeMailboxBinding(
  tenantId: string,
  bindingId: string,
  expectedBindingVersion: number,
): Promise<MutationReceipt> {
  return revokeMailboxBindingOperation({
    tenantId,
    bindingId,
    body: { expectedBindingVersion },
    idempotencyKey: newIdempotencyKey(),
  });
}

export function getMailboxClientAssociation(
  tenantId: string,
  bindingId: string,
): Promise<MailboxClientAssociationProjection> {
  return getMailboxClientAssociationOperation({ tenantId, bindingId });
}

export function changeMailboxClientAssociation(
  tenantId: string,
  bindingId: string,
  input: ChangeMailboxClientAssociationInput,
): Promise<MailboxClientAssociationMutationReceipt> {
  return changeMailboxClientAssociationOperation({
    tenantId,
    bindingId,
    body: input,
    idempotencyKey: newIdempotencyKey(),
  });
}

export function getMailboxJob(
  tenantId: string,
  bindingId: string,
  jobId: string,
): Promise<MailboxJobProjection> {
  return getMailboxJobOperation({ tenantId, bindingId, jobId });
}

export function createMailboxJob(
  tenantId: string,
  bindingId: string,
  input: CreateMailboxJobInput,
): Promise<MutationReceipt> {
  return createMailboxJobOperation({
    tenantId,
    bindingId,
    body: input,
    idempotencyKey: newIdempotencyKey(),
  });
}

export function runMailboxJob(
  tenantId: string,
  bindingId: string,
  jobId: string,
  expectedJobVersion: number,
): Promise<MutationReceipt> {
  return runMailboxJobOperation({
    tenantId,
    bindingId,
    jobId,
    body: { expectedJobVersion },
    idempotencyKey: newIdempotencyKey(),
  });
}
