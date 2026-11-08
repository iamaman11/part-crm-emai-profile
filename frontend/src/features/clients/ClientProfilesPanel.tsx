import { ApiProblem } from '../../shared/api/openapi-runtime';
import { useInfiniteQuery, useQueries, useQueryClient } from '@tanstack/react-query';
import { useEffect, useState, type FormEvent } from 'react';
import {
  assignProfile,
  detachProfile,
  dispatchBrowserProfileOpen,
  getProfile,
  launchProfile,
} from '../profiles';
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
  const [launchError, setLaunchError] = useState<Error | null>(null);
  const [launchingProfileId, setLaunchingProfileId] = useState<string | null>(null);
  const queryKey = ['client-registry', tenantId, clientId, 'profiles'] as const;

  const profiles = useInfiniteQuery({
    queryKey,
    initialPageParam: null as string | null,
    queryFn: ({ pageParam, signal }) => listClientProfiles(tenantId, clientId, signal, pageParam),
    getNextPageParam: (lastPage) => lastPage.nextCursor ?? undefined,
  });

  // The existing Worker ACL prepares these authorized v2 intents when the
  // already-visible client profile list loads, never on browser Open click.
  // No URI is synthesized from a profile ID or client relationship.
  const launchQuery = (profile: { profileId: string; version: number }) => ({
    queryKey: ['operator-query', tenantId, 'profile-launch', profile.profileId, profile.version] as const,
    queryFn: () => launchProfile(tenantId, profile.profileId, newIdempotencyKey()),
    staleTime: 60_000,
    retry: false,
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

  const visibleProfiles = profiles.data?.pages.flatMap((page) => page.profiles) ?? [];
  const launchable = visibleProfiles.filter((profile) => ['DRAFT', 'READY'].includes(profile.status));
  const preparedLaunches = useQueries({
    queries: launchable.map((profile) => ({
      ...launchQuery(profile),
      enabled: tenantId.length > 0,
      refetchOnWindowFocus: false,
      refetchOnReconnect: false,
    })),
  });
  // First click never starts a Worker request: any permission check is
  // already in progress from the separately authorized profile list.
  // A pending click dispatches automatically once that one request completes.
  useEffect(() => {
    if (launchingProfileId === null) return;
    const index = launchable.findIndex((profile) => profile.profileId === launchingProfileId);
    if (index < 0) {
      setLaunchingProfileId(null);
      return;
    }
    const proof = preparedLaunches[index];
    if (proof?.data) {
      try {
        dispatchBrowserProfileOpen(proof.data.launchUri);
      } catch {
        setLaunchError(new Error('Profile Bridge open was refused. Retry from this profile.'));
      } finally {
        setLaunchingProfileId(null);
      }
    } else if (proof?.isError) {
      setLaunchError(proof.error instanceof ApiProblem
        ? proof.error
        : new Error('Profile launch failed. Retry from this profile.'));
      setLaunchingProfileId(null);
    }
  }, [launchingProfileId, tenantId, clientId, profiles.data, preparedLaunches]);

  const requestLaunch = (profile: { profileId: string; status: string }) => {
    if (!['DRAFT', 'READY'].includes(profile.status) || launchingProfileId !== null) return;
    setLaunchError(null);
    setLaunchingProfileId(profile.profileId);
  };

  return (
    <section className="panel full-span">
      <span className="eyebrow">Business relationship · independent profile ACL</span>
      <h2>Browser profiles</h2>
      <p>
        This list contains only profiles independently visible to the active actor. Client visibility
        and assignment never grant profile access. Launch authorization, device selection and generation
        selection remain server-owned.
      </p>
      <p>First time on this Windows computer? <a href={`/devices?tenant=${encodeURIComponent(tenantId)}`}>Install and connect Profile Bridge</a> once, then launch any profile you can operate.</p>
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
                    disabled={!['DRAFT', 'READY'].includes(profile.status) || launchingProfileId !== null}
                    aria-label={`Launch profile ${profile.profileId}`}
                    aria-busy={launchingProfileId === profile.profileId}
                    onClick={() => requestLaunch(profile)}
                  >
                    {launchingProfileId === profile.profileId ? 'Opening…' : 'Launch'}
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
