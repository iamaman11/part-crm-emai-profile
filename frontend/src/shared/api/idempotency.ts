export function newIdempotencyKey(): string {
  return `idem_${crypto.randomUUID().replaceAll('-', '')}`;
}
