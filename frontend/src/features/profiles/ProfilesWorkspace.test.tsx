import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TenantProvider } from '../../app/TenantContext';
import { changeGenerationActivation, getGeneration, getProfile } from './api';
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
const activation = vi.mocked(changeGenerationActivation);

describe('ProfilesWorkspace generation refresh (#802 F06)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    window.history.replaceState(null, '', '/profiles');
  });

  it('refreshes authoritative profile and generation after deactivate/reactivate without reload', async () => {
    const profile = { profileId: 'profile_test', status: 'READY', version: 11, linkedClientId: null };
    const generation = { generationId: 'generation_test', status: 'VERIFIED', version: 3, verificationReference: null };
    profileLookup.mockImplementation(async () => ({ ...profile }) as Awaited<ReturnType<typeof getProfile>>);
    generationLookup.mockImplementation(async () => ({ ...generation }) as Awaited<ReturnType<typeof getGeneration>>);
    activation.mockImplementation(async (_tenant, _profile, _generation, expectedVersion, activate) => {
      expect(expectedVersion).toBe(profile.version);
      profile.status = activate ? 'READY' : 'SUSPENDED';
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
});
