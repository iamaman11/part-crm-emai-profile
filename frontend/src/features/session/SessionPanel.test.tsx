import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TenantProvider } from '../../app/TenantContext';
import { ApiProblem } from '../../shared/api/client';
import { getSession } from './api';
import { SessionPanel } from './SessionPanel';

vi.mock('./api', () => ({
  getSession: vi.fn(),
}));

const mockedGetSession = vi.mocked(getSession);

function renderPanel() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <TenantProvider>
        <SessionPanel />
      </TenantProvider>
    </QueryClientProvider>,
  );
}

describe('SessionPanel', () => {
  beforeEach(() => {
    window.history.replaceState(null, '', '/?tenant=tenant_01JTEST');
  });

  it('renders the authenticated actor projection returned by the Worker', async () => {
    mockedGetSession.mockResolvedValue({
      tenantId: 'tenant_01JTEST',
      actorId: 'actor_01JTEST',
      role: 'TENANT_OWNER',
    });

    renderPanel();

    expect(await screen.findByText('actor_01JTEST')).toBeTruthy();
    expect(screen.getByText('TENANT_OWNER')).toBeTruthy();
    expect(mockedGetSession).toHaveBeenCalledTimes(1);
  });

  it('preserves neutral disclosure on unresolved authenticated session', async () => {
    mockedGetSession.mockRejectedValue(new ApiProblem({
      type: 'urn:part-crm:problem:not-found',
      title: 'Not Found',
      status: 404,
      code: 'not_found',
      correlation_id: 'corr_session',
    }));

    renderPanel();

    expect(await screen.findByText('Resource unavailable')).toBeTruthy();
    expect(screen.queryByText('Not Found')).toBeNull();
  });
});
