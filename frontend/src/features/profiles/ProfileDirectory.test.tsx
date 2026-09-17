import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TenantProvider } from '../../app/TenantContext';
import { listProfiles } from './api';
import { ProfileDirectory } from './ProfileDirectory';

vi.mock('./api', () => ({ listProfiles: vi.fn() }));

const mockedListProfiles = vi.mocked(listProfiles);

type TenantContextsLoader = NonNullable<Parameters<typeof TenantProvider>[0]['loadTenantContexts']>;

function renderDirectory(loadTenantContexts: TenantContextsLoader) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <TenantProvider loadTenantContexts={loadTenantContexts}>
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
    renderDirectory(() => new Promise(() => undefined));

    expect(await screen.findByText('Select an authorized organization before loading profiles.')).toBeTruthy();
    expect(mockedListProfiles).not.toHaveBeenCalled();
  });

  it('starts the profile request only after exactly one authorized tenant resolves', async () => {
    mockedListProfiles.mockResolvedValue({ profiles: [], nextCursor: null });

    renderDirectory(async () => ({
      tenants: [
        {
          tenantId: 'tenant_01JAUTH',
          displayName: 'Authorized organization',
          actorId: 'actor_01JAUTH',
          role: 'MEMBER',
        },
      ],
    }));

    await waitFor(() => expect(mockedListProfiles).toHaveBeenCalledTimes(1));
    expect(mockedListProfiles.mock.calls[0]?.[0]).toBe('tenant_01JAUTH');
  });
});
