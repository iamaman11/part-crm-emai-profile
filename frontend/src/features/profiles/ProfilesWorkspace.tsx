import { useMutation } from '@tanstack/react-query';
import { useEffect, useState, type FormEvent } from 'react';
import { useTenant } from '../../app/TenantContext';
import {
  assignProfile,
  changeGenerationActivation,
  commandCoordinator,
  createProfile,
  getCoordinator,
  getGeneration,
  getProfile,
  quarantineGeneration,
  registerGeneration,
  setProfileGrant,
  verifyGeneration,
} from './api';
import { newIdempotencyKey } from '../../shared/api/client';
import type { CoordinatorResponse, GenerationProjection, ProfileProjection } from './api';
import { ConfirmAction } from '../../shared/ui/ConfirmAction';
import { StatusMessage } from '../../shared/ui/StatusMessage';

function field(data: FormData, name: string): string {
  return String(data.get(name) ?? '').trim();
}

interface GrantDraft {
  actorId: string;
  role: 'PROFILE_VIEWER' | 'PROFILE_OPERATOR';
  reason: string;
  expectedProfileVersion: number;
}

export function ProfilesWorkspace({
  selectedProfileId = null,
  onProfileSelected,
}: {
  selectedProfileId?: string | null;
  onProfileSelected: (profileId: string) => void;
}) {
  const { tenantId } = useTenant();
  const [profileId, setProfileId] = useState(selectedProfileId ?? '');
  const [profile, setProfile] = useState<ProfileProjection | null>(null);
  const [generationId, setGenerationId] = useState('');
  const [generation, setGeneration] = useState<GenerationProjection | null>(null);
  const [coordinator, setCoordinator] = useState<CoordinatorResponse | null>(null);

  const lookup = useMutation({
    mutationFn: (id: string) => getProfile(tenantId, id),
    onSuccess: (data) => setProfile(data ?? null),
  });
  const create = useMutation({
    mutationFn: (id: string) => createProfile(tenantId, id),
    onSuccess: (receipt) => {
      if (receipt?.resourceId) onProfileSelected(receipt.resourceId);
    },
  });
  const assign = useMutation({
    mutationFn: (input: { assignmentId: string; clientId: string; reason: string; expectedProfileVersion: number }) =>
      assignProfile(tenantId, profileId, input),
  });
  const grant = useMutation({
    mutationFn: (input: GrantDraft & { revoke: boolean }) => setProfileGrant(
      tenantId,
      profileId,
      input.actorId,
      { role: input.role, reason: input.reason, expectedProfileVersion: input.expectedProfileVersion },
      input.revoke,
    ),
  });
  const generationLookup = useMutation({
    mutationFn: (id: string) => getGeneration(tenantId, profileId, id),
    onSuccess: (data) => setGeneration(data ?? null),
  });
  const generationRegister = useMutation({
    mutationFn: (input: { generationId: string; objectKey: string; metadataDigest: string; containerDigest: string }) => registerGeneration(tenantId, profileId, input),
  });
  const generationAction = useMutation({
    mutationFn: (input:
      | { kind: 'verify'; expectedVersion: number; reference: string }
      | { kind: 'activate' | 'deactivate'; expectedVersion: number }
      | { kind: 'quarantine'; expectedVersion: number }) => {
      if (input.kind === 'verify') {
        return verifyGeneration(tenantId, profileId, generationId, {
          expectedGenerationVersion: input.expectedVersion,
          verificationReference: input.reference,
        });
      }
      if (input.kind === 'quarantine') {
        return quarantineGeneration(tenantId, profileId, generationId, input.expectedVersion);
      }
      return changeGenerationActivation(tenantId, profileId, generationId, input.expectedVersion, input.kind === 'activate');
    },
  });
  const coordinatorLookup = useMutation({
    mutationFn: () => getCoordinator(tenantId, profileId),
    onSuccess: (data) => setCoordinator(data ?? null),
  });
  const coordinatorCommand = useMutation({
    mutationFn: (command: import('./api').CoordinatorCommandDto) => {
      if (!coordinator) throw new Error('Load the coordinator snapshot before issuing a command.');
      return commandCoordinator(tenantId, profileId, {
        idempotency_key: newIdempotencyKey(),
        sequence: coordinator.sequence + 1,
        expected_version: coordinator.version,
        command,
      });
    },
    onSuccess: (data) => setCoordinator(data ?? null),
  });

  useEffect(() => {
    const nextProfileId = selectedProfileId ?? '';
    setProfileId(nextProfileId);
    setProfile(null);
    setGenerationId('');
    setGeneration(null);
    setCoordinator(null);
    if (tenantId && nextProfileId) lookup.mutate(nextProfileId);
  }, [tenantId, selectedProfileId]);

  const enabled = tenantId.length > 0;
  const profileLoaded = enabled && profileId.length > 0 && profile !== null;

  return (
    <div className="workspace-grid">
      <section className="panel">
        <span className="eyebrow">Visible resource lookup</span>
        <h2>Profile</h2>
        <form
          className="stack-form"
          onSubmit={(event) => {
            event.preventDefault();
            onProfileSelected(field(new FormData(event.currentTarget), 'profileId'));
          }}
        >
          <label htmlFor="profile-lookup-id">Profile ID</label>
          <input id="profile-lookup-id" name="profileId" placeholder="profile_..." required disabled={!enabled} />
          <button type="submit" disabled={!enabled || lookup.isPending}>Open profile</button>
        </form>
        <StatusMessage state={lookup.error ?? (lookup.isPending ? 'Loading profile…' : null)} />
        {selectedProfileId && !lookup.isPending && !profile && !lookup.error && (
          <p>The selected profile is not visible to the active actor.</p>
        )}
        {profile && (
          <dl className="projection">
            <div><dt>ID</dt><dd>{profile.profileId}</dd></div>
            <div><dt>Status</dt><dd>{profile.status}</dd></div>
            <div><dt>Version</dt><dd>{profile.version}</dd></div>
            <div><dt>Client</dt><dd>{profile.linkedClientId ?? 'Not assigned'}</dd></div>
          </dl>
        )}
      </section>

      <section className="panel">
        <span className="eyebrow">Member self-service · explicit creator grant</span>
        <h2>Create profile</h2>
        <p>Active members may create a profile. Access is granted only to the creator; client assignment does not grant profile access.</p>
        <form className="stack-form" onSubmit={(event) => {
          event.preventDefault();
          create.mutate(field(new FormData(event.currentTarget), 'profileId'));
        }}>
          <label htmlFor="profile-create-id">Profile ID</label>
          <input id="profile-create-id" name="profileId" placeholder="profile_..." required disabled={!enabled} />
          <button type="submit" disabled={!enabled || create.isPending}>Create</button>
        </form>
        <StatusMessage state={create.error ?? (create.data ? `${create.data.resultCode}: ${create.data.resourceId}` : null)} />
      </section>

      <section className="panel">
        <span className="eyebrow">Business relationship, not authorization</span>
        <h2>Assign client</h2>
        <form className="stack-form" onSubmit={(event) => {
          event.preventDefault();
          const data = new FormData(event.currentTarget);
          assign.mutate({
            assignmentId: field(data, 'assignmentId'),
            clientId: field(data, 'clientId'),
            reason: field(data, 'reason'),
            expectedProfileVersion: Number(field(data, 'expectedVersion')),
          });
        }}>
          <label>Assignment ID<input name="assignmentId" required disabled={!profileLoaded} /></label>
          <label>Client ID<input name="clientId" required disabled={!profileLoaded} /></label>
          <label>Expected profile version<input key={profile?.version ?? 1} name="expectedVersion" type="number" min="1" defaultValue={profile?.version ?? 1} required disabled={!profileLoaded} /></label>
          <label>Reason<input name="reason" required disabled={!profileLoaded} /></label>
          <button type="submit" disabled={!profileLoaded || assign.isPending}>Assign</button>
        </form>
        <StatusMessage state={assign.error ?? (assign.data ? `${assign.data.resultCode}: ${assign.data.resourceId}` : null)} />
      </section>

      <section className="panel">
        <span className="eyebrow">Explicit profile ACL</span>
        <h2>Profile grant</h2>
        <ProfileGrantPanel
          disabled={!profileLoaded}
          defaultVersion={profile?.version ?? 1}
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
            generationLookup.mutate(id);
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
        {generation && <GenerationActions generation={generation} profileVersion={profile?.version ?? 1} busy={generationAction.isPending} onAction={(input) => generationAction.mutate(input)} onHighImpact={(input) => generationAction.mutateAsync(input).then(() => undefined)} />}
        <StatusMessage state={generationAction.error ?? (generationAction.data ? `${generationAction.data.resultCode}: ${generationAction.data.resourceId}` : null)} />
      </section>

      <section className="panel full-span">
        <span className="eyebrow">Durable Object lease projection</span>
        <h2>Profile coordinator</h2>
        <button type="button" disabled={!profileLoaded || coordinatorLookup.isPending} onClick={() => coordinatorLookup.mutate()}>Load coordinator snapshot</button>
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
  const [expectedProfileVersion, setExpectedVersion] = useState(defaultVersion);

  useEffect(() => {
    setExpectedVersion(defaultVersion);
  }, [defaultVersion]);

  const draft = { actorId, role, reason, expectedProfileVersion };
  const unavailable = disabled || busy || !actorId || !reason;
  return (
    <div className="action-grid">
      <label>Actor ID<input value={actorId} onChange={(e) => setActorId(e.currentTarget.value)} disabled={disabled} /></label>
      <label>Role<select value={role} onChange={(e) => setRole(e.currentTarget.value as typeof role)} disabled={disabled}><option value="PROFILE_VIEWER">Viewer</option><option value="PROFILE_OPERATOR">Operator</option></select></label>
      <label>Expected version<input type="number" min="1" value={expectedProfileVersion} onChange={(e) => setExpectedVersion(Number(e.currentTarget.value))} disabled={disabled} /></label>
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
        <button type="button" disabled={busy || !reference} onClick={() => onAction({ kind: 'verify', expectedVersion: generation.version, reference })}>Verify</button>
        <ConfirmAction label="activate generation" consequence="Activation changes the profile's active immutable generation. The Worker enforces verified status and optimistic versioning." disabled={busy} onConfirm={() => onHighImpact({ kind: 'activate', expectedVersion: profileVersion })} />
        <ConfirmAction label="deactivate generation" consequence="Deactivation removes the active generation from the profile and can make it non-launchable." disabled={busy} onConfirm={() => onHighImpact({ kind: 'deactivate', expectedVersion: profileVersion })} />
        <ConfirmAction label="quarantine generation" consequence="Quarantine is terminal for this generation and blocks future activation." disabled={busy} onConfirm={() => onHighImpact({ kind: 'quarantine', expectedVersion: generation.version })} />
      </div>
    </div>
  );
}
