export const REALTIME_INVALIDATION_VERSION = 1 as const;

export const REALTIME_RESOURCES = [
  'clients',
  'profiles',
  'mailboxes',
  'memberships',
  'devices',
  'platform',
] as const;

export type RealtimeResource = (typeof REALTIME_RESOURCES)[number];

export interface RealtimeInvalidationSignal {
  version: typeof REALTIME_INVALIDATION_VERSION;
  eventId: string;
  resource: RealtimeResource;
  occurredAtMs: number;
}

const RESOURCE_SET = new Set<string>(REALTIME_RESOURCES);
const SIGNAL_KEYS = ['eventId', 'occurredAtMs', 'resource', 'version'] as const;
const MAX_EVENT_ID_LENGTH = 200;
const DEFAULT_DEDUPE_CAPACITY = 512;

export function parseRealtimeInvalidationSignal(value: unknown): RealtimeInvalidationSignal | null {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return null;
  const candidate = value as Record<string, unknown>;
  const keys = Object.keys(candidate).sort();
  if (keys.length !== SIGNAL_KEYS.length || keys.some((key, index) => key !== SIGNAL_KEYS[index])) {
    return null;
  }
  if (candidate.version !== REALTIME_INVALIDATION_VERSION) return null;
  if (
    typeof candidate.eventId !== 'string'
    || candidate.eventId.length === 0
    || candidate.eventId.length > MAX_EVENT_ID_LENGTH
    || !candidate.eventId.startsWith('outbox_')
  ) {
    return null;
  }
  if (typeof candidate.resource !== 'string' || !RESOURCE_SET.has(candidate.resource)) return null;
  if (
    typeof candidate.occurredAtMs !== 'number'
    || !Number.isSafeInteger(candidate.occurredAtMs)
    || candidate.occurredAtMs < 0
  ) {
    return null;
  }
  return candidate as unknown as RealtimeInvalidationSignal;
}

export function parseRealtimeMessage(raw: string): RealtimeInvalidationSignal | null {
  try {
    return parseRealtimeInvalidationSignal(JSON.parse(raw));
  } catch {
    return null;
  }
}

export class RealtimeEventDeduper {
  readonly #capacity: number;
  readonly #seen = new Set<string>();
  readonly #order: string[] = [];

  constructor(capacity = DEFAULT_DEDUPE_CAPACITY) {
    if (!Number.isSafeInteger(capacity) || capacity < 1 || capacity > 10_000) {
      throw new RangeError('Realtime dedupe capacity is outside the supported range');
    }
    this.#capacity = capacity;
  }

  accept(eventId: string): boolean {
    if (this.#seen.has(eventId)) return false;
    this.#seen.add(eventId);
    this.#order.push(eventId);
    while (this.#order.length > this.#capacity) {
      const oldest = this.#order.shift();
      if (oldest !== undefined) this.#seen.delete(oldest);
    }
    return true;
  }
}

interface RealtimeLocation {
  protocol: string;
  host: string;
}

export function realtimeWebSocketUrl(tenantId: string, location: RealtimeLocation): string {
  if (!tenantId || tenantId.includes('/') || tenantId.includes('\\')) {
    throw new TypeError('Tenant identifier cannot contain path separators');
  }
  const protocol = location.protocol === 'https:' ? 'wss:' : location.protocol === 'http:' ? 'ws:' : null;
  if (protocol === null || !location.host) {
    throw new TypeError('Realtime requires an HTTP(S) same-origin location');
  }
  return `${protocol}//${location.host}/api/v1/tenants/${encodeURIComponent(tenantId)}/notifications/realtime`;
}
