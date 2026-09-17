import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TenantProvider } from '../../app/TenantContext';
import { getTenantContexts } from '../session/api';
import { listProfiles } from './api';
import { ProfileDirectory } from './ProfileDirectory';

vi.mock('../session/api', () => ({ getTenantContexts: vi.fn() }));
vi.mock('./api', () => ({ listProfiles: vi.fn() }));

const mockedGetTenantContexts = vi.mocked(getTenantContexts);
const mockedListProfiles = vi.mocked(listProfiles);

function renderDirectory() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <TenantProvider>
        <ProfileDirectory onSelect={() => undefined} />
      </TenantProvider>
    </QueryClientProvider>,
  );
}

describe('ProfileDirectory tenant bootstrap boundary', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    window.history.replaceState(null, '', '/profiles');
  });

  it('does not start a tenant-scoped profile request while tenant authorization is unresolved', async () => {
    mockedGetTenantContexts.mockImplementation(() => new Promise(() => undefined));

    renderDirectory();

    expect(await screen.findByText('Select an authorized organization before loading profiles.')).toBeTruthy();
    expect(mockedListProfiles).not.toHaveBeenCalled();
  });

  it('starts the profile request only after exactly one authorized tenant resolves', async () => {
    mockedGetTenantContexts.mockResolvedValue({
      tenants: [
        {
          tenantId: 'tenant_01JAUTH',
          displayName: 'Authorized organization',
          actorId: 'actor_01JAUTH',
          role: 'MEMBER',
        },
      ],
    });
    mockedListProfiles.mockResolvedValue({ profiles: [], nextCursor: null });

    renderDirectory();

    await waitFor(() => expect(mockedListProfiles).toHaveBeenCalledTimes(1));
    expect(mockedListProfiles.mock.calls[0]?.[0]).toBe('tenant_01JAUTH');
  });
});
