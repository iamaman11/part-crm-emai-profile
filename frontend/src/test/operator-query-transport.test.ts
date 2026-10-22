import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  listClientProfiles,
  listMailboxes,
  listMembers,
  listProfiles,
} from '../shared/api/generated/operations';
import { ApiProblem } from '../shared/api/openapi-runtime';

const tenantId = 'tenant_alegria_test';
const clientId = 'client_alegria_test_01';
const notFound = {
  type: 'https://example.invalid/problems/not-found',
  title: 'Not Found',
  status: 404,
  code: 'not_found',
  correlation_id: 'corr_unknown',
};

afterEach(() => vi.unstubAllGlobals());

describe('generated operator query HTTP requests', () => {
  it.each([
    { name: 'client profiles', run: () => listClientProfiles({ tenantId, clientId }), resource: `clients/${clientId}/profiles`, collection: 'profiles' },
    { name: 'profiles', run: () => listProfiles({ tenantId }), resource: 'profiles', collection: 'profiles' },
    { name: 'members', run: () => listMembers({ tenantId }), resource: 'members', collection: 'members' },
    { name: 'mailboxes', run: () => listMailboxes({ tenantId }), resource: 'mailboxes', collection: 'mailboxes' },
  ])('sends server-required correlation for $name and accepts an empty authorized page', async ({ run, resource, collection }) => {
    const fetch = vi.fn(async (_path: string, init: RequestInit) => {
      const correlation = new Headers(init.headers).get('X-Correlation-Id');
      // The shipping Worker rejects a missing correlation before identity resolution.
      if (!correlation) {
        return new Response(JSON.stringify(notFound), { status: 404, headers: { 'content-type': 'application/problem+json' } });
      }
      return new Response(JSON.stringify({ [collection]: [], nextCursor: null }), { headers: { 'content-type': 'application/json' } });
    });
    vi.stubGlobal('fetch', fetch);

    await expect(run()).resolves.toEqual({ [collection]: [], nextCursor: null });
    const [path, init] = fetch.mock.calls[0]!;
    expect(path).toBe(`/api/v1/tenants/${tenantId}/${resource}`);
    expect(new Headers(init.headers).get('X-Correlation-Id')).toMatch(/^corr_[a-f0-9]{32}$/);
    expect(init.method).toBe('GET');
    expect(init.credentials).toBe('same-origin');
  });

  it('preserves an actual not_found instead of treating it as an empty profile page', async () => {
    const fetch = vi.fn(async () => new Response(JSON.stringify(notFound), {
      status: 404,
      headers: { 'content-type': 'application/problem+json' },
    }));
    vi.stubGlobal('fetch', fetch);

    await expect(listClientProfiles({ tenantId, clientId })).rejects.toBeInstanceOf(ApiProblem);
  });
});
