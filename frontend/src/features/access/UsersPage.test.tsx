import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TenantProvider } from '../../app/TenantContext';
import { listMembers } from './api';
import { UsersPage } from './route';

vi.mock('./api', async (loadOriginal) => ({
  ...await loadOriginal<typeof import('./api')>(),
  listMembers: vi.fn(),
}));

const mockedListMembers = vi.mocked(listMembers);

function renderUsers(role: 'OWNER' | 'MEMBER') {
  const query = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={query}>
      <TenantProvider loadTenantContexts={async () => ({
        tenants: [{
          tenantId: 'tenant_01JAUTH',
          displayName: 'Disposable team',
          actorId: 'actor_01JAUTH',
          role,
        }],
      })}>
        <UsersPage />
      </TenantProvider>
    </QueryClientProvider>,
  );
}

describe('Users scope after physical Windows audit', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    window.history.replaceState(null, '', '/users');
    mockedListMembers.mockResolvedValue({ members: [], nextCursor: null });
  });

  it('does not expose owner ceremonies and HMAC/version inputs to regular members', async () => {
    renderUsers('MEMBER');
    await waitFor(() => expect(mockedListMembers).toHaveBeenCalledTimes(1));
    expect(screen.getByText(/This view is read-only/)).toBeTruthy();
    expect(screen.queryByLabelText('Invited contact HMAC')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Transfer owner' })).toBeNull();
    expect(screen.queryByText('Advanced owner operations')).toBeNull();
  });

  it('labels raw owner-only commands as advanced, never ordinary invitation self-service', async () => {
    renderUsers('OWNER');
    await waitFor(() => expect(mockedListMembers).toHaveBeenCalledTimes(1));
    expect(screen.getByRole('heading', { name: 'Advanced owner operations' })).toBeTruthy();
    expect(screen.getByLabelText('Invited contact HMAC')).toBeTruthy();
    expect(screen.getByText(/not ordinary invite or membership self-service/i)).toBeTruthy();
  });
});
