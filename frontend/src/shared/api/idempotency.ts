export function newIdempotencyKey(): string {
  return globalThis.crypto.randomUUID();
}
