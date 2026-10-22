import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TenantProvider, useTenant } from '../../app/TenantContext';
import { bootstrapOwner } from './api';
import { OrganizationSetup } from './OrganizationSetup';

vi.mock('./api', () => ({ bootstrapOwner: vi.fn() }));
const bootstrap = vi.mocked(bootstrapOwner);
type Loader = NonNullable<Parameters<typeof TenantProvider>[0]['loadTenantContexts']>;

function Probe() {
  const { tenantId } = useTenant();
  return <output>{tenantId || 'no-active-organization'}</output>;
}

function show(loadTenantContexts: Loader) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}><TenantProvider loadTenantContexts={loadTenantContexts}><OrganizationSetup /><Probe /></TenantProvider></QueryClientProvider>);
}

describe('first organization setup', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    window.history.replaceState(null, '', '/');
  });

  it('offers no owner action unless the server explicitly permits it', async () => {
    show(async () => ({ tenants: [] }));
    expect(await screen.findByText('Organization access required')).toBeTruthy();
    expect(screen.queryByLabelText('Organization name')).toBeNull();
    expect(bootstrap).not.toHaveBeenCalled();
  });

  it('sends only a name then resolves and selects the new authorized organization', async () => {
    const load = vi.fn<Loader>()
      .mockResolvedValueOnce({ tenants: [], canBootstrapOwner: true })
      .mockResolvedValue({ tenants: [{ tenantId: 'tenant_organization', actorId: 'actor_owner', role: 'TENANT_OWNER', displayName: 'Alegria' }], canBootstrapOwner: false });
    bootstrap.mockResolvedValue({ resultCode: 'bootstrapped', resourceId: 'tenant_organization', aggregateVersion: 1 });
    show(load);
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText('Organization name'), ' Alegria ');
    await user.click(screen.getByRole('button', { name: 'Create organization' }));
    expect(await screen.findByText('tenant_organization')).toBeTruthy();
    expect(bootstrap).toHaveBeenCalledWith({ tenantDisplayName: 'Alegria' }, expect.any(String));
    expect(load).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole('button', { name: 'Create organization' })).toBeNull();
  });

  it('retries the same logical request after a network error', async () => {
    bootstrap.mockRejectedValueOnce(new Error('Network unavailable'))
      .mockResolvedValue({ resultCode: 'bootstrapped', resourceId: 'tenant_organization', aggregateVersion: 1 });
    show(async () => ({ tenants: [], canBootstrapOwner: true }));
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText('Organization name'), 'Alegria');
    await user.click(screen.getByRole('button', { name: 'Create organization' }));
    await user.click(await screen.findByRole('button', { name: 'Retry previous setup request' }));
    await waitFor(() => expect(bootstrap).toHaveBeenCalledTimes(2));
    expect(bootstrap.mock.calls[1]).toEqual(bootstrap.mock.calls[0]);
  });

  it('does not submit a blank organization name', async () => {
    show(async () => ({ tenants: [], canBootstrapOwner: true }));
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText('Organization name'), '   ');
    await user.click(screen.getByRole('button', { name: 'Create organization' }));
    expect(bootstrap).not.toHaveBeenCalled();
  });
});
