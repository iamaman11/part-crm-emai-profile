import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
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

  it('hides terminal deleted profiles until explicitly requested', async () => {
    mockedListProfiles.mockResolvedValue({
      profiles: [
        {
          profileId: 'profile_active_test',
          status: 'READY',
          version: 2,
          linkedClientId: null,
          activeGenerationId: 'generation_active_test',
        },
        {
          profileId: 'profile_deleted_test',
          status: 'DELETED',
          version: 3,
          linkedClientId: null,
          activeGenerationId: null,
        },
      ],
      nextCursor: null,
    });

    renderDirectory(async () => ({
      tenants: [{
        tenantId: 'tenant_01JAUTH',
        displayName: 'Authorized organization',
        actorId: 'actor_01JAUTH',
        role: 'TENANT_OWNER',
      }],
    }));

    expect(await screen.findByText('profile_active_test')).toBeTruthy();
    expect(screen.queryByText('profile_deleted_test')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Show deleted profiles (1)' }));
    expect(screen.getByText('profile_deleted_test')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Hide deleted profiles' }));
    expect(screen.queryByText('profile_deleted_test')).toBeNull();
    // Presentation filtering must not issue a second tenant query.
    expect(mockedListProfiles).toHaveBeenCalledTimes(1);
  });

  it('does not offer another page when all loaded profiles are deleted and there is no next cursor', async () => {
    mockedListProfiles.mockResolvedValue({
      profiles: [{
        profileId: 'profile_only_deleted',
        status: 'DELETED',
        version: 3,
        linkedClientId: null,
        activeGenerationId: null,
      }],
      nextCursor: null,
    });

    renderDirectory(async () => ({
      tenants: [{
        tenantId: 'tenant_01JAUTH',
        displayName: 'Authorized organization',
        actorId: 'actor_01JAUTH',
        role: 'TENANT_OWNER',
      }],
    }));

    expect(await screen.findByText('No active profiles loaded. Show deleted profiles.')).toBeTruthy();
    expect(screen.queryByText(/or load more/)).toBeNull();
    expect(screen.queryByRole('button', { name: 'Show more profiles' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Show deleted profiles (1)' }));
    expect(screen.getByText('profile_only_deleted')).toBeTruthy();
  });

  it('loads an active profile beyond a deleted-only first page using the server cursor', async () => {
    mockedListProfiles.mockImplementation(async (_tenantId, _signal, cursor) => {
      if (cursor === 'next_test_page') {
        return {
          profiles: [{
            profileId: 'profile_page_two_ready',
            status: 'READY',
            version: 4,
            linkedClientId: null,
            activeGenerationId: 'generation_page_two',
          }],
          nextCursor: null,
        };
      }
      return {
        profiles: [{
          profileId: 'profile_page_one_deleted',
          status: 'DELETED',
          version: 3,
          linkedClientId: null,
          activeGenerationId: null,
        }],
        nextCursor: 'next_test_page',
      };
    });

    renderDirectory(async () => ({
      tenants: [{
        tenantId: 'tenant_01JAUTH',
        displayName: 'Authorized organization',
        actorId: 'actor_01JAUTH',
        role: 'MEMBER',
      }],
    }));

    expect(await screen.findByText('No active profiles loaded. Show deleted profiles or load more.')).toBeTruthy();
    expect(screen.queryByText('profile_page_one_deleted')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Show more profiles' }));
    expect(await screen.findByText('profile_page_two_ready')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Show more profiles' })).toBeNull();
    expect(mockedListProfiles).toHaveBeenCalledTimes(2);
    expect(mockedListProfiles.mock.calls[1]?.[2]).toBe('next_test_page');

    fireEvent.click(screen.getByRole('button', { name: 'Show deleted profiles (1)' }));
    expect(screen.getByText('profile_page_one_deleted')).toBeTruthy();
    expect(screen.getByText('profile_page_two_ready')).toBeTruthy();
  });

  it('keeps already loaded profiles visible after a later page fails', async () => {
    mockedListProfiles.mockImplementation(async (_tenantId, _signal, cursor) => {
      if (cursor === 'next_test_page') throw new Error('temporary page read failure');
      return {
        profiles: [{
          profileId: 'profile_page_one_ready',
          status: 'READY',
          version: 2,
          linkedClientId: null,
          activeGenerationId: 'generation_page_one',
        }],
        nextCursor: 'next_test_page',
      };
    });

    renderDirectory(async () => ({
      tenants: [{
        tenantId: 'tenant_01JAUTH',
        displayName: 'Authorized organization',
        actorId: 'actor_01JAUTH',
        role: 'MEMBER',
      }],
    }));

    expect(await screen.findByText('profile_page_one_ready')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Show more profiles' }));
    await waitFor(() => expect(mockedListProfiles).toHaveBeenCalledTimes(2));
    expect(screen.getByText('profile_page_one_ready')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Show more profiles' })).toBeTruthy();
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
