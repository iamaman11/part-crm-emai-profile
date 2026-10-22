import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiProblem } from '../../shared/api/openapi-runtime';
import { getDeviceInstaller, getDeviceRegistration } from './api';

const tenantId = 'tenant_01JTEST';
const deviceId = 'device_01JTEST';
const registration = { tenantId, deviceId, actorId: 'actor_01JTEST', bindingVersion: 1, status: 'REGISTERED' };

afterEach(() => vi.unstubAllGlobals());

describe('device registration through the real generated HTTP operation', () => {
  it('performs only an authenticated correlated read of the requested registration', async () => {
    const fetch = vi.fn(async (_path: string, _init: RequestInit) => new Response(JSON.stringify(registration), {
      headers: { 'content-type': 'application/json' },
    }));
    vi.stubGlobal('fetch', fetch);
    await expect(getDeviceRegistration(tenantId, deviceId)).resolves.toEqual(registration);
    const [path, init] = fetch.mock.calls[0]!;
    expect(path).toBe(`/api/v1/tenants/${tenantId}/devices/${deviceId}/registration`);
    expect(init.method).toBe('GET');
    expect(init.body).toBeUndefined();
    expect(init.credentials).toBe('same-origin');
    expect(new Headers(init.headers).get('X-Correlation-Id')).toMatch(/^corr_[a-f0-9]{32}$/);
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it.each(['tenantId', 'deviceId'])('rejects a valid wire response for a different %s', async (field) => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ ...registration, [field]: 'another_01JTEST' }), {
      headers: { 'content-type': 'application/json' },
    })));
    await expect(getDeviceRegistration(tenantId, deviceId)).rejects.toThrow('Device registration response failed validation.');
  });

  it('rejects a forged runtime-ready verdict at the generated response boundary', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ ...registration, status: 'READY' }), {
      headers: { 'content-type': 'application/json' },
    })));
    await expect(getDeviceRegistration(tenantId, deviceId)).rejects.toThrow();
  });

  it.each([404, 503])('retains a %s server failure rather than inventing registration success', async (status) => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({
      type: 'https://example.invalid/problems/device-registration', title: 'Registration unavailable',
      status, code: status === 404 ? 'not_found' : 'internal_failure', correlation_id: 'corr_01JTEST',
    }), { status, headers: { 'content-type': 'application/problem+json' } })));
    await expect(getDeviceRegistration(tenantId, deviceId)).rejects.toBeInstanceOf(ApiProblem);
  });
});

describe('deployed Windows installer through the generated operation', () => {
  const releaseSetId = `release-set-v3-sha256-${'a'.repeat(64)}`;
  const downloadUrl = `https://github.com/iamaman11/part-crm-emai-profile/releases/download/${releaseSetId}/ProfileBridgeSetup.exe`;
  it('uses the membership-protected metadata read', async () => {
    const fetch = vi.fn(async (_path: string, _init: RequestInit) => new Response(JSON.stringify({ schemaVersion: 1, releaseSetId, downloadUrl }), {
      headers: { 'content-type': 'application/json' },
    }));
    vi.stubGlobal('fetch', fetch);
    await expect(getDeviceInstaller(tenantId)).resolves.toEqual({ schemaVersion: 1, releaseSetId, downloadUrl });
    expect(fetch.mock.calls[0]?.[0]).toBe(`/api/v1/tenants/${tenantId}/devices/installer`);
  });
  it.each([404, 503])('preserves installer metadata failure %s and correlation', async (status) => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({
      type: 'https://example.invalid/problems/installer', title: 'Installer unavailable',
      status, code: status === 404 ? 'not_found' : 'internal_failure', correlation_id: 'corr_01JTEST',
    }), { status, headers: { 'content-type': 'application/problem+json' } })));
    await expect(getDeviceInstaller(tenantId)).rejects.toMatchObject({ correlationId: 'corr_01JTEST', status });
  });
  it.each([
    'https://example.com/installer.exe',
    'https://github.com/iamaman11/part-crm-emai-profile/releases/latest/download/ProfileBridgeSetup.exe',
    downloadUrl.replace('a'.repeat(64), 'b'.repeat(64)),
  ])('rejects a download outside the exact deployed release: %s', async (url) => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ schemaVersion: 1, releaseSetId, downloadUrl: url }), {
      headers: { 'content-type': 'application/json' },
    })));
    await expect(getDeviceInstaller(tenantId)).rejects.toThrow('Windows installer response failed validation.');
  });
});
