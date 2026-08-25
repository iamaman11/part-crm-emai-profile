import { getAuthenticatedSession as getSessionOperation } from '../../shared/api/generated/operations';
import type { ActorSession } from '../../shared/api/generated/operations';

export function getSession(tenantId: string, signal?: AbortSignal): Promise<ActorSession> {
  return getSessionOperation({
    tenantId,
    ...(signal === undefined ? {} : { signal }),
  });
}
