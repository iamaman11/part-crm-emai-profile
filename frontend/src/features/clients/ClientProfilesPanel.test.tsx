import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  assignProfile,
  detachProfile,
  getProfile,
} from '../profiles';
import { dispatchBrowserProfileOpen } from '../profiles/api';
import { ClientProfilesPanel } from './ClientProfilesPanel';
import { listClientProfiles } from './api';

vi.mock('../profiles', () => ({
  assignProfile: vi.fn(),
  detachProfile: vi.fn(),
  getProfile: vi.fn(),
}));
vi.mock('../profiles/api', () => ({ dispatchBrowserProfileOpen: vi.fn() }));

vi.mock('./api', () => ({
  listClientProfiles: vi.fn(),
}));

const mockedAssignProfile = vi.mocked(assignProfile);
const mockedDetachProfile = vi.mocked(detachProfile);
const mockedGetProfile = vi.mocked(getProfile);
const mockedListClientProfiles = vi.mocked(listClientProfiles);
const mockedOpen = vi.mocked(dispatchBrowserProfileOpen);

const tenantId = 'tenant_01JCLIENTPROFILES';
const clientId = 'client_01JCLIENTPROFILES';
const actorId = 'actor_01JCLIENTPROFILES';

function profileItem(profileId: string, version: number) {
  return {
    profileId,
    displayName: profileId,
    status: 'READY' as const,
    version,
    metadataVersion: version,
    linkedClientId: clientId,
    activeGenerationId: null,
    canOperate: true,
  };
}

function renderPanel(onMutated = vi.fn().mockResolvedValue(undefined), selectedActorId: string | null = actorId) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return {
    onMutated,
    ...render(
      <QueryClientProvider client={queryClient}>
        <ClientProfilesPanel tenantId={tenantId} actorId={selectedActorId} clientId={clientId} onMutated={onMutated} />
      </QueryClientProvider>,
    ),
  };
}

describe('ClientProfilesPanel relationship and native Open navigation', () => {
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

  it('offers direct native Open alongside profile details without a Worker launch', async () => {
    const profileId = 'profile_01JCLIENTPROFILES_LAUNCH';
    mockedListClientProfiles.mockResolvedValue({
      profiles: [profileItem(profileId, 4)],
      nextCursor: null,
    });
    renderPanel();
    expect(await screen.findByText(profileId)).toBeTruthy();
    const link = screen.getByRole('link', { name: `View profile ${profileId}` });
    expect(link.getAttribute('href')).toBe(
      `/profiles/${profileId}?tenant=${tenantId}`,
    );
    fireEvent.click(screen.getByRole('button', { name: `Open browser for ${profileId}` }));
    expect(mockedOpen).toHaveBeenCalledExactlyOnceWith(tenantId, actorId, profileId, 4, 'READY');
    expect(mockedGetProfile).not.toHaveBeenCalled();
  });

  it('opens an authorized DRAFT from a client profile list with its exact row version', async () => {
    const profileId = 'profile_01JCLIENTPROFILES_DRAFT';
    mockedListClientProfiles.mockResolvedValue({
      profiles: [{ ...profileItem(profileId, 3), status: 'DRAFT', displayName: 'New client browser' }],
      nextCursor: null,
    });
    renderPanel();

    fireEvent.click(await screen.findByRole('button', { name: 'Open browser for New client browser' }));
    expect(mockedOpen).toHaveBeenCalledExactlyOnceWith(tenantId, actorId, profileId, 3, 'DRAFT');
    expect(mockedListClientProfiles).toHaveBeenCalledTimes(1);
    expect(mockedGetProfile).not.toHaveBeenCalled();
    expect(screen.getByText(profileId)).toBeTruthy();
  });

  it('never treats client assignment as Open authority, and explains unavailable states visibly', async () => {
    const profileId = 'profile_01JCLIENTPROFILES_SUSPENDED';
    mockedListClientProfiles.mockResolvedValue({
      profiles: [
        { ...profileItem(profileId, 4), status: 'SUSPENDED' },
        { ...profileItem('profile_01JCLIENTPROFILES_VIEWER', 6), canOperate: false },
      ],
      nextCursor: null,
    });
    renderPanel();
    expect(await screen.findByText(profileId)).toBeTruthy();
    expect(screen.getByRole('link', { name: `View profile ${profileId}` })).toBeTruthy();
    const suspended = screen.getByRole('button', { name: `Open browser for ${profileId}` }) as HTMLButtonElement;
    const viewer = screen.getByRole('button', { name: 'Open browser for profile_01JCLIENTPROFILES_VIEWER' }) as HTMLButtonElement;
    expect(suspended.disabled).toBe(true);
    expect(viewer.disabled).toBe(true);
    expect(screen.getByText('Cannot open a SUSPENDED profile.')).toBeTruthy();
    expect(screen.getByText('Operator permission is required.')).toBeTruthy();
    expect(mockedOpen).not.toHaveBeenCalled();
  });

  it('refuses a row Open when the authorized tenant actor is unavailable', async () => {
    const profileId = 'profile_01JCLIENTPROFILES_NOACTOR';
    mockedListClientProfiles.mockResolvedValue({ profiles: [profileItem(profileId, 4)], nextCursor: null });
    renderPanel(undefined, null);

    const button = await screen.findByRole('button', { name: `Open browser for ${profileId}` }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    expect(screen.getByText('Organization access is unavailable.')).toBeTruthy();
    expect(mockedOpen).not.toHaveBeenCalled();
  });

  it('reports a native handoff failure safely without replaying Open', async () => {
    const profileId = 'profile_01JCLIENTPROFILES_HANDOFF';
    mockedListClientProfiles.mockResolvedValue({ profiles: [profileItem(profileId, 7)], nextCursor: null });
    mockedOpen.mockImplementationOnce(() => { throw new Error('secret device path'); });
    renderPanel();

    fireEvent.click(await screen.findByRole('button', { name: `Open browser for ${profileId}` }));
    expect(mockedOpen).toHaveBeenCalledTimes(1);
    expect((await screen.findByRole('alert')).textContent).toContain('Could not hand off');
    expect(screen.queryByText(/secret device path/)).toBeNull();
  });

  it('loads the profile through its own visibility boundary before attach or atomic reassign', async () => {
    mockedGetProfile.mockResolvedValue({
      profileId: 'profile_01JCLIENTPROFILES_ATTACH',
      displayName: 'profile_01JCLIENTPROFILES_ATTACH',
      status: 'READY',
      version: 7,
      metadataVersion: 7,
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
        expectedMetadataVersion: 7,
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
        expectedMetadataVersion: 4,
        reason: 'relationship no longer applies',
      },
      expect.any(String),
    );
    expect(onMutated).toHaveBeenCalledTimes(1);
  });
});
