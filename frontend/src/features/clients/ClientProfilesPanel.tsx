import { useInfiniteQuery, useQueryClient } from '@tanstack/react-query';
import { useState, type FormEvent } from 'react';
import { assignProfile, detachProfile, getProfile, launchProfile } from '../profiles';
import { invokeProfileBridgeLaunch } from '../profiles/launchBridge';
import { newIdempotencyKey } from '../../shared/api/idempotency';
import { ConfirmAction } from '../../shared/ui/ConfirmAction';
import { StatusMessage } from '../../shared/ui/StatusMessage';
import { useLogicalCommandMutation } from '../../shared/ui/useLogicalCommandMutation';
import { listClientProfiles } from './api';

function field(data: FormData, name: string): string {
  return String(data.get(name) ?? '').trim();
}

export function ClientProfilesPanel({
  tenantId,
  clientId,
  onMutated,
}: {
  tenantId: string;
  clientId: string;
  onMutated: () => Promise<void>;
}) {
  const queryClient = useQueryClient();
  const [detachReason, setDetachReason] = useState('');
  const [launchingProfileId, setLaunchingProfileId] = useState<string | null>(null);
  const [launchError, setLaunchError] = useState<Error | null>(null);
  const queryKey = ['client-registry', tenantId, clientId, 'profiles'] as const;

  const profiles = useInfiniteQuery({
    queryKey,
    initialPageParam: null as string | null,
    queryFn: ({ pageParam, signal }) => listClientProfiles(tenantId, clientId, signal, pageParam),
    getNextPageParam: (lastPage) => lastPage.nextCursor ?? undefined,
  });

  const refreshRelationship = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey }),
      queryClient.invalidateQueries({ queryKey: ['client-registry', tenantId, clientId, 'history'] }),
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
          expectedProfileVersion: profile.version,
        },
        idempotencyKey,
      );
    },
    { onSuccess: refreshRelationship },
  );

  const detach = useLogicalCommandMutation(
    (
      input: { profileId: string; expectedProfileVersion: number; reason: string },
      idempotencyKey,
    ) => detachProfile(
      tenantId,
      input.profileId,
      {
        expectedProfileVersion: input.expectedProfileVersion,
        reason: input.reason,
      },
      idempotencyKey,
    ),
    { onSuccess: refreshRelationship },
  );

  const requestLaunch = async (profileId: string) => {
    setLaunchError(null);
    setLaunchingProfileId(profileId);
    try {
      const launch = await launchProfile(tenantId, profileId, newIdempotencyKey());
      invokeProfileBridgeLaunch(launch.launchUri);
    } catch {
      setLaunchError(new Error('Profile launch failed. Retry from this profile.'));
    } finally {
      setLaunchingProfileId(null);
    }
  };

  const visibleProfiles = profiles.data?.pages.flatMap((page) => page.profiles) ?? [];

  return (
    <section className="panel full-span">
      <span className="eyebrow">Business relationship · independent profile ACL</span>
      <h2>Browser profiles</h2>
      <p>
        This list contains only profiles independently visible to the active actor. Client visibility
        and assignment never grant profile access. Launch authorization, device selection and generation
        selection remain server-owned.
      </p>
      <StatusMessage
        state={
          profiles.error
          ?? launchError
          ?? assign.error
          ?? detach.error
          ?? (profiles.isPending ? 'Loading attached profiles…' : null)
          ?? (assign.data ? `${assign.data.resultCode}: ${assign.data.resourceId}` : null)
          ?? (detach.data ? `${detach.data.resultCode}: ${detach.data.resourceId}` : null)
        }
      />

      {!profiles.isPending && visibleProfiles.length === 0 && !profiles.error ? (
        <p>No independently visible profiles are attached to this client.</p>
      ) : null}

      {visibleProfiles.length > 0 ? (
        <div className="stack-form">
          <label htmlFor={`client-profile-detach-reason-${clientId}`}>Detach reason</label>
          <input
            id={`client-profile-detach-reason-${clientId}`}
            value={detachReason}
            maxLength={500}
            onChange={(event) => setDetachReason(event.currentTarget.value)}
            placeholder="Required before detaching a profile"
          />
          <div className="registry-list">
            {visibleProfiles.map((profile) => (
              <article className="registry-row" key={profile.profileId}>
                <div>
                  <strong>{profile.profileId}</strong>
                  <div className="muted">
                    {profile.status} · version {profile.version}
                  </div>
                </div>
                <div className="row-actions">
                  <button
                    type="button"
                    disabled={launchingProfileId !== null}
                    aria-label={`Launch profile ${profile.profileId}`}
                    onClick={() => void requestLaunch(profile.profileId)}
                  >
                    {launchingProfileId === profile.profileId ? 'Launching…' : 'Launch'}
                  </button>
                  <ConfirmAction
                    label="detach profile"
                    consequence="This closes only the active Client/Profile relationship. The Client, Profile and explicit ACL grants remain intact."
                    disabled={detach.isPending || detachReason.trim().length === 0}
                    onConfirm={() => detach.mutateAsync({
                      profileId: profile.profileId,
                      expectedProfileVersion: profile.version,
                      reason: detachReason.trim(),
                    }).then(() => undefined)}
                  />
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
      <h3>Attach or reassign profile</h3>
      <p>
        The profile is loaded through its own visibility boundary first. Its current version is then
        used for the canonical optimistic assignment command; concurrent changes fail with a version conflict.
        Relationship identity is generated by the server from authenticated idempotent request evidence.
      </p>
      <form
        className="stack-form"
        onSubmit={(event: FormEvent<HTMLFormElement>) => {
          event.preventDefault();
          const data = new FormData(event.currentTarget);
          assign.mutate({
            profileId: field(data, 'profileId'),
            reason: field(data, 'reason'),
          });
        }}
      >
        <label htmlFor={`client-profile-attach-id-${clientId}`}>Profile ID</label>
        <input id={`client-profile-attach-id-${clientId}`} name="profileId" required placeholder="profile_..." />
        <label htmlFor={`client-profile-attach-reason-${clientId}`}>Reason</label>
        <input id={`client-profile-attach-reason-${clientId}`} name="reason" required maxLength={500} />
        <button type="submit" disabled={assign.isPending}>Attach / reassign</button>
      </form>
    </section>
  );
}
