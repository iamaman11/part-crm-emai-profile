import { requestJson } from '../../shared/api/client';
import type { ActorSession } from '../../shared/api/generated/control-plane';

export function getSession(tenantId: string, signal?: AbortSignal): Promise<ActorSession | undefined> {
  return requestJson<ActorSession>('/api/v1/session', { tenantId, signal });
}
