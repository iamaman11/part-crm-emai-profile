import { newIdempotencyKey, requestJson, sha256Hex } from './client';
import type { MutationReceipt } from './generated/control-plane';

export function segment(value: string): string {
  if (!value || value.includes('/') || value.includes('\\')) {
    throw new TypeError('Opaque identifiers cannot contain path separators');
  }
  return encodeURIComponent(value);
}

export function pagedPath(path: string, cursor?: string | null, limit = 50): string {
  if (!Number.isInteger(limit) || limit < 1 || limit > 100) {
    throw new RangeError('Query page size must be an integer between 1 and 100');
  }
  const search = new URLSearchParams({ limit: String(limit) });
  if (cursor) search.set('cursor', cursor);
  return `${path}?${search.toString()}`;
}

async function mutationBody<T extends object>(body: T): Promise<T & { requestDigest: string }> {
  return { ...body, requestDigest: await sha256Hex(body) };
}

export function mutate<T extends object>(
  path: string,
  tenantId: string,
  method: 'POST' | 'PUT' | 'PATCH' | 'DELETE',
  body: T,
): Promise<MutationReceipt | undefined> {
  return mutationBody(body).then((payload) => requestJson<MutationReceipt>(path, {
    tenantId,
    method,
    body: payload,
    idempotencyKey: newIdempotencyKey(),
  }));
}
