import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TenantProvider } from '../../app/TenantContext';
import {
  authorizeDevicePairing,
  getAuthenticatedDevicePairingSession,
} from './api';
import { DevicePairingPanel } from './DevicePairingPanel';

vi.mock('./api', () => ({
  authorizeDevicePairing: vi.fn(),
  getAuthenticatedDevicePairingSession: vi.fn(),
}));

const mockedAuthorizeDevicePairing = vi.mocked(authorizeDevicePairing);
const mockedGetAuthenticatedDevicePairingSession = vi.mocked(getAuthenticatedDevicePairingSession);

function renderPanel() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <TenantProvider
        loadTenantContexts={async () => ({
          tenants: [{
            tenantId: 'tenant_01JTEST',
            displayName: 'Test organization',
            actorId: 'actor_01JTEST',
            role: 'TENANT_OWNER',
          }],
        })}
      >
        <DevicePairingPanel />
      </TenantProvider>
    </QueryClientProvider>,
  );
}

function authenticatedSession() {
  return {
    tenantId: 'tenant_01JTEST',
    actorId: 'actor_01JTEST',
    role: 'TENANT_OWNER' as const,
    profileId: 'rehearsal-core-v2',
    profileDigest: 'a'.repeat(64),
    capabilities: ['foundation'] as const,
  };
}

describe('DevicePairingPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    window.history.replaceState(null, '', '/devices?tenant=tenant_01JTEST');
  });

  it('starts pairing with only safe tenant and browser-generated device identity', async () => {
    renderPanel();

    const link = await screen.findByRole('link', { name: 'Connect this computer' });
    expect(link.getAttribute('href')).toMatch(
      /^profilebridge:\/\/pair\/start\/tenant_01JTEST\/device_[0-9a-f]{32}$/,
    );
  });

  it('authorizes a pending pairing and clears the capability fragment', async () => {
    const pairingToken = 'a'.repeat(64);
    const challengeToken = 'b'.repeat(64);
    const nonceHex = 'c'.repeat(64);
    const expiresAtMs = Date.now() + 60_000;
    window.history.replaceState(
      null,
      '',
      `/devices?tenant=tenant_01JTEST#pairing=${pairingToken}&device=device_01JTEST`,
    );
    mockedGetAuthenticatedDevicePairingSession.mockResolvedValue(authenticatedSession());
    mockedAuthorizeDevicePairing.mockResolvedValue({
      challengeToken,
      deviceId: 'device_01JTEST',
      nonceHex,
      expiresAtMs,
    });

    renderPanel();
    fireEvent.click(await screen.findByRole('button', { name: 'Authorize this computer' }));

    const finish = await screen.findByRole('link', { name: 'Finish connecting' });
    expect(finish.getAttribute('href')).toBe(
      `profilebridge://pair/complete/tenant_01JTEST/actor_01JTEST/device_01JTEST/${pairingToken}/${challengeToken}/${nonceHex}/${expiresAtMs}`,
    );
    expect(window.location.hash).toBe('');
    expect(mockedGetAuthenticatedDevicePairingSession).toHaveBeenCalledWith('tenant_01JTEST');
    expect(mockedAuthorizeDevicePairing).toHaveBeenCalledWith('tenant_01JTEST', pairingToken);
    expect(document.body.textContent).not.toContain(pairingToken);
    expect(document.body.textContent).not.toContain(challengeToken);
  });

  it('fails closed when the Access-authorized challenge targets another device', async () => {
    const pairingToken = 'a'.repeat(64);
    window.history.replaceState(
      null,
      '',
      `/devices?tenant=tenant_01JTEST#pairing=${pairingToken}&device=device_01JTEST`,
    );
    mockedGetAuthenticatedDevicePairingSession.mockResolvedValue(authenticatedSession());
    mockedAuthorizeDevicePairing.mockResolvedValue({
      challengeToken: 'b'.repeat(64),
      deviceId: 'device_01JOTHER',
      nonceHex: 'c'.repeat(64),
      expiresAtMs: Date.now() + 60_000,
    });

    renderPanel();
    fireEvent.click(await screen.findByRole('button', { name: 'Authorize this computer' }));

    expect(await screen.findByText('Device pairing response failed validation.')).toBeTruthy();
    expect(screen.queryByRole('link', { name: 'Finish connecting' })).toBeNull();
    expect(window.location.hash).toContain('pairing=');
  });

  it('renders the native completion receipt without exposing a credential', async () => {
    window.history.replaceState(
      null,
      '',
      '/devices?tenant=tenant_01JTEST#connected=device_01JTEST',
    );

    renderPanel();

    expect(await screen.findByText('This computer is connected')).toBeTruthy();
    expect(screen.getByText('device_01JTEST')).toBeTruthy();
    expect(mockedAuthorizeDevicePairing).not.toHaveBeenCalled();
  });
});
