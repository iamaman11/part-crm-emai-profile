import { dispatchBrowserProfileDelete, dispatchBrowserProfileOpen } from './api';
import { newIdempotencyKey } from '../../shared/api/idempotency';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useState, type FormEvent } from 'react';
import { useTenant } from '../../app/TenantContext';
import {
  assignProfile,
  changeGenerationActivation,
  commandCoordinator,
  createProfile,
  deleteProfile,
  detachProfile,
  getCoordinator,
  getGeneration,
  getProfile,
  quarantineGeneration,
  renameProfile,
  registerGeneration,
  setProfileGrant,
  verifyGeneration,
} from './api';
import { useLogicalCommandMutation } from '../../shared/ui/useLogicalCommandMutation';
import type { CoordinatorResponse, GenerationProjection, ProfileProjection } from './api';
import { ConfirmAction } from '../../shared/ui/ConfirmAction';
import { StatusMessage } from '../../shared/ui/StatusMessage';

function field(data: FormData, name: string): string {
  return String(data.get(name) ?? '').trim();
}

type ProfileSelection = { tenantId: string; profileId: string };
type GenerationActionTarget = ProfileSelection & { generationId: string };
type GenerationActionCommand = (
  | { kind: 'verify'; expectedVersion: number; reference: string }
  | { kind: 'activate' | 'deactivate'; expectedVersion: number }
  | { kind: 'quarantine'; expectedVersion: number }
) & { target: GenerationActionTarget };

type ProfileNameCommand = ProfileSelection & { displayName: string; metadataVersion: number };
type ProfileNameReadback = ProfileSelection & { displayName: string; minMetadataVersion: number };

type ProfileClientAssociationCommand =
  ProfileSelection
  & { metadataVersion: number; reason: string }
  & ({ kind: 'attach'; clientId: string } | { kind: 'detach' });

type ProfileClientAssociationReadback = ProfileSelection & {
  minMetadataVersion: number;
  linkedClientId: string | null;
};

interface GrantDraft {
  actorId: string;
  role: 'PROFILE_VIEWER' | 'PROFILE_OPERATOR';
  reason: string;
  expectedMetadataVersion: number;
}

// Projection reads can finish out of order for the same selected profile.
// Keep the last authoritative version; terminal DELETED must not resurrect Open.
function acceptProfileReadback(
  previous: ProfileProjection | null,
  latest: ProfileProjection | null,
): ProfileProjection | null {
  if (!previous || !latest || previous.profileId !== latest.profileId) return latest;
  if (latest.version < previous.version || latest.metadataVersion < previous.metadataVersion || (previous.status === 'DELETED' && latest.status !== 'DELETED')) {
    return previous;
  }
  return latest;
}

export function ProfilesWorkspace({
  selectedProfileId = null,
  onProfileSelected,
}: {
  selectedProfileId?: string | null;
  onProfileSelected: (profileId: string) => void;
}) {
  const { tenantId, contexts } = useTenant();
  const cache = useQueryClient();
  const [profileId, setProfileId] = useState(selectedProfileId ?? '');
  const [profile, setProfile] = useState<ProfileProjection | null>(null);
  const activeTenant = contexts.find((context) => context.tenantId === tenantId);
  const owner = activeTenant?.role === 'TENANT_OWNER';
  const [generationId, setGenerationId] = useState('');
  const [generation, setGeneration] = useState<GenerationProjection | null>(null);
  const [coordinator, setCoordinator] = useState<CoordinatorResponse | null>(null);
  const [generationRefreshUnconfirmed, setGenerationRefreshUnconfirmed] = useState(false);
  const selected = useRef({ tenantId, profileId, generationId });
  selected.current = { tenantId, profileId, generationId };

  const [profileNameDraft, setProfileNameDraft] = useState('');
  const [profileNameUnconfirmed, setProfileNameUnconfirmed] = useState(false);
  const profileNameReadback = useRef<ProfileNameReadback | null>(null);
  const [clientIdDraft, setClientIdDraft] = useState('');
  const [relationshipReason, setRelationshipReason] = useState('');
  const [relationshipRefreshUnconfirmed, setRelationshipRefreshUnconfirmed] = useState(false);
  const relationshipReadback = useRef<ProfileClientAssociationReadback | null>(null);
  const [openError, setOpenError] = useState<Error | null>(null);
  const [localDeleteError, setLocalDeleteError] = useState<Error | null>(null);
  const requestLocalDeletion = (terminal: ProfileProjection) => {
    if (!owner || !activeTenant || terminal.status !== 'DELETED'
      || terminal.profileId !== profileId) return;
    try {
      setLocalDeleteError(null);
      dispatchBrowserProfileDelete(
        tenantId, activeTenant.actorId, terminal.profileId, terminal.version,
      );
    } catch {
      setLocalDeleteError(new Error('Installed Profile Bridge did not accept local cleanup.'));
    }
  };
  const openSelectedProfile = () => {
    // Native protocol dispatch must stay synchronous in the actual user
    // click. No ReactQuery mutation, background prefetch or Worker launch.
    if (!activeTenant || !profile || profile.profileId !== profileId
      || profile.canOperate !== true
      || (profile.status !== 'DRAFT' && profile.status !== 'READY')
      || generationRefreshUnconfirmed) {
      setOpenError(new Error('The selected local profile is not ready to open.'));
      return;
    }
    try {
      setOpenError(null);
      dispatchBrowserProfileOpen(
        tenantId, activeTenant.actorId, profile.profileId, profile.version, profile.status,
      );
    } catch {
      setOpenError(new Error('Native Profile Bridge did not accept this profile Open.'));
    }
  };
  const lookup = useMutation({
    mutationFn: (target: ProfileSelection) => getProfile(target.tenantId, target.profileId),
    onSuccess: (data, target) => {
      // Late readback for the previous profile/tenant must never replace the
      // newly selected profile, even if the previous request succeeded.
      if (selected.current.tenantId === target.tenantId
        && selected.current.profileId === target.profileId) {
        const expected = relationshipReadback.current;
        if (expected
          && expected.tenantId === target.tenantId
          && expected.profileId === target.profileId) {
          if (!data || data.metadataVersion < expected.minMetadataVersion
            || data.linkedClientId !== expected.linkedClientId) {
            setRelationshipRefreshUnconfirmed(true);
            return;
          }
          relationshipReadback.current = null;
          setRelationshipRefreshUnconfirmed(false);
        }
        const nameExpected = profileNameReadback.current;
        if (nameExpected && nameExpected.tenantId === target.tenantId && nameExpected.profileId === target.profileId) {
          if (!data || data.metadataVersion < nameExpected.minMetadataVersion || data.displayName !== nameExpected.displayName) {
            setProfileNameUnconfirmed(true);
            return;
          }
          profileNameReadback.current = null;
          setProfileNameUnconfirmed(false);
        }
        if (data) setProfileNameDraft(data.displayName);
        setProfile((previous) => acceptProfileReadback(previous, data ?? null));
      }
    },
  });
  const create = useLogicalCommandMutation((input: { profileId: string; displayName: string }, idempotencyKey) => createProfile(tenantId, input.profileId, input.displayName, idempotencyKey), {
    onSuccess: (receipt) => {
      void cache.invalidateQueries({ queryKey: ['operator-query', tenantId, 'profiles'] });
      if (receipt?.resourceId) onProfileSelected(receipt.resourceId);
    },
  });
  const rename = useLogicalCommandMutation(
    (command: ProfileNameCommand, idempotencyKey) => renameProfile(
      command.tenantId, command.profileId,
      { displayName: command.displayName, expectedMetadataVersion: command.metadataVersion },
      idempotencyKey,
    ),
    {
      onSuccess: async (_receipt, command) => {
        if (selected.current.tenantId !== command.tenantId
          || selected.current.profileId !== command.profileId) return;
        const expected: ProfileNameReadback = {
          tenantId: command.tenantId, profileId: command.profileId,
          displayName: command.displayName, minMetadataVersion: command.metadataVersion + 1,
        };
        profileNameReadback.current = expected;
        setProfileNameUnconfirmed(true);
        void cache.invalidateQueries({ queryKey: ['operator-query', command.tenantId, 'profiles'] });
        try {
          const refreshed = await getProfile(command.tenantId, command.profileId);
          if (selected.current.tenantId !== command.tenantId
            || selected.current.profileId !== command.profileId
            || profileNameReadback.current !== expected
            || refreshed.metadataVersion < expected.minMetadataVersion
            || refreshed.displayName !== expected.displayName) return;
          profileNameReadback.current = null;
          setProfileNameUnconfirmed(false);
          setProfile((previous) => acceptProfileReadback(previous, refreshed));
          setProfileNameDraft(refreshed.displayName);
        } catch {
          // The command is not replayed when its readback is lost.
        }
      },
    },
  );
  const deleteAction = useMutation({
    mutationFn: async ({ targetTenant, targetProfile, expectedVersion }: {
      targetTenant: string;
      targetProfile: string;
      expectedVersion: number;
    }) => deleteProfile(targetTenant, targetProfile, expectedVersion),
    onSuccess: async (receipt, variables) => {
      await cache.invalidateQueries({ queryKey: ['operator-query', variables.targetTenant, 'profiles'] });
      if (selected.current.tenantId !== variables.targetTenant
        || selected.current.profileId !== variables.targetProfile) return;
      try {
        const refreshed = await getProfile(variables.targetTenant, variables.targetProfile);
        setProfile((previous) => acceptProfileReadback(previous, refreshed ?? null));
      } catch {
        // A stale local projection cannot be used as proof of deletion.
        setProfile(null);
      }
      if (receipt.status === 'DELETED') setGeneration(null);
    },
  });
  // Client linkage is a business relationship, not an ACL or browser state.
  // Reuse the identical typed optimistic commands already used in ClientProfilesPanel.
  const relationship = useLogicalCommandMutation(
    (command: ProfileClientAssociationCommand, idempotencyKey) =>
      command.kind === 'attach'
        ? assignProfile(command.tenantId, command.profileId, {
          clientId: command.clientId,
          reason: command.reason,
          expectedMetadataVersion: command.metadataVersion,
        }, idempotencyKey)
        : detachProfile(command.tenantId, command.profileId, {
          reason: command.reason,
          expectedMetadataVersion: command.metadataVersion,
        }, idempotencyKey),
    {
      onSuccess: async (_receipt, command) => {
        const expected: ProfileClientAssociationReadback = {
          tenantId: command.tenantId,
          profileId: command.profileId,
          minMetadataVersion: command.metadataVersion + 1,
          linkedClientId: command.kind === 'attach' ? command.clientId : null,
        };
        if (selected.current.tenantId !== command.tenantId
          || selected.current.profileId !== command.profileId) return;
        relationshipReadback.current = expected;
        setRelationshipRefreshUnconfirmed(true);
        void cache.invalidateQueries({ queryKey: ['operator-query', command.tenantId, 'profiles'] });
        void cache.invalidateQueries({ queryKey: ['client-registry', command.tenantId] });
        try {
          const refreshed = await getProfile(command.tenantId, command.profileId);
          if (selected.current.tenantId !== command.tenantId
            || selected.current.profileId !== command.profileId
            || relationshipReadback.current !== expected) return;
          if (refreshed.metadataVersion < expected.minMetadataVersion
            || refreshed.linkedClientId !== expected.linkedClientId) return;
          relationshipReadback.current = null;
          setRelationshipRefreshUnconfirmed(false);
          setRelationshipReason('');
          setProfile((previous) => acceptProfileReadback(previous, refreshed));
        } catch {
          // Never infer success from a receipt or replay a mutation on readback failure.
        }
      },
    },
  );
  const grant = useLogicalCommandMutation((input: GrantDraft & { revoke: boolean }, idempotencyKey) => setProfileGrant(
      tenantId,
      profileId,
      input.actorId,
      { role: input.role, reason: input.reason, expectedMetadataVersion: input.expectedMetadataVersion },
      idempotencyKey,
      input.revoke,
    ), {});
  const generationLookup = useMutation({
    mutationFn: (target: GenerationActionTarget) =>
      getGeneration(target.tenantId, target.profileId, target.generationId),
    onSuccess: (data, target) => {
      // A stale generation from another selection can never authorize actions.
      if (selected.current.tenantId === target.tenantId
        && selected.current.profileId === target.profileId
        && selected.current.generationId === target.generationId) setGeneration(data ?? null);
    },
  });
  const generationRegister = useLogicalCommandMutation((input: { generationId: string; objectKey: string; metadataDigest: string; containerDigest: string }, idempotencyKey) =>
    registerGeneration(tenantId, profileId, input, idempotencyKey));
  async function refreshGenerationProjection(
    requested: GenerationActionTarget = { tenantId, profileId, generationId },
  ): Promise<void> {
    // Refresh is read-only and does not repeat the successful generation action.
    // Use the invocation target for successful mutations: a later selection
    // must never inherit a previous profile's mutation receipt.
    const [profileResult, generationResult, directoryResult] = await Promise.allSettled([
      getProfile(requested.tenantId, requested.profileId),
      getGeneration(requested.tenantId, requested.profileId, requested.generationId),
      cache.invalidateQueries({ queryKey: ['operator-query', requested.tenantId, 'profiles'] }),
    ]);
    if (selected.current.tenantId !== requested.tenantId
      || selected.current.profileId !== requested.profileId
      || selected.current.generationId !== requested.generationId) return;
    if (profileResult.status === 'fulfilled' && generationResult.status === 'fulfilled'
      && directoryResult.status === 'fulfilled') {
      setProfile(profileResult.value ?? null);
      setGeneration(generationResult.value ?? null);
      setGenerationRefreshUnconfirmed(false);
    } else {
      // Fail closed: a stale READY snapshot cannot authorize another Open.
      setGenerationRefreshUnconfirmed(true);
    }
  }

  const generationAction = useLogicalCommandMutation((input: GenerationActionCommand, idempotencyKey) => {
      const { tenantId: commandTenant, profileId: commandProfile, generationId: commandGeneration } = input.target;
      if (input.kind === 'verify') {
        return verifyGeneration(commandTenant, commandProfile, commandGeneration, {
          expectedGenerationVersion: input.expectedVersion,
          verificationReference: input.reference,
        }, idempotencyKey);
      }
      if (input.kind === 'quarantine') {
        return quarantineGeneration(commandTenant, commandProfile, commandGeneration, input.expectedVersion, idempotencyKey);
      }
      return changeGenerationActivation(commandTenant, commandProfile, commandGeneration, input.expectedVersion, input.kind === 'activate', idempotencyKey);
    }, {
      onSuccess: (_receipt, input) => refreshGenerationProjection(input.target),
    },
  );
  const coordinatorLookup = useMutation({
    mutationFn: (target: ProfileSelection) => getCoordinator(target.tenantId, target.profileId),
    onSuccess: (data, target) => {
      // Never show a stale owner/lease projection for the newly selected profile.
      if (selected.current.tenantId === target.tenantId
        && selected.current.profileId === target.profileId) setCoordinator(data ?? null);
    },
  });
  const coordinatorCommand = useLogicalCommandMutation((command: import('./api').CoordinatorCommandDto, idempotencyKey) => {
      if (!coordinator) throw new Error('Load the coordinator snapshot before issuing a command.');
      return commandCoordinator(tenantId, profileId, {
        idempotency_key: idempotencyKey,
        sequence: coordinator.sequence + 1,
        expected_version: coordinator.version,
        command,
      });
    }, {
    onSuccess: (data) => setCoordinator(data ?? null),
  });

  useEffect(() => {
    const nextProfileId = selectedProfileId ?? '';
    setProfileId(nextProfileId);
    setProfile(null);
    setGenerationId('');
    setGeneration(null);
    setCoordinator(null);
    setGenerationRefreshUnconfirmed(false);
    setProfileNameDraft('');
    setProfileNameUnconfirmed(false);
    profileNameReadback.current = null;
    setClientIdDraft('');
    setRelationshipReason('');
    setRelationshipRefreshUnconfirmed(false);
    relationshipReadback.current = null;
    if (tenantId && nextProfileId) lookup.mutate({ tenantId, profileId: nextProfileId });
  }, [tenantId, selectedProfileId]);

  useEffect(() => {
    // Bridge saves are performed outside this tab. On return, read the
    // canonical projections through their existing query owners; a focus
    // event is a revalidation signal, not a second realtime control plane.
    const refresh = () => {
      if (!tenantId) return;
      void cache.invalidateQueries({ queryKey: ['operator-query', tenantId, 'profiles'] });
      if (selectedProfileId) lookup.mutate({ tenantId, profileId: selectedProfileId });
    };
    const onVisibility = () => {
      if (globalThis.document.visibilityState === 'visible') refresh();
    };
    globalThis.addEventListener('focus', refresh);
    globalThis.document.addEventListener('visibilitychange', onVisibility);
    return () => {
      globalThis.removeEventListener('focus', refresh);
      globalThis.document.removeEventListener('visibilitychange', onVisibility);
    };
  }, [tenantId, selectedProfileId, cache]);

  useEffect(() => {
    // Only a confirmed DELETING projection needs periodic readback. The existing
    // Worker remains the sole Delete owner; never replay its mutation or launch
    // the native protocol from an asynchronous browser callback.
    if (!tenantId || !profileId || profile?.status !== 'DELETING' || profile.profileId !== profileId) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout>;
    const refreshPendingDelete = async () => {
      if (globalThis.document.visibilityState === 'visible') {
        try {
          const latest = await getProfile(tenantId, profileId);
          if (!cancelled && latest?.profileId === profileId
            && selected.current.tenantId === tenantId
            && selected.current.profileId === profileId) {
            setProfile((previous) => acceptProfileReadback(previous, latest));
          }
        } catch {
          // Keep the last confirmed projection; normal Refresh remains usable.
        }
      }
      if (!cancelled) timer = globalThis.setTimeout(() => { void refreshPendingDelete(); }, 5000);
    };
    timer = globalThis.setTimeout(() => { void refreshPendingDelete(); }, 5000);
    return () => {
      cancelled = true;
      globalThis.clearTimeout(timer);
    };
  }, [tenantId, profileId, profile?.status, profile?.version]);

  const enabled = tenantId.length > 0;
  // Explain an existing projection directly. Never query Worker merely to
  // decide whether a local Open button can be enabled.
  const openUnavailableReason = !profile ? null
    : profile.status === 'DELETED' ? 'This profile was deleted and cannot be opened.'
    : profile.status === 'DELETING' ? 'This profile is being deleted and cannot be opened.'
    : !['DRAFT', 'READY'].includes(profile.status) ? `This profile is ${profile.status} and cannot be opened.`
    : !activeTenant ? 'Select an authorized tenant before opening this profile.'
    : profile.profileId !== profileId ? 'Refresh the selected profile before opening it.'
    : profile.canOperate !== true ? 'This account has no operator permission for this profile.'
    : generationAction.isPending ? 'The profile generation is being updated. Wait for the result.'
    : generationRefreshUnconfirmed ? 'The latest profile version is not confirmed. Refresh status.'
    : null;
  // A mutation receipt is historical; the last confirmed Profile projection is
  // the only authority for a terminal Delete. Do not leave DELETING on-screen
  // after a later Refresh observes DELETED (or reuse another profile's error).
  const deleteBelongsToSelection = deleteAction.variables?.targetTenant === tenantId
    && deleteAction.variables.targetProfile === profileId;
  const deletionStatus = !profile ? null
    : profile.status === 'DELETED' ? 'Cloud deletion confirmed. To discard the local browser copy, close Camoufox and click Remove local copy. Local removal has not been verified.'
    : profile.status === 'DELETING'
      ? 'Deletion is pending; the server resumes automatically after signed upload permissions expire.'
      : deleteBelongsToSelection && deleteAction.variables?.expectedVersion === profile.version
        ? deleteAction.error
        : null;
  const profileLoaded = enabled && profileId.length > 0 && profile !== null;
  const actionTarget = generationAction.variables?.input.target;
  const actionBelongsToSelection = actionTarget?.tenantId === tenantId
    && actionTarget.profileId === profileId
    && actionTarget.generationId === generationId;

  return (
    <div className="workspace-grid">
      <section className="panel">
        <span className="eyebrow">Browser profile</span>
        <h2>{profile?.displayName ?? profile?.profileId ?? selectedProfileId ?? 'Profile'}</h2>
        <details><summary>Find a profile by identifier</summary>
        <form
          className="stack-form"
          onSubmit={(event) => {
            event.preventDefault();
            onProfileSelected(field(new FormData(event.currentTarget), 'profileId'));
          }}
        >
          <label htmlFor="profile-lookup-id">Profile ID</label>
          <input id="profile-lookup-id" name="profileId" placeholder="profile_..." required disabled={!enabled} />
          <button type="submit" disabled={!enabled || lookup.isPending}>Find profile</button>
        </form>
        </details>
        <StatusMessage state={lookup.error ?? (lookup.isPending ? 'Loading profile…' : null)} />
        {selectedProfileId && !lookup.isPending && !profile && !lookup.error && (
          <p>The selected profile is not visible to the active actor.</p>
        )}
        {profile && <>
          <div className="action-row">
            <button type="button" disabled={openUnavailableReason !== null} aria-describedby={openUnavailableReason ? 'profile-open-status' : undefined} onClick={openSelectedProfile}>Open browser</button>
            <button type="button" disabled={lookup.isPending} onClick={() => {
              void cache.invalidateQueries({ queryKey: ['operator-query', tenantId, 'profiles'] });
              lookup.mutate({ tenantId, profileId });
            }}>Refresh status</button>
            {owner && ['DRAFT', 'READY', 'SUSPENDED', 'QUARANTINED'].includes(profile.status) && (
              <ConfirmAction
                label="Delete profile"
                consequence={<>Delete this profile and its encrypted cloud snapshots. This cannot be undone. Close any open browser first if you need to save unsynced local changes.</>}
                disabled={deleteAction.isPending}
                onConfirm={async () => {
                  await deleteAction.mutateAsync({
                    targetTenant: tenantId,
                    targetProfile: profile.profileId,
                    expectedVersion: profile.version,
                  });
                }}
              />
            )}
          </div>
          {openUnavailableReason && <p id="profile-open-status" role="status">{openUnavailableReason}</p>}
          <StatusMessage state={openError} />
          <StatusMessage state={deletionStatus} />
          {owner && profile.status === 'DELETED' && (
            <div>
              <button type="button" onClick={() => requestLocalDeletion(profile)}>
                Remove local copy
              </button>
              <p>Cloud deletion is confirmed. This button asks installed Profile Bridge to
                verify DELETED and native browser locks before removing local files. The browser does not receive a completion receipt.</p>
              <StatusMessage state={localDeleteError} />
            </div>
          )}
        </>}
        {profile && <>
          <dl className="projection">
            <div><dt>Status</dt><dd>{profile.status}</dd></div>
            <div><dt>Client</dt><dd>{profile.linkedClientId ?? 'Not assigned'}</dd></div>
          </dl>
          {owner && ['DRAFT', 'READY', 'SUSPENDED', 'QUARANTINED'].includes(profile.status) && (
            <details>
              <summary>Rename profile</summary>
              <form className="stack-form" onSubmit={(event) => {
                event.preventDefault();
                if (rename.isPending || profileNameUnconfirmed || relationship.isPending
                  || !profileNameDraft.trim() || profileNameDraft.trim() === profile.displayName) return;
                rename.mutate({
                  tenantId, profileId: profile.profileId, metadataVersion: profile.metadataVersion,
                  displayName: profileNameDraft.trim(),
                });
              }}>
                <label htmlFor="profile-display-name">Profile name</label>
                <input id="profile-display-name" value={profileNameDraft}
                  onChange={(event) => setProfileNameDraft(event.currentTarget.value)}
                  maxLength={80} required />
                <button type="submit" disabled={rename.isPending || profileNameUnconfirmed
                  || relationship.isPending || !profileNameDraft.trim()
                  || profileNameDraft.trim() === profile.displayName}>Save name</button>
              </form>
              <StatusMessage state={rename.error ?? (rename.isPending ? 'Renaming profile…' : null)} />
              {profileNameUnconfirmed && <div role="alert">
                Rename command completed, but the updated name was not confirmed. Refresh without repeating the command.
                <button type="button" disabled={lookup.isPending}
                  onClick={() => lookup.mutate({ tenantId, profileId: profile.profileId })}>
                  Refresh profile name
                </button>
              </div>}
            </details>
          )}
          {owner && ['DRAFT', 'READY', 'SUSPENDED', 'QUARANTINED'].includes(profile.status) && (
            <details>
              <summary>Manage client association</summary>
              <p>Client linkage is a business relationship; it does not grant profile access.</p>
              <form className="stack-form" onSubmit={(event: FormEvent<HTMLFormElement>) => {
                event.preventDefault();
                if (relationship.isPending || rename.isPending || profileNameUnconfirmed || relationshipRefreshUnconfirmed
                  || !clientIdDraft.trim() || !relationshipReason.trim()) return;
                relationship.mutate({
                  kind: 'attach', tenantId, profileId: profile.profileId,
                  metadataVersion: profile.metadataVersion, clientId: clientIdDraft.trim(), reason: relationshipReason.trim(),
                });
              }}>
                <label htmlFor="profile-client-association-id">Client ID</label>
                <input id="profile-client-association-id" value={clientIdDraft} onChange={(event) => setClientIdDraft(event.currentTarget.value)} placeholder="client_..." required />
                <label htmlFor="profile-client-association-reason">Reason</label>
                <input id="profile-client-association-reason" value={relationshipReason} onChange={(event) => setRelationshipReason(event.currentTarget.value)} maxLength={500} required />
                <button type="submit" disabled={relationship.isPending || rename.isPending || profileNameUnconfirmed || relationshipRefreshUnconfirmed || !clientIdDraft.trim() || !relationshipReason.trim() || clientIdDraft.trim() === profile.linkedClientId}>
                  {profile.linkedClientId ? 'Change client' : 'Attach client'}
                </button>
              </form>
              {profile.linkedClientId && (
                <ConfirmAction
                  label="Detach client"
                  consequence="Remove only the Client/Profile relationship. Browser data and independent profile access remain unchanged."
                  disabled={relationship.isPending || rename.isPending || profileNameUnconfirmed || relationshipRefreshUnconfirmed || !relationshipReason.trim()}
                  onConfirm={() => relationship.mutateAsync({
                    kind: 'detach', tenantId, profileId: profile.profileId,
                    metadataVersion: profile.metadataVersion, reason: relationshipReason.trim(),
                  }).then(() => undefined)}
                />
              )}
              <StatusMessage state={relationship.error ?? (relationship.isPending ? 'Updating client relationship…' : null)} />
              {relationshipRefreshUnconfirmed && (
                <div role="alert">
                  Relationship command completed, but the new metadata version was not confirmed. Refresh the read model; do not repeat the command.
                  <button type="button" onClick={() => lookup.mutate({ tenantId, profileId: profile.profileId })} disabled={lookup.isPending}>Refresh relationship</button>
                </div>
              )}
            </details>
          )}
          <details>
            <summary>Technical identifiers</summary>
            <dl className="projection">
              <div><dt>ID</dt><dd>{profile.profileId}</dd></div>
              <div><dt>Version</dt><dd>{profile.version}</dd></div>
            </dl>
          </details>
        </>}
      </section>

      {!selectedProfileId && <section className="panel">
        <span className="eyebrow">New browser profile</span>
        <h2>Create profile</h2>
        <p>Name your profile, then choose Open browser. The application prepares the browser state and saves changes when you close it.</p>
        <form className="stack-form" onSubmit={(event) => {
          event.preventDefault();
          const data = new FormData(event.currentTarget);
          create.mutate({ profileId: field(data, 'profileId') || `profile_${globalThis.crypto.randomUUID().replaceAll('-', '')}`,
            displayName: field(data, 'displayName') });
        }}>
          <label htmlFor="profile-create-name">Profile name</label>
          <input id="profile-create-name" name="displayName" minLength={1} maxLength={80} required disabled={!enabled} placeholder="For example, Sales mailbox" />
          <details><summary>Advanced: custom identifier</summary>
            <label htmlFor="profile-create-id">Profile ID</label>
            <input id="profile-create-id" name="profileId" placeholder="Generated automatically" disabled={!enabled} />
          </details>
          <button type="submit" disabled={!enabled || create.isPending}>Create</button>
        </form>
        <StatusMessage state={create.error ?? (create.data ? `${create.data.resultCode}: ${create.data.resourceId}` : null)} />
      </section>}

      <details className="panel full-span"><summary>Administration and diagnostics</summary>
      <section className="panel">
        <span className="eyebrow">Explicit profile ACL</span>
        <h2>Profile grant</h2>
        <ProfileGrantPanel
          disabled={!profileLoaded}
          defaultVersion={profile?.metadataVersion ?? 1}
          busy={grant.isPending}
          onGrant={(draft) => grant.mutate({ ...draft, revoke: false })}
          onRevoke={(draft) => grant.mutateAsync({ ...draft, revoke: true }).then(() => undefined)}
        />
        <StatusMessage state={grant.error ?? (grant.data ? `${grant.data.resultCode}: ${grant.data.resourceId}` : null)} />
      </section>

      <section className="panel full-span">
        <span className="eyebrow">Immutable generation registry</span>
        <h2>Generation</h2>
        <div className="split-grid">
          <form className="stack-form" onSubmit={(event) => {
            event.preventDefault();
            const id = field(new FormData(event.currentTarget), 'generationId');
            setGenerationId(id);
            setGeneration(null);
            generationLookup.mutate({ tenantId, profileId, generationId: id });
          }}>
            <label>Generation ID<input name="generationId" placeholder="generation_..." required disabled={!profileLoaded} /></label>
            <button type="submit" disabled={!profileLoaded || generationLookup.isPending}>Lookup generation</button>
          </form>
          <form className="stack-form" onSubmit={(event) => {
            event.preventDefault();
            const data = new FormData(event.currentTarget);
            const id = field(data, 'generationId');
            setGenerationId(id);
            generationRegister.mutate({
              generationId: id,
              objectKey: field(data, 'objectKey'),
              metadataDigest: field(data, 'metadataDigest'),
              containerDigest: field(data, 'containerDigest'),
            });
          }}>
            <label>Generation ID<input name="generationId" required disabled={!profileLoaded} /></label>
            <label>Object key<input name="objectKey" required disabled={!profileLoaded} /></label>
            <label>Metadata digest<input name="metadataDigest" minLength={64} maxLength={64} required disabled={!profileLoaded} /></label>
            <label>Container digest<input name="containerDigest" minLength={64} maxLength={64} required disabled={!profileLoaded} /></label>
            <button type="submit" disabled={!profileLoaded || generationRegister.isPending}>Register generation</button>
          </form>
        </div>
        <StatusMessage state={generationLookup.error ?? generationRegister.error ?? (generationRegister.data ? `${generationRegister.data.resultCode}: ${generationRegister.data.resourceId}` : null)} />
        {generation && <GenerationActions generation={generation} profileVersion={profile?.version ?? 1} busy={generationAction.isPending} onAction={(input) => generationAction.mutate({ ...input, target: { tenantId, profileId, generationId } })} onHighImpact={(input) => generationAction.mutateAsync({ ...input, target: { tenantId, profileId, generationId } }).then(() => undefined)} />}
        <StatusMessage state={actionBelongsToSelection
          ? (generationAction.error ?? (generationAction.data ? `${generationAction.data.resultCode}: ${generationAction.data.resourceId}` : null))
          : null} />
        {generationRefreshUnconfirmed && <div role="alert">
          The action completed, but the latest profile status could not be confirmed. Do not repeat the action.
          <button type="button" onClick={() => { void refreshGenerationProjection(); }}>Refresh current status</button>
        </div>}
      </section>

      <section className="panel full-span">
        <span className="eyebrow">Durable Object lease projection</span>
        <h2>Profile coordinator</h2>
        <button type="button" disabled={!profileLoaded || coordinatorLookup.isPending} onClick={() => coordinatorLookup.mutate({ tenantId, profileId })}>Load coordinator snapshot</button>
        <StatusMessage state={coordinatorLookup.error ?? coordinatorCommand.error ?? (coordinatorLookup.isPending ? 'Loading coordinator…' : null)} />
        {coordinator && (
          <>
            <dl className="projection horizontal">
              <div><dt>Status</dt><dd>{coordinator.projection.status}</dd></div>
              <div><dt>Version</dt><dd>{coordinator.version}</dd></div>
              <div><dt>Sequence</dt><dd>{coordinator.sequence}</dd></div>
              <div><dt>Active session</dt><dd>{coordinator.projection.active_session_id ?? 'None'}</dd></div>
              <div><dt>Active device</dt><dd>{coordinator.projection.active_device_id ?? 'None'}</dd></div>
            </dl>
            <div className="action-row">
              <button type="button" disabled={coordinatorCommand.isPending} onClick={() => coordinatorCommand.mutate({ type: 'begin_drain' })}>Begin drain</button>
              <ConfirmAction
                label="mark coordinator recovered"
                consequence="Recovery clears an uncertain/dirty coordinator state and is owner-only. Confirm only after external recovery evidence is complete."
                disabled={coordinatorCommand.isPending}
                onConfirm={() => coordinatorCommand.mutateAsync({ type: 'mark_recovered' }).then(() => undefined)}
              />
            </div>
          </>
        )}
      </section>
      </details>
    </div>
  );
}

function ProfileGrantPanel({ disabled, defaultVersion, busy, onGrant, onRevoke }: {
  disabled: boolean;
  defaultVersion: number;
  busy: boolean;
  onGrant: (input: GrantDraft) => void;
  onRevoke: (input: GrantDraft) => Promise<void>;
}) {
  const [actorId, setActorId] = useState('');
  const [role, setRole] = useState<'PROFILE_VIEWER' | 'PROFILE_OPERATOR'>('PROFILE_VIEWER');
  const [reason, setReason] = useState('');
  const [expectedMetadataVersion, setExpectedVersion] = useState(defaultVersion);

  useEffect(() => {
    setExpectedVersion(defaultVersion);
  }, [defaultVersion]);

  const draft = { actorId, role, reason, expectedMetadataVersion };
  const unavailable = disabled || busy || !actorId || !reason;
  return (
    <div className="action-grid">
      <label>Actor ID<input value={actorId} onChange={(e) => setActorId(e.currentTarget.value)} disabled={disabled} /></label>
      <label>Role<select value={role} onChange={(e) => setRole(e.currentTarget.value as typeof role)} disabled={disabled}><option value="PROFILE_VIEWER">Viewer</option><option value="PROFILE_OPERATOR">Operator</option></select></label>
      <label>Expected metadata version<input type="number" min="1" value={expectedMetadataVersion} onChange={(e) => setExpectedVersion(Number(e.currentTarget.value))} disabled={disabled} /></label>
      <label className="wide">Reason<input value={reason} onChange={(e) => setReason(e.currentTarget.value)} disabled={disabled} /></label>
      <button type="button" disabled={unavailable} onClick={() => onGrant(draft)}>Apply grant</button>
      <ConfirmAction label="revoke profile grant" consequence="This removes explicit profile access. Client assignment is unaffected." disabled={unavailable} onConfirm={() => onRevoke(draft)} />
    </div>
  );
}

function GenerationActions({ generation, profileVersion, busy, onAction, onHighImpact }: {
  generation: GenerationProjection;
  profileVersion: number;
  busy: boolean;
  onAction: (input: { kind: 'verify'; expectedVersion: number; reference: string } | { kind: 'activate' | 'deactivate' | 'quarantine'; expectedVersion: number }) => void;
  onHighImpact: (input: { kind: 'activate' | 'deactivate' | 'quarantine'; expectedVersion: number }) => Promise<void>;
}) {
  const [reference, setReference] = useState('');
  return (
    <div className="generation-card">
      <dl className="projection horizontal">
        <div><dt>ID</dt><dd>{generation.generationId}</dd></div>
        <div><dt>Status</dt><dd>{generation.status}</dd></div>
        <div><dt>Version</dt><dd>{generation.version}</dd></div>
        <div><dt>Verification</dt><dd>{generation.verificationReference ?? 'Not verified'}</dd></div>
      </dl>
      <div className="action-row">
        <label>Verification reference<input value={reference} onChange={(e) => setReference(e.currentTarget.value)} /></label>
        <button type="button" disabled={busy || generation.status !== 'REGISTERED' || !reference} onClick={() => onAction({ kind: 'verify', expectedVersion: generation.version, reference })}>Verify</button>
        <ConfirmAction label="activate generation" consequence="Activation changes the profile's active immutable generation. The Worker enforces verified status and optimistic versioning." disabled={busy || generation.status !== 'VERIFIED'} onConfirm={() => onHighImpact({ kind: 'activate', expectedVersion: profileVersion })} />
        <ConfirmAction label="deactivate generation" consequence="Deactivation removes the active generation from the profile and can make it non-launchable." disabled={busy || generation.status !== 'VERIFIED'} onConfirm={() => onHighImpact({ kind: 'deactivate', expectedVersion: profileVersion })} />
        <ConfirmAction label="quarantine generation" consequence="Quarantine is terminal for this generation and blocks future activation." disabled={busy || generation.status === 'QUARANTINED'} onConfirm={() => onHighImpact({ kind: 'quarantine', expectedVersion: generation.version })} />
      </div>
      {generation.status === 'QUARANTINED' && <p role="status">This generation is quarantined and cannot be activated. Select a verified generation instead.</p>}
    </div>
  );
}
