import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  assignProfile,
  detachProfile,
  getProfile,
  dispatchBrowserProfileOpen,
  launchProfile,
} from '../profiles';
import { ClientProfilesPanel } from './ClientProfilesPanel';
import { listClientProfiles } from './api';

vi.mock('../profiles', () => ({
  assignProfile: vi.fn(),
  detachProfile: vi.fn(),
  getProfile: vi.fn(),
  dispatchBrowserProfileOpen: vi.fn(),
  launchProfile: vi.fn(),
}));

vi.mock('./api', () => ({
  listClientProfiles: vi.fn(),
}));

const mockedAssignProfile = vi.mocked(assignProfile);
const mockedDetachProfile = vi.mocked(detachProfile);
const mockedGetProfile = vi.mocked(getProfile);
const mockedLaunchProfile = vi.mocked(launchProfile);
const mockedDispatchBrowserProfileOpen = vi.mocked(dispatchBrowserProfileOpen);
const mockedListClientProfiles = vi.mocked(listClientProfiles);

const tenantId = 'tenant_01JCLIENTPROFILES';
const clientId = 'client_01JCLIENTPROFILES';

function profileItem(profileId: string, version: number) {
  return {
    profileId,
    status: 'READY' as const,
    version,
    linkedClientId: clientId,
    activeGenerationId: null,
  };
}

function renderPanel(onMutated = vi.fn().mockResolvedValue(undefined)) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return {
    onMutated,
    ...render(
      <QueryClientProvider client={queryClient}>
        <ClientProfilesPanel tenantId={tenantId} clientId={clientId} onMutated={onMutated} />
      </QueryClientProvider>,
    ),
  };
}

describe('ClientProfilesPanel P1 relationship and P2 launch workflow', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockedListClientProfiles.mockResolvedValue({ profiles: [], nextCursor: null });
    mockedAssignProfile.mockResolvedValue({
      resultCode: 'assigned',
      resourceId: 'assignment_01JCLIENTPROFILES',
      aggregateVersion: 8,
    });
    mockedDetachProfile.mockResolvedValue({
      resultCode: 'detached',
      resourceId: 'assignment_01JCLIENTPROFILES',
      aggregateVersion: 5,
    });
    mockedLaunchProfile.mockResolvedValue({
      launchUri: 'profilebridge://open/v2/tenant_01JCLIENTPROFILES/actor_01JCLIENTPROFILES/profile_01JCLIENTPROFILES_LAUNCH/4/READY',
      schemaVersion: 2,
    });
  });

  it('renders the authorized inverse projection and paginates through the same generated query path', async () => {
    mockedListClientProfiles.mockImplementation(async (_tenant, _client, _signal, cursor) => {
      if (cursor === 'profiles:profile_01JCLIENTPROFILES_A') {
        return {
          profiles: [profileItem('profile_01JCLIENTPROFILES_B', 4)],
          nextCursor: null,
        };
      }
      return {
        profiles: [profileItem('profile_01JCLIENTPROFILES_A', 3)],
        nextCursor: 'profiles:profile_01JCLIENTPROFILES_A',
      };
    });

    const user = userEvent.setup();
    renderPanel();

    expect(await screen.findByText('profile_01JCLIENTPROFILES_A')).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Load more profiles' }));
    expect(await screen.findByText('profile_01JCLIENTPROFILES_B')).toBeTruthy();

    await waitFor(() => expect(mockedListClientProfiles).toHaveBeenCalledTimes(2));
    expect(mockedListClientProfiles.mock.calls[0]?.[0]).toBe(tenantId);
    expect(mockedListClientProfiles.mock.calls[0]?.[1]).toBe(clientId);
    expect(mockedListClientProfiles.mock.calls[1]?.[3]).toBe('profiles:profile_01JCLIENTPROFILES_A');
  });

  it('launches an attached profile without client-selected device or generation and immediately hands off the bounded URI', async () => {
    const profileId = 'profile_01JCLIENTPROFILES_LAUNCH';
    mockedListClientProfiles.mockResolvedValue({
      profiles: [profileItem(profileId, 4)],
      nextCursor: null,
    });

    const user = userEvent.setup();
    renderPanel();

    expect(await screen.findByText(profileId)).toBeTruthy();
    const launchButton = screen.getByRole('button', { name: `Launch profile ${profileId}` });
    await user.hover(launchButton);
    await waitFor(() => expect(mockedLaunchProfile).toHaveBeenCalledTimes(1));
    await user.click(launchButton);

    await waitFor(() => expect(mockedDispatchBrowserProfileOpen).toHaveBeenCalledTimes(1));
    const [actualTenantId, actualProfileId, idempotencyKey] = mockedLaunchProfile.mock.calls[0] ?? [];
    expect(actualTenantId).toBe(tenantId);
    expect(actualProfileId).toBe(profileId);
    expect(typeof idempotencyKey).toBe('string');
    expect(idempotencyKey).not.toContain('device');
    expect(idempotencyKey).not.toContain('generation');
    expect(mockedDispatchBrowserProfileOpen).toHaveBeenCalledWith(
      'profilebridge://open/v2/tenant_01JCLIENTPROFILES/actor_01JCLIENTPROFILES/profile_01JCLIENTPROFILES_LAUNCH/4/READY',
    );
    expect(screen.queryByText(/claim_0123456789abcdef/u)).toBeNull();
  });

  it('completes a cold first click without a hover or a second click', async () => {
    const profileId = 'profile_01JCLIENTPROFILES_LAUNCH';
    mockedListClientProfiles.mockResolvedValue({
      profiles: [profileItem(profileId, 4)],
      nextCursor: null,
    });
    renderPanel();
    expect(await screen.findByText(profileId)).toBeTruthy();

    // fireEvent.click deliberately does not prefetch through hover/focus.
    fireEvent.click(screen.getByRole('button', { name: `Launch profile ${profileId}` }));
    await waitFor(() => expect(mockedDispatchBrowserProfileOpen).toHaveBeenCalledTimes(1));
    expect(mockedLaunchProfile).toHaveBeenCalledTimes(1);
  });

  it('surfaces a neutral launch failure without exposing bearer material', async () => {
    const profileId = 'profile_01JCLIENTPROFILES_LAUNCH_FAIL';
    mockedListClientProfiles.mockResolvedValue({
      profiles: [profileItem(profileId, 4)],
      nextCursor: null,
    });
    mockedLaunchProfile.mockRejectedValue(new Error('profilebridge://claim/secret-claim-must-not-render'));

    const user = userEvent.setup();
    renderPanel();

    expect(await screen.findByText(profileId)).toBeTruthy();
    const launchButton = screen.getByRole('button', { name: `Launch profile ${profileId}` });
    await user.hover(launchButton);
    await user.click(launchButton);
    expect(await screen.findByText('Profile launch failed. Retry from this profile.')).toBeTruthy();
    expect(screen.queryByText(/secret-claim-must-not-render/u)).toBeNull();
    expect(mockedDispatchBrowserProfileOpen).not.toHaveBeenCalled();
  });

  it('loads the profile through its own visibility boundary before attach or atomic reassign', async () => {
    mockedGetProfile.mockResolvedValue({
      profileId: 'profile_01JCLIENTPROFILES_ATTACH',
      status: 'READY',
      version: 7,
      linkedClientId: 'client_01JOTHER',
    });

    const user = userEvent.setup();
    const { onMutated } = renderPanel();

    await screen.findByText('No independently visible profiles are attached to this client.');
    await user.type(screen.getByLabelText('Profile ID'), 'profile_01JCLIENTPROFILES_ATTACH');
    await user.type(screen.getByLabelText('Reason'), 'move to selected client');
    await user.click(screen.getByRole('button', { name: 'Attach / reassign' }));

    await waitFor(() => expect(mockedAssignProfile).toHaveBeenCalledTimes(1));
    expect(mockedGetProfile).toHaveBeenCalledWith(tenantId, 'profile_01JCLIENTPROFILES_ATTACH');
    expect(mockedAssignProfile).toHaveBeenCalledWith(
      tenantId,
      'profile_01JCLIENTPROFILES_ATTACH',
      {
        clientId,
        reason: 'move to selected client',
        expectedProfileVersion: 7,
      },
      expect.any(String),
    );
    expect(onMutated).toHaveBeenCalledTimes(1);
  });

  it('detaches only after explicit confirmation and sends no client-controlled relationship identity', async () => {
    mockedListClientProfiles.mockResolvedValue({
      profiles: [profileItem('profile_01JCLIENTPROFILES_DETACH', 4)],
      nextCursor: null,
    });

    const user = userEvent.setup();
    const { onMutated } = renderPanel();

    expect(await screen.findByText('profile_01JCLIENTPROFILES_DETACH')).toBeTruthy();
    await user.type(screen.getByLabelText('Detach reason'), 'relationship no longer applies');
    await user.click(screen.getByRole('button', { name: 'detach profile' }));

    expect(mockedDetachProfile).not.toHaveBeenCalled();
    expect(screen.getByText(/closes only the active Client\/Profile relationship/u)).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Confirm detach profile' }));

    await waitFor(() => expect(mockedDetachProfile).toHaveBeenCalledTimes(1));
    expect(mockedDetachProfile).toHaveBeenCalledWith(
      tenantId,
      'profile_01JCLIENTPROFILES_DETACH',
      {
        expectedProfileVersion: 4,
        reason: 'relationship no longer applies',
      },
      expect.any(String),
    );
    expect(onMutated).toHaveBeenCalledTimes(1);
  });
});
