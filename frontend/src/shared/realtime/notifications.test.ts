import { describe, expect, it } from 'vitest';
import {
  parseRealtimeInvalidationSignal,
  parseRealtimeMessage,
  realtimeWebSocketUrl,
  RealtimeEventDeduper,
} from './notifications';

const signal = {
  version: 1,
  eventId: 'outbox_01JREALTIME',
  resource: 'clients',
  occurredAtMs: 42,
} as const;

describe('realtime invalidation contract', () => {
  it('accepts only the closed metadata-safe shape', () => {
    expect(parseRealtimeInvalidationSignal(signal)).toEqual(signal);
    expect(parseRealtimeInvalidationSignal({ ...signal, aggregateId: 'client_confidential' })).toBeNull();
    expect(parseRealtimeInvalidationSignal({ ...signal, payload: { body: 'secret' } })).toBeNull();
    expect(parseRealtimeInvalidationSignal({ ...signal, resource: 'contacts' })).toBeNull();
    expect(parseRealtimeInvalidationSignal({ ...signal, version: 2 })).toBeNull();
  });

  it('fails closed for malformed frames', () => {
    expect(parseRealtimeMessage('not-json')).toBeNull();
    expect(parseRealtimeMessage(JSON.stringify({ ...signal, eventId: '' }))).toBeNull();
    expect(parseRealtimeMessage(JSON.stringify({ ...signal, occurredAtMs: -1 }))).toBeNull();
  });
});

describe('realtime duplicate suppression', () => {
  it('suppresses duplicate event ids without suppressing later events', () => {
    const deduper = new RealtimeEventDeduper(2);
    expect(deduper.accept('outbox_a')).toBe(true);
    expect(deduper.accept('outbox_a')).toBe(false);
    expect(deduper.accept('outbox_b')).toBe(true);
    expect(deduper.accept('outbox_c')).toBe(true);
    expect(deduper.accept('outbox_a')).toBe(true);
  });
});

describe('realtime connection URL', () => {
  it('is same-origin and upgrades HTTP schemes only', () => {
    expect(realtimeWebSocketUrl('tenant_01', { protocol: 'https:', host: 'crm.example' })).toBe(
      'wss://crm.example/api/v1/tenants/tenant_01/notifications/realtime',
    );
    expect(realtimeWebSocketUrl('tenant_01', { protocol: 'http:', host: 'localhost:5173' })).toBe(
      'ws://localhost:5173/api/v1/tenants/tenant_01/notifications/realtime',
    );
    expect(() => realtimeWebSocketUrl('tenant_01', { protocol: 'file:', host: '' })).toThrow(TypeError);
  });
});
