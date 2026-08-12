import { newIdempotencyKey, requestJson, sha256Hex } from '../../shared/api/client';
import { mutate, pagedPath, segment } from '../../shared/api/endpoint';
import type { MutationReceipt } from '../../shared/api/generated/control-plane';
import type {
  ChangeMailboxClientAssociationRequestDto,
  MailboxClientAssociationMutationReceiptDto,
  MailboxClientAssociationProjectionDto,
} from '../../shared/api/generated/mailbox-client-association';
import type {
  CreateMailboxBindingRequestDto,
  CreateMailboxJobRequestDto,
  MailboxBindingProjectionDto,
  MailboxJobProjectionDto,
  RevokeMailboxBindingRequestDto,
  RunMailboxJobRequestDto,
} from '../../shared/api/generated/mailbox';
import type { MailboxListPageDto } from '../../shared/api/generated/operator-query';

export type CreateMailboxBindingInput = Omit<CreateMailboxBindingRequestDto, 'requestDigest'>;
export type RevokeMailboxBindingInput = Omit<RevokeMailboxBindingRequestDto, 'requestDigest'>;
export type CreateMailboxJobInput = Omit<CreateMailboxJobRequestDto, 'requestDigest'>;
export type RunMailboxJobInput = Omit<RunMailboxJobRequestDto, 'requestDigest'>;
export type ChangeMailboxClientAssociationInput = Omit<
  ChangeMailboxClientAssociationRequestDto,
  'requestDigest'
>;
export type MailboxBindingProjection = MailboxBindingProjectionDto;
export type MailboxJobProjection = MailboxJobProjectionDto;
export type MailboxClientAssociationProjection = MailboxClientAssociationProjectionDto;
export type MailboxClientAssociationMutationReceipt = MailboxClientAssociationMutationReceiptDto;

export function listMailboxes(
  tenantId: string,
  signal?: AbortSignal,
  cursor?: string | null,
  limit = 50,
): Promise<MailboxListPageDto | undefined> {
  return requestJson<MailboxListPageDto>(
    pagedPath(`/api/v1/tenants/${segment(tenantId)}/mailboxes`, cursor, limit),
    { tenantId, signal },
  );
}

export function getMailboxBinding(tenantId: string, bindingId: string): Promise<MailboxBindingProjection | undefined> {
  return requestJson<MailboxBindingProjection>(
    `/api/v1/tenants/${segment(tenantId)}/mailboxes/${segment(bindingId)}`,
    { tenantId },
  );
}

export function createMailboxBinding(
  tenantId: string,
  input: CreateMailboxBindingInput,
): Promise<MutationReceipt | undefined> {
  return mutate(`/api/v1/tenants/${segment(tenantId)}/mailboxes`, tenantId, 'POST', input);
}

export function revokeMailboxBinding(
  tenantId: string,
  bindingId: string,
  expectedBindingVersion: number,
): Promise<MutationReceipt | undefined> {
  const input: RevokeMailboxBindingInput = { expectedBindingVersion };
  return mutate(`/api/v1/tenants/${segment(tenantId)}/mailboxes/${segment(bindingId)}/revoke`, tenantId, 'POST', input);
}

export function getMailboxClientAssociation(
  tenantId: string,
  bindingId: string,
): Promise<MailboxClientAssociationProjection | undefined> {
  return requestJson<MailboxClientAssociationProjection>(
    `/api/v1/tenants/${segment(tenantId)}/mailboxes/${segment(bindingId)}/client-association`,
    { tenantId },
  );
}

export async function changeMailboxClientAssociation(
  tenantId: string,
  bindingId: string,
  input: ChangeMailboxClientAssociationInput,
): Promise<MailboxClientAssociationMutationReceipt | undefined> {
  const command = {
    clientId: input.clientId,
    expectedRelationshipVersion: input.expectedRelationshipVersion,
  };
  const payload: ChangeMailboxClientAssociationRequestDto = {
    ...command,
    requestDigest: await sha256Hex(command),
  };
  return requestJson<MailboxClientAssociationMutationReceipt>(
    `/api/v1/tenants/${segment(tenantId)}/mailboxes/${segment(bindingId)}/client-association`,
    {
      tenantId,
      method: 'POST',
      body: payload,
      idempotencyKey: newIdempotencyKey(),
    },
  );
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
  input: CreateMailboxJobInput,
): Promise<MutationReceipt | undefined> {
  return mutate(`/api/v1/tenants/${segment(tenantId)}/mailboxes/${segment(bindingId)}/jobs`, tenantId, 'POST', input);
}

export function runMailboxJob(
  tenantId: string,
  bindingId: string,
  jobId: string,
  expectedJobVersion: number,
): Promise<MutationReceipt | undefined> {
  const input: RunMailboxJobInput = { expectedJobVersion };
  return mutate(
    `/api/v1/tenants/${segment(tenantId)}/mailboxes/${segment(bindingId)}/jobs/${segment(jobId)}/run`,
    tenantId,
    'POST',
    input,
  );
}
