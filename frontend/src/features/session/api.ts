import { getAuthenticatedSession } from '../../shared/api/generated/operations';
import type { ActorSession } from '../../shared/api/generated/operations';

export function getSession(tenantId: string, signal?: AbortSignal): Promise<ActorSession> {
  return getAuthenticatedSession({
    tenantId,
    ...(signal === undefined ? {} : { signal }),
  });
}
