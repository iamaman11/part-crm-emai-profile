import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiProblem, requestJson } from './client';

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('requestJson', () => {
  it('rejects non-same-origin API paths before fetch', async () => {
    const fetchSpy = vi.fn();
    vi.stubGlobal('fetch', fetchSpy);

    await expect(requestJson('https://example.com/api/v1/session', { tenantId: 'tenant_01JTEST' }))
      .rejects.toThrow('same-origin');
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('uses same-origin credentials and required request metadata', async () => {
    const fetchSpy = vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) => {
      expect(init?.credentials).toBe('same-origin');
      expect(init?.redirect).toBe('error');
      const headers = new Headers(init?.headers);
      expect(headers.get('X-Tenant-Id')).toBe('tenant_01JTEST');
      expect(headers.get('X-Correlation-Id')).toMatch(/^corr_[0-9a-f]+$/);
      return new Response(JSON.stringify({ tenantId: 'tenant_01JTEST' }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    });
    vi.stubGlobal('fetch', fetchSpy);

    await expect(requestJson<{ tenantId: string }>('/api/v1/session', { tenantId: 'tenant_01JTEST' }))
      .resolves.toEqual({ tenantId: 'tenant_01JTEST' });
    expect(fetchSpy).toHaveBeenCalledTimes(1);
  });

  it('normalizes stable problem payloads without exposing arbitrary response bodies', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({
      type: 'urn:part-crm:problem:not-found',
      title: 'Not Found',
      status: 404,
      code: 'not_found',
      correlation_id: 'corr_01JTEST',
      ignored: 'must not become part of the error surface',
    }), {
      status: 404,
      headers: { 'content-type': 'application/problem+json' },
    })));

    const promise = requestJson('/api/v1/tenants/tenant_01JTEST/clients/client_01JTEST', {
      tenantId: 'tenant_01JTEST',
    });

    await expect(promise).rejects.toMatchObject<ApiProblem>({
      name: 'ApiProblem',
      status: 404,
      code: 'not_found',
      correlationId: 'corr_01JTEST',
      message: 'Not Found',
    });
  });
});
