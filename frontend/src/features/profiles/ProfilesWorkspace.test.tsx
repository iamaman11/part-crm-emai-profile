import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TenantProvider } from '../../app/TenantContext';
import { changeGenerationActivation, getCoordinator, getGeneration, getProfile } from './api';
import { ProfilesWorkspace } from './ProfilesWorkspace';

vi.mock('./api', () => ({
  openBrowserProfile: vi.fn(),
  changeGenerationActivation: vi.fn(),
  commandCoordinator: vi.fn(),
  createProfile: vi.fn(),
  getCoordinator: vi.fn(),
  getGeneration: vi.fn(),
  getProfile: vi.fn(),
  quarantineGeneration: vi.fn(),
  registerGeneration: vi.fn(),
  setProfileGrant: vi.fn(),
  verifyGeneration: vi.fn(),
}));

const profileLookup = vi.mocked(getProfile);
const generationLookup = vi.mocked(getGeneration);
const coordinatorLookup = vi.mocked(getCoordinator);
const activation = vi.mocked(changeGenerationActivation);

describe('ProfilesWorkspace generation refresh (#802 F06)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    window.history.replaceState(null, '', '/profiles');
  });

  it('refreshes authoritative profile and generation after deactivate/reactivate without reload', async () => {
    const profile = { profileId: 'profile_test', status: 'READY', version: 11, linkedClientId: null, activeGenerationId: 'generation_test' as string | null };
    const generation = { generationId: 'generation_test', status: 'VERIFIED', version: 3, verificationReference: null };
    profileLookup.mockImplementation(async () => ({ ...profile }) as Awaited<ReturnType<typeof getProfile>>);
    generationLookup.mockImplementation(async () => ({ ...generation }) as Awaited<ReturnType<typeof getGeneration>>);
    activation.mockImplementation(async (_tenant, _profile, _generation, expectedVersion, activate) => {
      expect(expectedVersion).toBe(profile.version);
      profile.status = activate ? 'READY' : 'SUSPENDED';
      profile.activeGenerationId = activate ? 'generation_test' : null;
      profile.version += 1;
      return { resultCode: 'SUCCEEDED', resourceId: profile.profileId } as Awaited<ReturnType<typeof changeGenerationActivation>>;
    });

    const cache = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={cache}>
        <TenantProvider loadTenantContexts={async () => ({
          tenants: [{ tenantId: 'tenant_test', displayName: 'Test tenant', actorId: 'actor_test', role: 'TENANT_OWNER' }],
        })}>
          <ProfilesWorkspace selectedProfileId="profile_test" onProfileSelected={() => undefined} />
        </TenantProvider>
      </QueryClientProvider>,
    );

    const open = await screen.findByRole('button', { name: 'Open browser' }) as HTMLButtonElement;
    expect(open.disabled).toBe(false);
    // The Generation controls are intentionally inside a collapsed advanced
    // disclosure. Exercise the real operator path rather than querying hidden UI.
    fireEvent.click(screen.getByText('Administration and diagnostics'));
    fireEvent.change(screen.getAllByLabelText('Generation ID')[0]!, { target: { value: 'generation_test' } });
    fireEvent.click(screen.getByRole('button', { name: 'Lookup generation' }));
    await screen.findByRole('button', { name: 'deactivate generation' });

    fireEvent.click(screen.getByRole('button', { name: 'deactivate generation' }));
    fireEvent.click(screen.getByRole('button', { name: 'Confirm deactivate generation' }));
    await waitFor(() => expect(open.disabled).toBe(true));
    expect(profileLookup).toHaveBeenCalledTimes(2);
    expect(generationLookup).toHaveBeenCalledTimes(2);

    fireEvent.click(screen.getByRole('button', { name: 'activate generation' }));
    fireEvent.click(screen.getByRole('button', { name: 'Confirm activate generation' }));
    await waitFor(() => expect(open.disabled).toBe(false));
    expect(profileLookup).toHaveBeenCalledTimes(3);
    expect(generationLookup).toHaveBeenCalledTimes(3);
    expect(activation).toHaveBeenCalledTimes(2);

    // Mutation succeeds, but the immediately following projection read fails.
    // The server command must not be replayed and stale READY must not permit Open.
    generationLookup.mockRejectedValueOnce(new Error('readback temporarily unavailable'));
    fireEvent.click(screen.getByRole('button', { name: 'deactivate generation' }));
    fireEvent.click(screen.getByRole('button', { name: 'Confirm deactivate generation' }));
    expect(await screen.findByText(/action completed, but the latest profile status could not be confirmed/i)).toBeTruthy();
    expect(open.disabled).toBe(true);
    expect(activation).toHaveBeenCalledTimes(3);
    fireEvent.click(screen.getByRole('button', { name: 'Refresh current status' }));
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Refresh current status' })).toBeNull());
    expect(open.disabled).toBe(true); // Fresh SUSPENDED is still non-launchable.
    expect(activation).toHaveBeenCalledTimes(3); // Read-only recovery, no duplicate command.
    expect(generationLookup).toHaveBeenCalledTimes(5);
  });
  it('never applies an old generation mutation to a newly selected profile', async () => {
    let finishOld: (value: Awaited<ReturnType<typeof changeGenerationActivation>>) => void = () => {
      throw new Error('deferred command was not started');
    };
    const oldAction = new Promise<Awaited<ReturnType<typeof changeGenerationActivation>>>((resolve) => {
      finishOld = resolve;
    });
    profileLookup.mockImplementation(async (_tenant, id) => ({
      profileId: id, status: 'READY', version: 11, linkedClientId: null,
      activeGenerationId: id === 'profile_old' ? 'generation_old' : null,
    }) as Awaited<ReturnType<typeof getProfile>>);
    generationLookup.mockResolvedValue({
      generationId: 'generation_old', status: 'VERIFIED', version: 3,
      verificationReference: null,
    } as Awaited<ReturnType<typeof getGeneration>>);
    activation.mockImplementation(() => oldAction);
    const cache = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const provider = (id: string) => (
      <QueryClientProvider client={cache}>
        <TenantProvider loadTenantContexts={async () => ({
          tenants: [{ tenantId: 'tenant_test', displayName: 'Test tenant', actorId: 'actor_test', role: 'TENANT_OWNER' }],
        })}>
          <ProfilesWorkspace selectedProfileId={id} onProfileSelected={() => undefined} />
        </TenantProvider>
      </QueryClientProvider>
    );
    const view = render(provider('profile_old'));
    await screen.findByRole('button', { name: 'Open browser' });
    fireEvent.click(screen.getByText('Administration and diagnostics'));
    fireEvent.change(screen.getAllByLabelText('Generation ID')[0]!, { target: { value: 'generation_old' } });
    fireEvent.click(screen.getByRole('button', { name: 'Lookup generation' }));
    await screen.findByRole('button', { name: 'deactivate generation' });
    fireEvent.click(screen.getByRole('button', { name: 'deactivate generation' }));
    fireEvent.click(screen.getByRole('button', { name: 'Confirm deactivate generation' }));
    await waitFor(() => expect(activation).toHaveBeenCalledTimes(1));

    view.rerender(provider('profile_new'));
    await screen.findByText('profile_new');
    finishOld({ resultCode: 'SUCCEEDED', resourceId: 'profile_old' } as Awaited<ReturnType<typeof changeGenerationActivation>>);
    await waitFor(() => expect((screen.getByRole('button', { name: 'Open browser' }) as HTMLButtonElement).disabled).toBe(false));
    expect(screen.queryByText('profile_old')).toBeNull();
    expect(screen.queryByText('SUCCEEDED: profile_old')).toBeNull();
    expect(activation).toHaveBeenCalledTimes(1);
    expect(generationLookup).toHaveBeenCalledTimes(2); // Initial lookup + original readback, never new generation.
  });

  it('does not show a delayed profile lookup from a previous selection', async () => {
    let resolveOld: (value: Awaited<ReturnType<typeof getProfile>>) => void = () => undefined;
    const oldRead = new Promise<Awaited<ReturnType<typeof getProfile>>>((resolve) => {
      resolveOld = resolve;
    });
    profileLookup.mockImplementation(async (_tenant, id) => {
      if (id === 'profile_old') return oldRead;
      return {
        profileId: id, status: 'READY', version: 8, linkedClientId: null,
      } as Awaited<ReturnType<typeof getProfile>>;
    });
    const cache = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const provider = (id: string) => (
      <QueryClientProvider client={cache}>
        <TenantProvider loadTenantContexts={async () => ({
          tenants: [{ tenantId: 'tenant_test', displayName: 'Test tenant', actorId: 'actor_test', role: 'TENANT_OWNER' }],
        })}>
          <ProfilesWorkspace selectedProfileId={id} onProfileSelected={() => undefined} />
        </TenantProvider>
      </QueryClientProvider>
    );
    const view = render(provider('profile_old'));
    await waitFor(() => expect(profileLookup).toHaveBeenCalledWith('tenant_test', 'profile_old'));
    view.rerender(provider('profile_new'));
    await screen.findByText('profile_new');
    await act(async () => {
      resolveOld({
        profileId: 'profile_old', status: 'SUSPENDED', version: 18, linkedClientId: null,
      } as Awaited<ReturnType<typeof getProfile>>);
      await oldRead;
    });
    expect(screen.queryByText('profile_old')).toBeNull();
    expect(screen.getByText('profile_new')).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Open browser' }) as HTMLButtonElement).disabled).toBe(false);
  });

  it('does not render stale generation or coordinator after switching profiles', async () => {
    let resolveGeneration: (value: Awaited<ReturnType<typeof getGeneration>>) => void = () => undefined;
    let resolveCoordinator: (value: Awaited<ReturnType<typeof getCoordinator>>) => void = () => undefined;
    const oldGeneration = new Promise<Awaited<ReturnType<typeof getGeneration>>>((resolve) => {
      resolveGeneration = resolve;
    });
    const oldCoordinator = new Promise<Awaited<ReturnType<typeof getCoordinator>>>((resolve) => {
      resolveCoordinator = resolve;
    });
    profileLookup.mockImplementation(async (_tenant, id) => ({
      profileId: id, status: 'READY', version: 9, linkedClientId: null,
    }) as Awaited<ReturnType<typeof getProfile>>);
    generationLookup.mockReturnValue(oldGeneration);
    coordinatorLookup.mockReturnValue(oldCoordinator);
    const cache = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const provider = (id: string) => (
      <QueryClientProvider client={cache}>
        <TenantProvider loadTenantContexts={async () => ({
          tenants: [{ tenantId: 'tenant_test', displayName: 'Test tenant', actorId: 'actor_test', role: 'TENANT_OWNER' }],
        })}>
          <ProfilesWorkspace selectedProfileId={id} onProfileSelected={() => undefined} />
        </TenantProvider>
      </QueryClientProvider>
    );
    const view = render(provider('profile_old'));
    await screen.findByRole('button', { name: 'Open browser' });
    fireEvent.click(screen.getByText('Administration and diagnostics'));
    fireEvent.change(screen.getAllByLabelText('Generation ID')[0]!, { target: { value: 'generation_old' } });
    fireEvent.click(screen.getByRole('button', { name: 'Lookup generation' }));
    fireEvent.click(screen.getByRole('button', { name: 'Load coordinator snapshot' }));
    await waitFor(() => expect(generationLookup).toHaveBeenCalledWith('tenant_test', 'profile_old', 'generation_old'));
    await waitFor(() => expect(coordinatorLookup).toHaveBeenCalledWith('tenant_test', 'profile_old'));

    view.rerender(provider('profile_new'));
    await screen.findByText('profile_new');
    await act(async () => {
      resolveGeneration({
        generationId: 'generation_old', status: 'VERIFIED', version: 3, verificationReference: null,
      } as Awaited<ReturnType<typeof getGeneration>>);
      resolveCoordinator({
        version: 7, sequence: 6, projection: { status: 'Active' },
      } as unknown as Awaited<ReturnType<typeof getCoordinator>>);
      await Promise.all([oldGeneration, oldCoordinator]);
    });
    expect(screen.queryByRole('button', { name: 'deactivate generation' })).toBeNull();
    expect(screen.queryByText('Active session')).toBeNull();
    expect(screen.getByText('profile_new')).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Open browser' }) as HTMLButtonElement).disabled).toBe(false);
  });

});
