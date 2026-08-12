import { useMutation } from '@tanstack/react-query';
import { useState, type FormEvent } from 'react';
import { useTenant } from '../../app/TenantContext';
import {
  changeMailboxClientAssociation,
  createMailboxBinding,
  createMailboxJob,
  getMailboxBinding,
  getMailboxClientAssociation,
  getMailboxJob,
  revokeMailboxBinding,
  runMailboxJob,
} from './api';
import type {
  MailboxBindingProjection,
  MailboxClientAssociationProjection,
  MailboxJobProjection,
} from './api';
import { ConfirmAction } from '../../shared/ui/ConfirmAction';
import { StatusMessage } from '../../shared/ui/StatusMessage';

function field(data: FormData, name: string): string {
  return String(data.get(name) ?? '').trim();
}

export function MailboxesWorkspace() {
  const { tenantId } = useTenant();
  const [bindingId, setBindingId] = useState('');
  const [binding, setBinding] = useState<MailboxBindingProjection | null>(null);
  const [association, setAssociation] = useState<MailboxClientAssociationProjection | null>(null);
  const [jobId, setJobId] = useState('');
  const [job, setJob] = useState<MailboxJobProjection | null>(null);

  const associationLookup = useMutation({
    mutationFn: (id: string) => getMailboxClientAssociation(tenantId, id),
    onSuccess: (data) => setAssociation(data ?? null),
  });
  const bindingLookup = useMutation({
    mutationFn: (id: string) => getMailboxBinding(tenantId, id),
    onSuccess: (data) => setBinding(data ?? null),
  });
  const bindingCreate = useMutation({
    mutationFn: (input: { bindingId: string; provider: 'GMAIL_API' | 'IMAP' | 'BROWSER_FALLBACK'; secretHandle: string }) => createMailboxBinding(tenantId, input),
  });
  const bindingRevoke = useMutation({
    mutationFn: (version: number) => revokeMailboxBinding(tenantId, bindingId, version),
  });
  const associationChange = useMutation({
    mutationFn: (input: { bindingId: string; clientId: string | null; expectedRelationshipVersion: number }) =>
      changeMailboxClientAssociation(tenantId, input.bindingId, {
        clientId: input.clientId,
        expectedRelationshipVersion: input.expectedRelationshipVersion,
      }),
    onSuccess: (_data, variables) => associationLookup.mutate(variables.bindingId),
  });
  const jobLookup = useMutation({
    mutationFn: (input: { bindingId: string; jobId: string }) => getMailboxJob(tenantId, input.bindingId, input.jobId),
    onSuccess: (data) => setJob(data ?? null),
  });
  const jobCreate = useMutation({
    mutationFn: (input: { jobId: string; cursor: string | null; delayMs: number; maxAttempts: number }) => createMailboxJob(tenantId, bindingId, input),
  });
  const jobRun = useMutation({
    mutationFn: (version: number) => runMailboxJob(tenantId, bindingId, jobId, version),
  });

  const enabled = tenantId.length > 0;
  const bindingLoaded = enabled && bindingId.length > 0;

  const lookupBinding = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const id = field(new FormData(event.currentTarget), 'bindingId');
    setBindingId(id);
    setBinding(null);
    setAssociation(null);
    setJob(null);
    bindingLookup.mutate(id);
    associationLookup.mutate(id);
  };

  return (
    <div className="workspace-grid">
      <section className="panel">
        <span className="eyebrow">Owner-only metadata projection</span>
        <h2>Mailbox binding</h2>
        <form className="stack-form" onSubmit={lookupBinding}>
          <label>Binding ID<input name="bindingId" placeholder="mailbox_..." required disabled={!enabled} /></label>
          <button type="submit" disabled={!enabled || bindingLookup.isPending || associationLookup.isPending}>Lookup binding</button>
        </form>
        <StatusMessage state={bindingLookup.error ?? (bindingLookup.isPending ? 'Loading mailbox binding…' : null)} />
        {binding && (
          <>
            <dl className="projection">
              <div><dt>ID</dt><dd>{binding.bindingId}</dd></div>
              <div><dt>Provider</dt><dd>{binding.provider}</dd></div>
              <div><dt>Status</dt><dd>{binding.status}</dd></div>
              <div><dt>Version</dt><dd>{binding.version}</dd></div>
            </dl>
            <ConfirmAction
              label="revoke mailbox binding"
              consequence="Revocation disables this binding. The secret value is never returned to the browser."
              disabled={bindingRevoke.isPending || binding.status === 'REVOKED'}
              onConfirm={() => bindingRevoke.mutateAsync(binding.version).then(() => undefined)}
            />
          </>
        )}
        <StatusMessage state={bindingRevoke.error ?? (bindingRevoke.data ? `${bindingRevoke.data.resultCode}: ${bindingRevoke.data.resourceId}` : null)} />
      </section>

      <section className="panel">
        <span className="eyebrow">Explicit mailbox → Client relationship</span>
        <h2>Client association</h2>
        <p>Association controls which Client this mailbox belongs to. It does not grant Client access and profile assignment is not used as authorization.</p>
        <StatusMessage state={associationLookup.error ?? (associationLookup.isPending ? 'Loading Client association…' : null)} />
        {association && (
          <>
            <dl className="projection">
              <div><dt>Client</dt><dd>{association.clientId ?? 'Unassigned'}</dd></div>
              <div><dt>Relationship version</dt><dd>{association.relationshipVersion}</dd></div>
              <div><dt>Mailbox executable</dt><dd>{association.mailboxExecutable ? 'Yes' : 'No'}</dd></div>
              <div><dt>Can manage</dt><dd>{association.canManage ? 'Yes' : 'No'}</dd></div>
            </dl>
            <form className="stack-form" onSubmit={(event) => {
              event.preventDefault();
              const clientId = field(new FormData(event.currentTarget), 'clientId');
              associationChange.mutate({
                bindingId: association.bindingId,
                clientId,
                expectedRelationshipVersion: association.relationshipVersion,
              });
            }}>
              <label>Client ID<input name="clientId" placeholder="client_..." required disabled={!association.canManage || !association.mailboxExecutable || associationChange.isPending} /></label>
              <button type="submit" disabled={!association.canManage || !association.mailboxExecutable || associationChange.isPending}>
                {association.clientId === null ? 'Bind Client' : 'Rebind Client'}
              </button>
            </form>
            {association.clientId !== null && (
              <ConfirmAction
                label="unbind Client"
                consequence="The mailbox will become unassigned and immediately ineligible for Client Mail until explicitly bound again. Client grants are unchanged."
                disabled={!association.canManage || !association.mailboxExecutable || associationChange.isPending}
                onConfirm={() => associationChange.mutateAsync({
                  bindingId: association.bindingId,
                  clientId: null,
                  expectedRelationshipVersion: association.relationshipVersion,
                }).then(() => undefined)}
              />
            )}
          </>
        )}
        <StatusMessage state={associationChange.error ?? (associationChange.data ? `${associationChange.data.resultCode}: relationship v${associationChange.data.relationshipVersion}` : null)} />
      </section>

      <section className="panel">
        <span className="eyebrow">Secret-handle boundary</span>
        <h2>Create binding</h2>
        <p>Only an opaque secret handle is sent. Raw mailbox passwords/tokens are outside this UI contract.</p>
        <form className="stack-form" onSubmit={(event) => {
          event.preventDefault();
          const data = new FormData(event.currentTarget);
          const id = field(data, 'bindingId');
          setBindingId(id);
          bindingCreate.mutate({
            bindingId: id,
            provider: field(data, 'provider') as 'GMAIL_API' | 'IMAP' | 'BROWSER_FALLBACK',
            secretHandle: field(data, 'secretHandle'),
          });
        }}>
          <label>Binding ID<input name="bindingId" required disabled={!enabled} /></label>
          <label>Provider<select name="provider" defaultValue="IMAP" disabled={!enabled}><option value="IMAP">IMAP</option><option value="GMAIL_API">Gmail API</option><option value="BROWSER_FALLBACK">Browser fallback</option></select></label>
          <label>Secret handle<input name="secretHandle" autoComplete="off" required disabled={!enabled} /></label>
          <button type="submit" disabled={!enabled || bindingCreate.isPending}>Create binding</button>
        </form>
        <StatusMessage state={bindingCreate.error ?? (bindingCreate.data ? `${bindingCreate.data.resultCode}: ${bindingCreate.data.resourceId}` : null)} />
      </section>

      <section className="panel full-span">
        <span className="eyebrow">Bounded metadata-only execution</span>
        <h2>Mailbox job</h2>
        <div className="split-grid">
          <form className="stack-form" onSubmit={(event) => {
            event.preventDefault();
            const data = new FormData(event.currentTarget);
            const binding = field(data, 'bindingId');
            const jobValue = field(data, 'jobId');
            setBindingId(binding);
            setJobId(jobValue);
            setJob(null);
            jobLookup.mutate({ bindingId: binding, jobId: jobValue });
          }}>
            <label>Binding ID<input name="bindingId" defaultValue={bindingId} required disabled={!enabled} /></label>
            <label>Job ID<input name="jobId" required disabled={!enabled} /></label>
            <button type="submit" disabled={!enabled || jobLookup.isPending}>Lookup job</button>
          </form>
          <form className="stack-form" onSubmit={(event) => {
            event.preventDefault();
            const data = new FormData(event.currentTarget);
            const nextJobId = field(data, 'jobId');
            setJobId(nextJobId);
            jobCreate.mutate({
              jobId: nextJobId,
              cursor: field(data, 'cursor') || null,
              delayMs: Number(field(data, 'delayMs')),
              maxAttempts: Number(field(data, 'maxAttempts')),
            });
          }}>
            <label>Job ID<input name="jobId" required disabled={!bindingLoaded} /></label>
            <label>Cursor (optional)<input name="cursor" maxLength={512} disabled={!bindingLoaded} /></label>
            <label>Delay ms<input name="delayMs" type="number" min="0" max="604800000" defaultValue="0" required disabled={!bindingLoaded} /></label>
            <label>Max attempts<input name="maxAttempts" type="number" min="1" max="10" defaultValue="3" required disabled={!bindingLoaded} /></label>
            <button type="submit" disabled={!bindingLoaded || jobCreate.isPending}>Create job</button>
          </form>
        </div>
        <StatusMessage state={jobLookup.error ?? jobCreate.error ?? (jobLookup.isPending ? 'Loading mailbox job…' : null)} />
        {job && (
          <div className="generation-card">
            <dl className="projection horizontal">
              <div><dt>ID</dt><dd>{job.jobId}</dd></div>
              <div><dt>Status</dt><dd>{job.status}</dd></div>
              <div><dt>Attempt</dt><dd>{job.attempt}/{job.maxAttempts}</dd></div>
              <div><dt>Provider status</dt><dd>{job.providerStatus ?? 'None'}</dd></div>
              <div><dt>Bounded count</dt><dd>{job.boundedItemCount}</dd></div>
              <div><dt>Version</dt><dd>{job.version}</dd></div>
            </dl>
            <ConfirmAction
              label="run mailbox job"
              consequence="The current repository-local provider lane is synthetic metadata-only. Running advances the governed job state; it does not expose message payloads."
              disabled={jobRun.isPending}
              onConfirm={() => jobRun.mutateAsync(job.version).then(() => undefined)}
            />
          </div>
        )}
        <StatusMessage state={jobRun.error ?? (jobRun.data ? `${jobRun.data.resultCode}: ${jobRun.data.resourceId}` : jobCreate.data ? `${jobCreate.data.resultCode}: ${jobCreate.data.resourceId}` : null)} />
      </section>
    </div>
  );
}
