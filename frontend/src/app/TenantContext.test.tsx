import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getTenantContexts } from '../features/session/api';
import { TenantChooser, TenantProvider, useTenant } from './TenantContext';

vi.mock('../features/session/api', () => ({ getTenantContexts: vi.fn() }));

const mockedGetTenantContexts = vi.mocked(getTenantContexts);

function Probe() {
  const { tenantId } = useTenant();
  return <output>{tenantId || 'none'}</output>;
}

function renderContext() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}><TenantProvider><TenantChooser /><Probe /></TenantProvider></QueryClientProvider>);
}

describe('TenantContext', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    window.history.replaceState(null, '', '/');
  });

  it('shows an explicit no-organization state for zero ACTIVE contexts', async () => {
    mockedGetTenantContexts.mockResolvedValue({ tenants: [] });
    renderContext();
    expect(await screen.findByText('No organization access is active for this identity.')).toBeTruthy();
    expect(screen.getByText('none')).toBeTruthy();
  });

  it('auto-selects exactly one authorized context', async () => {
    mockedGetTenantContexts.mockResolvedValue({ tenants: [{ tenantId: 'tenant_01JONE', displayName: 'One organization', actorId: 'actor_01JONE', role: 'MEMBER' }] });
    renderContext();
    expect(await screen.findByText('Organization: One organization')).toBeTruthy();
    expect(screen.getByText('tenant_01JONE')).toBeTruthy();
  });

  it('rejects a stale URL tenant and requires a human-readable choice for multiple contexts', async () => {
    const user = userEvent.setup();
    window.history.replaceState(null, '', '/?tenant=tenant_stale');
    mockedGetTenantContexts.mockResolvedValue({ tenants: [
      { tenantId: 'tenant_01JA', displayName: 'Alpha organization', actorId: 'actor_01JA', role: 'TENANT_OWNER' },
      { tenantId: 'tenant_01JB', displayName: 'Beta organization', actorId: 'actor_01JB', role: 'MEMBER' },
    ] });
    renderContext();
    const selector = await screen.findByLabelText('Organization');
    expect(screen.getByText('none')).toBeTruthy();
    expect(window.location.search).toBe('');
    await user.selectOptions(selector, 'tenant_01JB');
    expect(await screen.findByText('tenant_01JB')).toBeTruthy();
  });

  it('renders a bounded retry state when bootstrap fails', async () => {
    mockedGetTenantContexts.mockRejectedValue(new Error('bootstrap unavailable'));
    renderContext();
    expect((await screen.findByRole('alert')).textContent).toContain('Unable to resolve your organizations.');
    expect(screen.getByRole('button', { name: 'Retry' })).toBeTruthy();
  });
});
