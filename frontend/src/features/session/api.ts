import { getAuthenticatedSession as getSessionOperation } from '../../shared/api/generated/operations';
import type { ActivationUnit, ActorSession } from '../../shared/api/generated/operations';

export type { ActivationUnit, ActorSession };

export function getSession(tenantId: string, signal?: AbortSignal): Promise<ActorSession> {
  return getSessionOperation({
    tenantId,
    ...(signal === undefined ? {} : { signal }),
  });
}
