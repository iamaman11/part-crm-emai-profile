import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  assignProfile,
  createProfile,
  detachProfile,
  getProfile,
  listProfiles,
  dispatchBrowserProfileOpen,
} from '../profiles';
import { ClientProfilesPanel } from './ClientProfilesPanel';
import { listClientProfiles } from './api';
import { invokeProfileBridgeDiagnostics } from '../../shared/bridgeDiagnostics';

vi.mock('../profiles', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../profiles')>()),
  assignProfile: vi.fn(),
  createProfile: vi.fn(),
  detachProfile: vi.fn(),
  getProfile: vi.fn(),
  listProfiles: vi.fn(),
  dispatchBrowserProfileOpen: vi.fn(),
}));

vi.mock('./api', () => ({
  listClientProfiles: vi.fn(),
}));
vi.mock('../../shared/bridgeDiagnostics', () => ({ invokeProfileBridgeDiagnostics: vi.fn() }));

const mockedAssignProfile = vi.mocked(assignProfile);
const mockedCreateProfile = vi.mocked(createProfile);
const mockedDetachProfile = vi.mocked(detachProfile);
const mockedGetProfile = vi.mocked(getProfile);
const mockedListProfiles = vi.mocked(listProfiles);
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
    mockedListProfiles.mockResolvedValue({ profiles: [], nextCursor: null });
    mockedCreateProfile.mockResolvedValue({
      resultCode: 'created',
      resourceId: 'profile_01JCLIENTPROFILES_CREATED',
      aggregateVersion: 1,
    });
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
    expect(screen.getByText('1 client profile loaded · more available below.')).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Load more profiles' }));
    expect(await screen.findByText('profile_01JCLIENTPROFILES_B')).toBeTruthy();

    await waitFor(() => expect(mockedListClientProfiles).toHaveBeenCalledTimes(2));
    expect(mockedListClientProfiles.mock.calls[0]?.[0]).toBe(tenantId);
    expect(mockedListClientProfiles.mock.calls[0]?.[1]).toBe(clientId);
    expect(mockedListClientProfiles.mock.calls[1]?.[3]).toBe('profiles:profile_01JCLIENTPROFILES_A');
  });

  it('lets the client operator choose a visible existing profile by name instead of typing its ID', async () => {
    mockedListProfiles.mockResolvedValue({
      profiles: [
        { ...profileItem('profile_01JCLIENTPROFILES_PICK', 5), displayName: 'Sales mailbox', linkedClientId: null },
        { ...profileItem('profile_01JCLIENTPROFILES_HIDDEN', 6), displayName: 'Already linked', linkedClientId: clientId },
      ],
      nextCursor: null,
    });
    mockedGetProfile.mockResolvedValue({ ...profileItem('profile_01JCLIENTPROFILES_PICK', 5), linkedClientId: null });
    const user = userEvent.setup();
    renderPanel();

    const picker = await screen.findByRole('combobox', { name: 'Available profiles' });
    await waitFor(() => expect(picker.textContent).toContain('Sales mailbox'));
    expect(picker.textContent).not.toContain('Already linked');
    await user.selectOptions(picker, 'profile_01JCLIENTPROFILES_PICK');
    await user.type(screen.getByLabelText('Reason for association'), 'used by this client');
    await user.click(screen.getByRole('button', { name: 'Attach / reassign' }));

    await waitFor(() => expect(mockedAssignProfile).toHaveBeenCalledWith(
      tenantId,
      'profile_01JCLIENTPROFILES_PICK',
      { clientId, reason: 'used by this client', expectedMetadataVersion: 5 },
      expect.any(String),
    ));
  });

  it('paginates the accessible profile picker instead of limiting selection to the first page', async () => {
    mockedListProfiles.mockImplementation(async (_tenant, _signal, cursor) => cursor === 'next-available'
      ? { profiles: [{ ...profileItem('profile_01JCLIENTPROFILES_NEXT', 2), linkedClientId: null, displayName: 'Second page mailbox' }], nextCursor: null }
      : { profiles: [{ ...profileItem('profile_01JCLIENTPROFILES_FIRST', 1), linkedClientId: null }], nextCursor: 'next-available' });
    const user = userEvent.setup();
    renderPanel();

    const picker = await screen.findByRole('combobox', { name: 'Available profiles' });
    await waitFor(() => expect(picker.textContent).toContain('profile_01JCLIENTPROFILES_FIRST'));
    await user.click(screen.getByRole('button', { name: 'Load more available profiles' }));
    await waitFor(() => expect(picker.textContent).toContain('Second page mailbox'));
    expect(mockedListProfiles.mock.calls[1]?.[2]).toBe('next-available');
  });

  it('creates a browser profile from the client card and only reports success after a linked readback', async () => {
    let readCount = 0;
    mockedGetProfile.mockImplementation(async (_tenant, id) => ({
      ...profileItem(id, 1),
      displayName: 'Client sales profile',
      linkedClientId: ++readCount === 1 ? null : clientId,
    }));
    const user = userEvent.setup();
    const { onMutated } = renderPanel();

    await user.type(screen.getByLabelText('New profile name'), 'Client sales profile');
    await user.click(screen.getByRole('button', { name: 'Create and attach profile' }));

    expect((await screen.findByRole('status')).textContent).toContain('Profile created and linked');
    expect(mockedCreateProfile).toHaveBeenCalledTimes(1);
    const created = mockedCreateProfile.mock.calls[0]!;
    expect(created[0]).toBe(tenantId);
    expect(created[1]).toMatch(/^profile_[0-9a-f]{32}$/);
    expect(created[2]).toBe('Client sales profile');
    expect(mockedAssignProfile).toHaveBeenCalledExactlyOnceWith(
      tenantId,
      created[1],
      { clientId, reason: 'Created from the client card', expectedMetadataVersion: 1 },
      expect.any(String),
    );
    expect(mockedAssignProfile.mock.calls[0]![3]).not.toBe(created[3]);
    expect(mockedGetProfile).toHaveBeenCalledTimes(2);
    await waitFor(() => expect(onMutated).toHaveBeenCalledTimes(1));
  });

  it('retains the generated ID and creation idempotency key if the association ACK is lost', async () => {
    let readCount = 0;
    mockedGetProfile.mockImplementation(async (_tenant, id) => ({
      ...profileItem(id, 1),
      linkedClientId: ++readCount === 1 ? null : clientId,
    }));
    mockedAssignProfile.mockRejectedValueOnce(new Error('private transport details'));
    const user = userEvent.setup();
    renderPanel();

    await user.type(screen.getByLabelText('New profile name'), 'Recoverable profile');
    await user.click(screen.getByRole('button', { name: 'Create and attach profile' }));

    const warning = await screen.findByRole('alert');
    expect(warning.textContent).toContain('Creating another profile is paused to avoid duplicates');
    expect(warning.textContent).not.toContain('private transport details');
    expect((screen.getByRole('button', { name: 'Create and attach profile' }) as HTMLButtonElement).disabled).toBe(true);
    const originalId = mockedCreateProfile.mock.calls[0]![1];
    expect(warning.textContent).toContain(originalId);
    await user.click(screen.getByRole('button', { name: 'Retry creation / attachment' }));

    expect((await screen.findByRole('status')).textContent).toContain('Profile created and linked');
    expect(mockedCreateProfile).toHaveBeenCalledTimes(2);
    expect(mockedCreateProfile.mock.calls[1]![1]).toBe(originalId);
    expect(mockedCreateProfile.mock.calls[1]![3]).toBe(mockedCreateProfile.mock.calls[0]![3]);
    expect(mockedAssignProfile).toHaveBeenCalledTimes(1);
    expect(mockedGetProfile).toHaveBeenCalledTimes(3);
  });

  it('does not claim a linked profile when the authoritative association readback is missing', async () => {
    mockedGetProfile.mockImplementation(async (_tenant, id) => ({ ...profileItem(id, 1), linkedClientId: null }));
    const user = userEvent.setup();
    renderPanel();

    await user.type(screen.getByLabelText('New profile name'), 'Unconfirmed association');
    await user.click(screen.getByRole('button', { name: 'Create and attach profile' }));

    const warning = await screen.findByRole('alert');
    expect(warning.textContent).toContain('Profile creation or attachment is not confirmed');
    expect(screen.queryByText(/Profile created and linked/)).toBeNull();
    expect(mockedCreateProfile).toHaveBeenCalledTimes(1);
    expect(mockedAssignProfile).toHaveBeenCalledTimes(1);
    expect(mockedGetProfile).toHaveBeenCalledTimes(2);
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
    expect(screen.getByText(/This page cannot confirm BrowserReady or cloud Saved/)).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Copy diagnostics' }));
    expect(invokeProfileBridgeDiagnostics).toHaveBeenCalledTimes(1);
    expect(mockedOpen).toHaveBeenCalledTimes(1);
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

  it('explicitly refreshes a native-saved client profile from DRAFT v1 to READY v3 before the next Open', async () => {
    const profileId = 'profile_01JCLIENTPROFILES_REFRESH';
    mockedListClientProfiles
      .mockResolvedValueOnce({
        profiles: [{ ...profileItem(profileId, 1), status: 'DRAFT' }],
        nextCursor: null,
      })
      .mockResolvedValue({
        profiles: [profileItem(profileId, 3)],
        nextCursor: null,
      });
    const user = userEvent.setup();
    renderPanel();

    expect(await screen.findByText('DRAFT · browser v1 · metadata v1')).toBeTruthy();
    expect(screen.getByText(/A browser launch alone does not confirm Saved/)).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Refresh profiles' }));

    expect(await screen.findByText('READY · browser v3 · metadata v3')).toBeTruthy();
    expect(mockedListClientProfiles).toHaveBeenCalledTimes(2);
    expect(mockedListProfiles).toHaveBeenCalledTimes(2);
    expect(mockedOpen).not.toHaveBeenCalled();
    expect(mockedGetProfile).not.toHaveBeenCalled();
    expect(mockedAssignProfile).not.toHaveBeenCalled();
    expect(mockedDetachProfile).not.toHaveBeenCalled();

    await user.click(screen.getByRole('button', { name: `Open browser for ${profileId}` }));
    expect(mockedOpen).toHaveBeenCalledExactlyOnceWith(tenantId, actorId, profileId, 3, 'READY');
  });

  it('does not fabricate READY or dispatch Open when a manual refresh fails', async () => {
    const profileId = 'profile_01JCLIENTPROFILES_REFRESHFAIL';
    mockedListClientProfiles
      .mockResolvedValueOnce({
        profiles: [{ ...profileItem(profileId, 1), status: 'DRAFT' }],
        nextCursor: null,
      })
      .mockRejectedValueOnce(new Error('Could not load current profile status'));
    const user = userEvent.setup();
    renderPanel();

    expect(await screen.findByText('DRAFT · browser v1 · metadata v1')).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Refresh profiles' }));
    expect((await screen.findByRole('alert')).textContent).toContain('Could not load current profile status');
    expect(screen.getByText('DRAFT · browser v1 · metadata v1')).toBeTruthy();
    expect(screen.queryByText(/READY · browser v3/)).toBeNull();
    expect(mockedOpen).not.toHaveBeenCalled();
    expect(mockedAssignProfile).not.toHaveBeenCalled();
    expect(mockedDetachProfile).not.toHaveBeenCalled();
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
    expect(screen.getAllByRole('button', { name: 'Copy diagnostics' })).toHaveLength(2);
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

    await screen.findByText(/No profiles attached to this client are visible to your account/);
    await user.click(screen.getByText('Advanced: enter a profile ID'));
    await user.type(screen.getByLabelText('Profile ID'), 'profile_01JCLIENTPROFILES_ATTACH');
    await user.type(screen.getByLabelText('Reason for association'), 'move to selected client');
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
