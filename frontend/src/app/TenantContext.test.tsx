import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it } from 'vitest';
import { TenantChooser, TenantProvider, useTenant } from './TenantContext';

function Probe() {
  const { tenantId } = useTenant();
  return <output>{tenantId || 'none'}</output>;
}

type TenantContextsLoader = NonNullable<Parameters<typeof TenantProvider>[0]['loadTenantContexts']>;

function renderContext(loadTenantContexts: TenantContextsLoader) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <TenantProvider loadTenantContexts={loadTenantContexts}>
        <TenantChooser />
        <Probe />
      </TenantProvider>
    </QueryClientProvider>,
  );
}

describe('TenantContext', () => {
  beforeEach(() => {
    window.history.replaceState(null, '', '/');
  });

  it('shows an explicit no-organization state for zero ACTIVE contexts', async () => {
    renderContext(async () => ({ tenants: [] }));
    expect(await screen.findByText('No organization access is active for this identity.')).toBeTruthy();
    expect(screen.getByText('none')).toBeTruthy();
  });

  it('auto-selects exactly one authorized context', async () => {
    renderContext(async () => ({ tenants: [{ tenantId: 'tenant_01JONE', displayName: 'One organization', actorId: 'actor_01JONE', role: 'MEMBER' }] }));
    expect(await screen.findByText('Organization: One organization')).toBeTruthy();
    expect(screen.getByText('tenant_01JONE')).toBeTruthy();
  });

  it('rejects a stale URL tenant and requires a human-readable choice for multiple contexts', async () => {
    const user = userEvent.setup();
    window.history.replaceState(null, '', '/?tenant=tenant_stale');
    renderContext(async () => ({ tenants: [
      { tenantId: 'tenant_01JA', displayName: 'Alpha organization', actorId: 'actor_01JA', role: 'TENANT_OWNER' },
      { tenantId: 'tenant_01JB', displayName: 'Beta organization', actorId: 'actor_01JB', role: 'MEMBER' },
    ] }));
    const selector = await screen.findByLabelText('Organization');
    expect(screen.getByText('none')).toBeTruthy();
    expect(window.location.search).toBe('');
    await user.selectOptions(selector, 'tenant_01JB');
    expect(await screen.findByText('tenant_01JB')).toBeTruthy();
  });

  it('renders a bounded retry state when bootstrap fails', async () => {
    renderContext(async () => { throw new Error('bootstrap unavailable'); });
    expect((await screen.findByRole('alert')).textContent).toContain('Unable to resolve your organizations.');
    expect(screen.getByRole('button', { name: 'Retry' })).toBeTruthy();
  });
});
