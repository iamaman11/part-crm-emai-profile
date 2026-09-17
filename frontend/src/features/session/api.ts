import { getAuthenticatedSession as getSessionOperation } from '../../shared/api/generated/operations';
import type { ActivationUnit, ActorSession, TenantContextsProjection } from '../../shared/api/generated/operations';
import { getAuthenticatedTenantContexts as getTenantContextsOperation } from '../../shared/api/generated/operations';

export type { ActivationUnit, ActorSession, TenantContextsProjection };

export function getTenantContexts(signal?: AbortSignal): Promise<TenantContextsProjection> {
  return getTenantContextsOperation(signal === undefined ? {} : { signal });
}

export function getSession(tenantId: string, signal?: AbortSignal): Promise<ActorSession> {
  return getSessionOperation({
    tenantId,
    ...(signal === undefined ? {} : { signal }),
  });
}
