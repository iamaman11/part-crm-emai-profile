import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TenantProvider } from '../../app/TenantContext';
import {
  authorizeDevicePairing,
  getAuthenticatedDevicePairingSession,
  getDeviceRegistration,
  getDeviceInstaller,
} from './api';
import { DevicePairingPanel } from './DevicePairingPanel';

vi.mock('./api', () => ({
  authorizeDevicePairing: vi.fn(),
  getAuthenticatedDevicePairingSession: vi.fn(),
  getDeviceRegistration: vi.fn(),
  getDeviceInstaller: vi.fn(),
}));

const mockedAuthorizeDevicePairing = vi.mocked(authorizeDevicePairing);
const mockedGetAuthenticatedDevicePairingSession = vi.mocked(getAuthenticatedDevicePairingSession);
const mockedGetDeviceRegistration = vi.mocked(getDeviceRegistration);

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
    const releaseSetId = `release-set-v3-sha256-${'a'.repeat(64)}`;
    vi.mocked(getDeviceInstaller).mockResolvedValue({
      schemaVersion: 1, releaseSetId,
      downloadUrl: `https://github.com/iamaman11/part-crm-emai-profile/releases/download/${releaseSetId}/ProfileBridgeSetup.exe`,
    });
    window.history.replaceState(null, '', '/devices?tenant=tenant_01JTEST');
  });

  it('offers the exact deployed installer and keeps installation distinct from registration', async () => {
    renderPanel();
    const link = await screen.findByRole('link', { name: 'Install or update for Windows' });
    expect(link.getAttribute('href')).toContain(`release-set-v3-sha256-${'a'.repeat(64)}/ProfileBridgeSetup.exe`);
    expect(screen.queryByText('Device registration confirmed')).toBeNull();
    expect(mockedGetDeviceRegistration).not.toHaveBeenCalled();
  });

  it('exposes the exact local diagnostics command without credentials or remote upload', async () => {
    renderPanel();
    const link = await screen.findByRole('link', { name: 'Скопировать диагностику' });
    expect(link.getAttribute('href')).toBe('profilebridge://diagnostics/copy');
    expect(link.getAttribute('href')).not.toContain('tenant_');
    expect(link.getAttribute('href')).not.toContain('http');
  });

  it('offers retry without an installer link when metadata is unavailable', async () => {
    vi.mocked(getDeviceInstaller).mockRejectedValue(new Error('Installer unavailable'));
    renderPanel();
    expect(await screen.findByRole('button', { name: 'Try loading installer again' })).toBeTruthy();
    expect(screen.queryByRole('link', { name: 'Install or update for Windows' })).toBeNull();
    expect(await screen.findByRole('link', { name: 'Connect this computer' })).toBeTruthy();
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

  it('checks a native completion locator against the server before showing registration', async () => {
    window.history.replaceState(
      null,
      '',
      '/devices?tenant=tenant_01JTEST#connected=device_01JTEST',
    );
    mockedGetDeviceRegistration.mockResolvedValue({
      tenantId: 'tenant_01JTEST', actorId: 'actor_01JTEST', deviceId: 'device_01JTEST',
      bindingVersion: 1, status: 'REGISTERED',
    });

    renderPanel();

    expect(await screen.findByText('Device registration confirmed')).toBeTruthy();
    expect(screen.getByText('device_01JTEST')).toBeTruthy();
    expect(screen.getByText(/Go to Profiles and choose Open browser/)).toBeTruthy();
    const installer = await screen.findByRole('link', { name: 'Install or update for Windows' });
    expect(installer.getAttribute('href')).toContain(`release-set-v3-sha256-${'a'.repeat(64)}/ProfileBridgeSetup.exe`);
    expect(screen.queryByRole('link', { name: 'Connect this computer' })).toBeNull();
    expect(mockedGetDeviceRegistration).toHaveBeenCalledWith('tenant_01JTEST', 'device_01JTEST', expect.any(AbortSignal));
    expect(mockedAuthorizeDevicePairing).not.toHaveBeenCalled();
  });

  it('never treats a forged callback or an unavailable registration as success', async () => {
    window.history.replaceState(null, '', '/devices?tenant=tenant_01JTEST#connected=device_01JTEST');
    mockedGetDeviceRegistration.mockRejectedValue(new Error('Registration unavailable.'));
    renderPanel();
    expect(await screen.findByText('Device registration not confirmed')).toBeTruthy();
    expect(screen.queryByText('This computer is connected')).toBeNull();
    expect(screen.queryByText('Device registration confirmed')).toBeNull();
    expect(mockedAuthorizeDevicePairing).not.toHaveBeenCalled();
  });

  it('keeps a stalled server check pending rather than inferring connection from a callback', async () => {
    window.history.replaceState(null, '', '/devices?tenant=tenant_01JTEST#connected=device_01JTEST');
    mockedGetDeviceRegistration.mockReturnValue(new Promise(() => {}));
    renderPanel();
    expect(await screen.findByText('Checking device registration')).toBeTruthy();
    expect(screen.queryByText('Device registration confirmed')).toBeNull();
  });

  it('retries only the registration read after an unavailable response', async () => {
    window.history.replaceState(null, '', '/devices?tenant=tenant_01JTEST#connected=device_01JTEST');
    mockedGetDeviceRegistration.mockRejectedValueOnce(new Error('Connection unavailable.'));
    mockedGetDeviceRegistration.mockResolvedValueOnce({
      tenantId: 'tenant_01JTEST', actorId: 'actor_01JTEST', deviceId: 'device_01JTEST',
      bindingVersion: 1, status: 'REGISTERED',
    });
    renderPanel();
    fireEvent.click(await screen.findByRole('button', { name: 'Check again' }));
    expect(await screen.findByText('Device registration confirmed')).toBeTruthy();
    expect(mockedGetDeviceRegistration).toHaveBeenCalledTimes(2);
    expect(mockedAuthorizeDevicePairing).not.toHaveBeenCalled();
  });
});
