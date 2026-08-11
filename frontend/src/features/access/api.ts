import { requestJson } from '../../shared/api/client';
import { mutate, pagedPath, segment } from '../../shared/api/endpoint';
import type { MemberListPageDto } from '../../shared/api/generated/operator-query';

export function bootstrapOwner(
  tenantId: string,
  input: { actorId: string; identityId: string; tenantDisplayName: string },
) {
  return mutate(`/api/v1/tenants/${segment(tenantId)}/owner/bootstrap`, tenantId, 'POST', input);
}

export function transferOwner(
  tenantId: string,
  input: { nextOwnerActorId: string; currentOwnerVersion: number; nextOwnerVersion: number },
) {
  return mutate(`/api/v1/tenants/${segment(tenantId)}/owner/transfer`, tenantId, 'POST', input);
}

export function createInvitation(
  tenantId: string,
  input: { invitationId: string; invitedContactHmac: string; expiresAtMs: number; expectedTenantVersion: number },
) {
  return mutate(`/api/v1/tenants/${segment(tenantId)}/invitations`, tenantId, 'POST', input);
}

export function acceptInvitation(
  tenantId: string,
  invitationId: string,
  input: { identityId: string; actorId: string },
) {
  return mutate(`/api/v1/tenants/${segment(tenantId)}/invitations/${segment(invitationId)}/accept`, tenantId, 'POST', input);
}

export function listMembers(
  tenantId: string,
  signal?: AbortSignal,
  cursor?: string | null,
  limit = 50,
): Promise<MemberListPageDto | undefined> {
  return requestJson<MemberListPageDto>(
    pagedPath(`/api/v1/tenants/${segment(tenantId)}/members`, cursor, limit),
    { tenantId, signal },
  );
}

export function updateMembershipStatus(
  tenantId: string,
  actorId: string,
  input: { status: 'ACTIVE' | 'SUSPENDED' | 'REVOKED'; expectedVersion: number },
) {
  return mutate(`/api/v1/tenants/${segment(tenantId)}/members/${segment(actorId)}/status`, tenantId, 'PUT', input);
}
