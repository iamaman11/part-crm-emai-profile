import { useInfiniteQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState, type FormEvent } from 'react';
import {
  assignProfile,
  createProfile,
  detachProfile,
  getProfile,
  listProfiles,
  dispatchBrowserProfileOpen,
  ProfileOpenAction,
} from '../profiles';
import { newIdempotencyKey } from '../../shared/api/idempotency';
import { ConfirmAction } from '../../shared/ui/ConfirmAction';
import { StatusMessage } from '../../shared/ui/StatusMessage';
import { useLogicalCommandMutation } from '../../shared/ui/useLogicalCommandMutation';
import { listClientProfiles } from './api';
import { StatusBadge } from '../../shared/ui/StatusBadge';

function field(data: FormData, name: string): string {
  return String(data.get(name) ?? '').trim();
}

type AttachedProfile = Awaited<ReturnType<typeof listClientProfiles>>['profiles'][number];

function DetachProfileControl({ profile, disabled, busy, onDetach }: {
  profile: AttachedProfile;
  disabled: boolean;
  busy: boolean;
  onDetach: (profile: AttachedProfile, reason: string) => Promise<void>;
}) {
  const [reason, setReason] = useState('');
  return (
    <details className="relationship-tools">
      <summary className="inline-summary">Manage relationship</summary>
      <label className="stack-form" htmlFor={`detach-reason-${profile.profileId}`}>Detach reason</label>
      <input id={`detach-reason-${profile.profileId}`} value={reason} maxLength={500}
        onChange={(event) => setReason(event.currentTarget.value)} placeholder="Why should this profile be detached?" />
      <ConfirmAction
        label="detach profile"
        consequence="This closes only the active Client/Profile relationship. The Client, Profile and explicit ACL grants remain intact."
        disabled={disabled || busy || !reason.trim()}
        onConfirm={() => onDetach(profile, reason.trim())}
      />
    </details>
  );
}

export function ClientProfilesPanel({
  tenantId,
  actorId,
  clientId,
  onMutated,
}: {
  tenantId: string;
  actorId: string | null;
  clientId: string;
  onMutated: () => Promise<void>;
}) {
  const queryClient = useQueryClient();
  const [attachError, setAttachError] = useState<string | null>(null);
  const createForm = useRef<HTMLFormElement>(null);
  const missingActor = !actorId || !tenantId;
  const [openError, setOpenError] = useState<{ tenantId: string; clientId: string; error: Error } | null>(null);
  const queryKey = ['client-registry', tenantId, clientId, 'profiles'] as const;

  const profiles = useInfiniteQuery({
    queryKey,
    initialPageParam: null as string | null,
    queryFn: ({ pageParam, signal }) => listClientProfiles(tenantId, clientId, signal, pageParam),
    getNextPageParam: (lastPage) => lastPage.nextCursor ?? undefined,
  });

  // Independently visible profiles are discoverable by name. The client
  // relationship never grants profile visibility or permission to Open.
  const available = useInfiniteQuery({
    queryKey: ['operator-query', tenantId, 'profiles', 'client-picker'],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam, signal }) => listProfiles(tenantId, signal, pageParam),
    getNextPageParam: (lastPage) => lastPage.nextCursor ?? undefined,
  });

  const refreshRelationship = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey }),
      queryClient.invalidateQueries({ queryKey: ['client-registry', tenantId, clientId, 'history'] }),
      queryClient.invalidateQueries({ queryKey: ['operator-query', tenantId, 'profiles'] }),
      onMutated(),
    ]);
  };

  const assign = useLogicalCommandMutation(
    async (
      input: { profileId: string; reason: string },
      idempotencyKey,
    ) => {
      const profile = await getProfile(tenantId, input.profileId);
      return assignProfile(
        tenantId,
        input.profileId,
        {
          clientId,
          reason: input.reason,
          expectedMetadataVersion: profile.metadataVersion,
        },
        idempotencyKey,
      );
    },
    { onSuccess: refreshRelationship },
  );

  const createForClient = useLogicalCommandMutation(
    async (input: { tenantId: string; clientId: string; profileId: string; displayName: string }, idempotencyKey) => {
      // Create and association are separate canonical commands. Keep the
      // generated ID and creation idempotency key across a retry. Read back
      // before assigning: a lost association ACK must not create a new link.
      await createProfile(input.tenantId, input.profileId, input.displayName, idempotencyKey);
      const current = await getProfile(input.tenantId, input.profileId);
      if (current.linkedClientId !== input.clientId) {
        if (current.linkedClientId) {
          throw new Error('The newly created profile is linked to another client.');
        }
        await assignProfile(input.tenantId, input.profileId, {
          clientId: input.clientId,
          reason: 'Created from the client card',
          expectedMetadataVersion: current.metadataVersion,
        }, newIdempotencyKey());
      }
      const confirmed = await getProfile(input.tenantId, input.profileId);
      if (confirmed.linkedClientId !== input.clientId) {
        throw new Error('The profile association was not confirmed.');
      }
      return { profileId: input.profileId };
    },
    {
      onSuccess: async (_receipt, input) => {
        createForm.current?.reset();
        await Promise.all([
          queryClient.invalidateQueries({ queryKey: ['operator-query', input.tenantId, 'profiles'] }),
          queryClient.invalidateQueries({ queryKey: ['client-registry', input.tenantId, input.clientId, 'profiles'] }),
          queryClient.invalidateQueries({ queryKey: ['client-registry', input.tenantId, input.clientId, 'history'] }),
          onMutated(),
        ]);
      },
      onError: async (_error, input) => {
        // Creation may have committed even if association or its ACK failed.
        await queryClient.invalidateQueries({ queryKey: ['operator-query', input.tenantId, 'profiles'] });
      },
    },
  );

  const detach = useLogicalCommandMutation(
    (
      input: { profileId: string; expectedMetadataVersion: number; reason: string },
      idempotencyKey,
    ) => detachProfile(
      tenantId,
      input.profileId,
      {
        expectedMetadataVersion: input.expectedMetadataVersion,
        reason: input.reason,
      },
      idempotencyKey,
    ),
    { onSuccess: refreshRelationship },
  );

  const visibleProfiles = profiles.data?.pages.flatMap((page) => page.profiles) ?? [];
  const selectableProfiles = available.data?.pages.flatMap((page) => page.profiles)
    .filter((profile) => profile.linkedClientId !== clientId && profile.status !== 'DELETED' && profile.status !== 'DELETING') ?? [];
  return (
    <section className="panel full-span">
      <span className="eyebrow">Client workspace</span>
      <h2>Browser profiles</h2>
      <p>Open a ready profile or create a new one for this client.</p>
      <p className="muted">New computer? <a href={`/devices?tenant=${encodeURIComponent(tenantId)}`}>Install and connect Profile Bridge</a>. After closing a browser, refresh for the latest cloud status; Open alone does not confirm Saved.</p>
      <button
        type="button"
        className="secondary"
        disabled={profiles.isFetching || available.isFetching}
        onClick={() => { void Promise.all([profiles.refetch(), available.refetch()]); }}
      >
        {profiles.isFetching || available.isFetching ? 'Refreshing profiles…' : 'Refresh profiles'}
      </button>
      <StatusMessage
        state={
          profiles.error
          ?? assign.error
          ?? detach.error
          ?? (profiles.isPending ? 'Loading attached profiles…' : null)
          ?? (assign.data ? `${assign.data.resultCode}: ${assign.data.resourceId}` : null)
          ?? (detach.data ? `${detach.data.resultCode}: ${detach.data.resourceId}` : null)
        }
      />
      <StatusMessage state={openError?.tenantId === tenantId && openError.clientId === clientId ? openError.error : null} />

      {!profiles.isPending && visibleProfiles.length === 0 && !profiles.error ? (
        <p>No profiles attached to this client are visible to your account. Create one here or attach an existing accessible profile below.</p>
      ) : null}

      {visibleProfiles.length > 0 && (
        <p className="muted">{visibleProfiles.length} client profile{visibleProfiles.length === 1 ? '' : 's'} loaded{profiles.hasNextPage ? ' · more available below' : ''}.</p>
      )}

      {visibleProfiles.length > 0 ? (
        <div className="stack-form">
          <div className="registry-list">
            {visibleProfiles.map((profile) => (
              <article className="registry-row" key={profile.profileId}>
                <div>
                  <strong>{profile.displayName}</strong>
                  <div><StatusBadge status={profile.status} /></div>
                  <details className="profile-row-id"><summary>Technical details</summary>{profile.displayName !== profile.profileId && <><code>{profile.profileId}</code> · </>}browser v{profile.version} · metadata v{profile.metadataVersion}</details>
                </div>
                <div className="row-actions">
                  <ProfileOpenAction
                    tenantId={tenantId}
                    actorId={actorId}
                    profile={profile}
                    dispatchOpen={dispatchBrowserProfileOpen}
                    onError={(error) => setOpenError(error ? { tenantId, clientId, error } : null)}
                  />
                  <a
                    href={`/profiles/${encodeURIComponent(profile.profileId)}?tenant=${encodeURIComponent(tenantId)}`}
                    aria-label={`View profile ${profile.profileId}`}
                  >
                    View profile
                  </a>
                  <DetachProfileControl profile={profile} disabled={missingActor} busy={detach.isPending}
                    onDetach={(target, reason) => detach.mutateAsync({
                      profileId: target.profileId,
                      expectedMetadataVersion: target.metadataVersion,
                      reason,
                    }).then(() => undefined)} />
                </div>
              </article>
            ))}
          </div>
        </div>
      ) : null}

      {profiles.hasNextPage ? (
        <button
          type="button"
          disabled={profiles.isFetchingNextPage}
          onClick={() => profiles.fetchNextPage()}
        >
          {profiles.isFetchingNextPage ? 'Loading more…' : 'Load more profiles'}
        </button>
      ) : null}

      <hr />
      <h3>Create a profile for this client</h3>
      <p>Give the browser a name. Its identifier is generated automatically; the new profile is linked to this client only after a confirmed server readback.</p>
      <form
        ref={createForm}
        className="stack-form"
        onSubmit={(event: FormEvent<HTMLFormElement>) => {
          event.preventDefault();
          const name = field(new FormData(event.currentTarget), 'displayName');
          if (!name || missingActor || createForClient.isPending || createForClient.isError) return;
          createForClient.mutate({
            tenantId,
            clientId,
            profileId: `profile_${globalThis.crypto.randomUUID().replaceAll('-', '')}`,
            displayName: name,
          });
        }}
      >
        <label htmlFor={`client-profile-create-name-${clientId}`}>New profile name</label>
        <input id={`client-profile-create-name-${clientId}`} name="displayName" required minLength={1} maxLength={80} placeholder="For example, Sales mailbox" disabled={missingActor || createForClient.isPending || createForClient.isError} />
        <button type="submit" disabled={missingActor || createForClient.isPending || createForClient.isError}>{createForClient.isPending ? 'Creating and linking…' : 'Create and attach profile'}</button>
      </form>
      {createForClient.data && !createForClient.error && (
        <p role="status">Profile created and linked. <a href={`/profiles/${encodeURIComponent(createForClient.data.profileId)}?tenant=${encodeURIComponent(tenantId)}`}>Open profile details</a></p>
      )}
      {createForClient.error && (
        <div role="alert">
          <p>Profile creation or attachment is not confirmed. Creating another profile is paused to avoid duplicates. Check the profile and retry the same request.</p>
          {createForClient.variables?.input && (
            <p>Profile ID: <code>{createForClient.variables.input.profileId}</code> · <a href={`/profiles/${encodeURIComponent(createForClient.variables.input.profileId)}?tenant=${encodeURIComponent(tenantId)}`}>Inspect profile</a></p>
          )}
          <button type="button" disabled={missingActor || createForClient.isPending} onClick={() => { void createForClient.retryLatestCommand().catch(() => undefined); }}>Retry creation / attachment</button>
        </div>
      )}

      <hr />
      <h3>Attach or reassign an existing profile</h3>
      <p>
        Select an accessible profile by name. The server separately checks profile visibility,
        current version and permission to change the client relationship.
      </p>
      <StatusMessage state={available.error ?? (available.isPending ? 'Loading available profiles…' : null) ?? attachError} />
      <form
        className="stack-form"
        onSubmit={(event: FormEvent<HTMLFormElement>) => {
          event.preventDefault();
          const data = new FormData(event.currentTarget);
          const selectedId = field(data, 'manualProfileId') || field(data, 'profileId');
          if (missingActor) {
            setAttachError('Organization access is unavailable.');
            return;
          }
          if (!selectedId) {
            setAttachError('Choose a profile or enter an identifier in Advanced options.');
            return;
          }
          setAttachError(null);
          assign.mutate({
            profileId: selectedId,
            reason: field(data, 'reason'),
          });
        }}
      >
        <label htmlFor={`client-profile-attach-choice-${clientId}`}>Available profiles</label>
        <select id={`client-profile-attach-choice-${clientId}`} name="profileId" disabled={missingActor || assign.isPending} defaultValue="">
          <option value="">Choose a profile…</option>
          {selectableProfiles.map((profile) => (
            <option key={profile.profileId} value={profile.profileId}>
              {profile.displayName} · {profile.status} · {profile.profileId}
            </option>
          ))}
        </select>
        <details>
          <summary>Advanced: enter a profile ID</summary>
          <label htmlFor={`client-profile-attach-id-${clientId}`}>Profile ID</label>
          <input id={`client-profile-attach-id-${clientId}`} name="manualProfileId" placeholder="profile_..." disabled={missingActor || assign.isPending} />
        </details>
        <label htmlFor={`client-profile-attach-reason-${clientId}`}>Reason for association</label>
        <input id={`client-profile-attach-reason-${clientId}`} name="reason" required maxLength={500} disabled={missingActor || assign.isPending} />
        <button type="submit" disabled={missingActor || assign.isPending}>Attach / reassign</button>
      </form>
      {available.hasNextPage && (
        <button type="button" disabled={available.isFetchingNextPage} onClick={() => { void available.fetchNextPage(); }}>
          {available.isFetchingNextPage ? 'Loading more available profiles…' : 'Load more available profiles'}
        </button>
      )}
    </section>
  );
}
