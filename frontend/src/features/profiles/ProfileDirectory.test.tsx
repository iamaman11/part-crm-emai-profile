import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TenantProvider } from '../../app/TenantContext';
import { dispatchBrowserProfileOpen, listProfiles } from './api';
import { invokeProfileBridgeDiagnostics } from '../../shared/bridgeDiagnostics';
import { ProfileDirectory } from './ProfileDirectory';

vi.mock('./api', () => ({ listProfiles: vi.fn(), dispatchBrowserProfileOpen: vi.fn() }));
vi.mock('../../shared/bridgeDiagnostics', () => ({ invokeProfileBridgeDiagnostics: vi.fn() }));

const mockedListProfiles = vi.mocked(listProfiles);
const mockedOpen = vi.mocked(dispatchBrowserProfileOpen);

type TenantContextsLoader = NonNullable<Parameters<typeof TenantProvider>[0]['loadTenantContexts']>;

function renderDirectory(loadTenantContexts: TenantContextsLoader, onSelect = vi.fn()) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <TenantProvider loadTenantContexts={loadTenantContexts}>
        <ProfileDirectory onSelect={onSelect} />
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
          displayName: 'Test profile_active_test',
          status: 'READY',
          version: 2, metadataVersion: 2,
          linkedClientId: null,
          activeGenerationId: 'generation_active_test',
          canOperate: true,
        },
        {
          profileId: 'profile_deleted_test',
          displayName: 'Test profile_deleted_test',
          status: 'DELETED',
          version: 3, metadataVersion: 3,
          linkedClientId: null,
          activeGenerationId: null,
          canOperate: false,
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
          displayName: 'Test profile_only_deleted',
        status: 'DELETED',
        version: 3, metadataVersion: 3,
        linkedClientId: null,
        activeGenerationId: null,
        canOperate: false,
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
          displayName: 'Test profile_page_two_ready',
            status: 'READY',
            version: 4, metadataVersion: 4,
            linkedClientId: null,
            activeGenerationId: 'generation_page_two',
            canOperate: true,
          }],
          nextCursor: null,
        };
      }
      return {
        profiles: [{
          profileId: 'profile_page_one_deleted',
          displayName: 'Test profile_page_one_deleted',
          status: 'DELETED',
          version: 3, metadataVersion: 3,
          linkedClientId: null,
          activeGenerationId: null,
          canOperate: false,
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
          displayName: 'Test profile_page_one_ready',
          status: 'READY',
          version: 2, metadataVersion: 2,
          linkedClientId: null,
          activeGenerationId: 'generation_page_one',
          canOperate: true,
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

  it('opens DRAFT and READY profiles directly from their list rows with the existing native URI owner', async () => {
    mockedListProfiles.mockResolvedValue({
      profiles: [
        { profileId: 'profile_draft_click', displayName: 'Draft browser', status: 'DRAFT', version: 4,
          metadataVersion: 2, linkedClientId: null, activeGenerationId: null, canOperate: true },
        { profileId: 'profile_ready_click', displayName: 'Ready browser', status: 'READY', version: 7,
          metadataVersion: 3, linkedClientId: null, activeGenerationId: 'generation_ready', canOperate: true },
      ],
      nextCursor: null,
    });
    const onSelect = vi.fn();
    renderDirectory(async () => ({ tenants: [{
      tenantId: 'tenant_01JAUTH', displayName: 'Organization', actorId: 'actor_01JAUTH', role: 'MEMBER',
    }] }), onSelect);

    fireEvent.click(await screen.findByRole('button', { name: 'Open browser for Draft browser' }));
    expect(mockedOpen).toHaveBeenNthCalledWith(1, 'tenant_01JAUTH', 'actor_01JAUTH', 'profile_draft_click', 4, 'DRAFT');
    fireEvent.click(screen.getByRole('button', { name: 'Open browser for Ready browser' }));
    expect(mockedOpen).toHaveBeenNthCalledWith(2, 'tenant_01JAUTH', 'actor_01JAUTH', 'profile_ready_click', 7, 'READY');
    expect(screen.getAllByText(/This page cannot confirm BrowserReady or cloud Saved/)).toHaveLength(2);
    fireEvent.click(screen.getByLabelText('More actions for Draft browser'));
    fireEvent.click(screen.getAllByRole('button', { name: 'Copy diagnostics' })[0]!);
    expect(invokeProfileBridgeDiagnostics).toHaveBeenCalledTimes(1);
    expect(mockedOpen).toHaveBeenCalledTimes(2); // diagnostics never replays Open
    expect(mockedListProfiles).toHaveBeenCalledTimes(1);
    expect(onSelect).not.toHaveBeenCalled();
    fireEvent.click(screen.getAllByRole('button', { name: 'View details' })[0]!);
    expect(onSelect).toHaveBeenCalledWith('profile_draft_click');
  });

  it('searches loaded rows and filters status without issuing new tenant requests or Open', async () => {
    mockedListProfiles.mockResolvedValue({
      profiles: [
        { profileId: 'profile_sales', displayName: 'Sales browser', status: 'READY', version: 2,
          metadataVersion: 2, linkedClientId: 'client_sales', activeGenerationId: null, canOperate: true },
        { profileId: 'profile_review', displayName: 'Review browser', status: 'DIRTY_LOCAL', version: 5,
          metadataVersion: 3, linkedClientId: null, activeGenerationId: null, canOperate: true },
      ],
      nextCursor: null,
    });
    renderDirectory(async () => ({ tenants: [{
      tenantId: 'tenant_01JAUTH', displayName: 'Organization', actorId: 'actor_01JAUTH', role: 'MEMBER',
    }] }));

    expect(await screen.findByText('Sales browser')).toBeTruthy();
    expect(screen.getByText('Review browser')).toBeTruthy();
    fireEvent.change(screen.getByRole('searchbox', { name: 'Search loaded profiles' }), { target: { value: 'client_sales' } });
    expect(screen.getByText('Sales browser')).toBeTruthy();
    expect(screen.queryByText('Review browser')).toBeNull();
    fireEvent.change(screen.getByRole('searchbox'), { target: { value: '' } });
    fireEvent.change(screen.getByRole('combobox', { name: 'Filter profiles by status' }), { target: { value: 'attention' } });
    expect(screen.queryByText('Sales browser')).toBeNull();
    expect(screen.getByText('Review browser')).toBeTruthy();
    expect(mockedListProfiles).toHaveBeenCalledTimes(1);
    expect(mockedOpen).not.toHaveBeenCalled();
  });

  it('disables direct Open for viewer grants and non-launchable or deleted lifecycle states', async () => {
    mockedListProfiles.mockResolvedValue({
      profiles: [
        { profileId: 'profile_viewer_only', displayName: 'Viewer only', status: 'READY', version: 3,
          metadataVersion: 3, linkedClientId: null, activeGenerationId: null, canOperate: false },
        { profileId: 'profile_suspended', displayName: 'Suspended browser', status: 'SUSPENDED', version: 8,
          metadataVersion: 3, linkedClientId: null, activeGenerationId: null, canOperate: true },
        { profileId: 'profile_deleted', displayName: 'Deleted browser', status: 'DELETED', version: 9,
          metadataVersion: 5, linkedClientId: null, activeGenerationId: null, canOperate: true },
      ],
      nextCursor: null,
    });
    renderDirectory(async () => ({ tenants: [{
      tenantId: 'tenant_01JAUTH', displayName: 'Organization', actorId: 'actor_01JAUTH', role: 'MEMBER',
    }] }));

    const viewer = await screen.findByRole('button', { name: 'Open browser for Viewer only' }) as HTMLButtonElement;
    const suspended = screen.getByRole('button', { name: 'Open browser for Suspended browser' }) as HTMLButtonElement;
    expect(viewer.disabled).toBe(true);
    expect(viewer.title).toContain('Operator permission');
    expect(screen.getByText('Operator permission is required.')).toBeTruthy();
    expect(suspended.disabled).toBe(true);
    expect(suspended.title).toContain('SUSPENDED');
    expect(screen.getByText('Cannot open a SUSPENDED profile.')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Open browser for Deleted browser' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Show deleted profiles (1)' }));
    expect((screen.getByRole('button', { name: 'Open browser for Deleted browser' }) as HTMLButtonElement).disabled).toBe(true);
    expect(mockedOpen).not.toHaveBeenCalled();
  });

  it('shows a safe handoff failure without claiming an Open or retrying the native command', async () => {
    mockedListProfiles.mockResolvedValue({ profiles: [{
      profileId: 'profile_handoff_fail', displayName: 'Failed handoff', status: 'READY', version: 10,
      metadataVersion: 3, linkedClientId: null, activeGenerationId: null, canOperate: true,
    }], nextCursor: null });
    mockedOpen.mockImplementationOnce(() => { throw new Error('sensitive native details'); });
    renderDirectory(async () => ({ tenants: [{
      tenantId: 'tenant_01JAUTH', displayName: 'Organization', actorId: 'actor_01JAUTH', role: 'TENANT_OWNER',
    }] }));

    fireEvent.click(await screen.findByRole('button', { name: 'Open browser for Failed handoff' }));
    expect(mockedOpen).toHaveBeenCalledTimes(1);
    expect(screen.queryByText(/This page cannot confirm BrowserReady or cloud Saved/)).toBeNull();
    expect((await screen.findByRole('alert')).textContent).toContain('Could not hand off Failed handoff');
    expect(screen.queryByText(/sensitive native details/)).toBeNull();
  });
});
