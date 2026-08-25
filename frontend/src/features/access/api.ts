import {
  acceptTenantInvitation as acceptTenantInvitationOperation,
  bootstrapTenantOwner as bootstrapTenantOwnerOperation,
  createTenantInvitation as createTenantInvitationOperation,
  listMembers as listMembersOperation,
  transferTenantOwner as transferTenantOwnerOperation,
  updateMembershipStatus as updateMembershipStatusOperation,
} from '../../shared/api/generated/operations';
import type { MemberListPageDto } from '../../shared/api/generated/operations';

export function bootstrapOwner(
  tenantId: string,
  input: { actorId: string; identityId: string; tenantDisplayName: string },
  idempotencyKey: string,
) {
  return bootstrapTenantOwnerOperation({
    tenantId,
    body: input,
    idempotencyKey,
  });
}

export function transferOwner(
  tenantId: string,
  input: { nextOwnerActorId: string; currentOwnerVersion: number; nextOwnerVersion: number },
  idempotencyKey: string,
) {
  return transferTenantOwnerOperation({
    tenantId,
    body: input,
    idempotencyKey,
  });
}

export function createInvitation(
  tenantId: string,
  input: { invitationId: string; invitedContactHmac: string; expiresAtMs: number; expectedTenantVersion: number },
  idempotencyKey: string,
) {
  return createTenantInvitationOperation({
    tenantId,
    body: input,
    idempotencyKey,
  });
}

export function acceptInvitation(
  tenantId: string,
  invitationId: string,
  input: { identityId: string; actorId: string },
  idempotencyKey: string,
) {
  return acceptTenantInvitationOperation({
    tenantId,
    invitationId,
    body: input,
    idempotencyKey,
  });
}

export function listMembers(
  tenantId: string,
  signal?: AbortSignal,
  cursor?: string | null,
  limit = 50,
): Promise<MemberListPageDto> {
  return listMembersOperation({
    tenantId,
    limit,
    ...(cursor === null || cursor === undefined ? {} : { cursor }),
    ...(signal === undefined ? {} : { signal }),
  });
}

export function updateMembershipStatus(
  tenantId: string,
  actorId: string,
  input: { status: 'ACTIVE' | 'SUSPENDED' | 'REVOKED'; expectedVersion: number },
  idempotencyKey: string,
) {
  return updateMembershipStatusOperation({
    tenantId,
    actorId,
    body: input,
    idempotencyKey,
  });
}
