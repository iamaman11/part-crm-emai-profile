import { useMutation } from '@tanstack/react-query';
import { useState, type FormEvent } from 'react';
import { useTenant } from '../../app/TenantContext';
import { createClient, getClient, setClientGrant } from '../../shared/api/endpoints';
import type { ClientProjection } from '../../shared/api/types';
import { ConfirmAction } from '../../shared/ui/ConfirmAction';
import { StatusMessage } from '../../shared/ui/StatusMessage';

function field(form: FormData, name: string): string {
  return String(form.get(name) ?? '').trim();
}

export function ClientsWorkspace() {
  const { tenantId } = useTenant();
  const [client, setClient] = useState<ClientProjection | null>(null);
  const [lookupId, setLookupId] = useState('');
  const lookup = useMutation({
    mutationFn: (clientId: string) => getClient(tenantId, clientId),
    onSuccess: (data) => setClient(data ?? null),
  });
  const create = useMutation({
    mutationFn: (input: { clientId: string; kind: 'PERSON' | 'ORGANIZATION'; displayName: string }) => createClient(tenantId, input),
  });
  const grant = useMutation({
    mutationFn: (input: { actorId: string; role: 'CLIENT_VIEWER' | 'CLIENT_EDITOR'; reason: string; expectedClientVersion: number; revoke: boolean }) =>
      setClientGrant(tenantId, lookupId, input.actorId, {
        role: input.role,
        reason: input.reason,
        expectedClientVersion: input.expectedClientVersion,
      }, input.revoke),
  });

  const requireTenant = tenantId.length > 0;
  const submitLookup = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!requireTenant) return;
    const id = field(new FormData(event.currentTarget), 'clientId');
    setLookupId(id);
    setClient(null);
    lookup.mutate(id);
  };

  return (
    <div className="workspace-grid">
      <section className="panel">
        <span className="eyebrow">Visible resource lookup</span>
        <h2>Client</h2>
        <form onSubmit={submitLookup} className="stack-form">
          <label htmlFor="client-lookup-id">Client ID</label>
          <input id="client-lookup-id" name="clientId" placeholder="client_..." required disabled={!requireTenant} />
          <button type="submit" disabled={!requireTenant || lookup.isPending}>Lookup client</button>
        </form>
        <StatusMessage state={lookup.error ?? (lookup.isPending ? 'Loading client…' : null)} />
        {client && (
          <dl className="projection">
            <div><dt>ID</dt><dd>{client.clientId}</dd></div>
            <div><dt>Name</dt><dd>{client.displayName}</dd></div>
            <div><dt>Kind</dt><dd>{client.kind}</dd></div>
            <div><dt>Status</dt><dd>{client.status}</dd></div>
            <div><dt>Version</dt><dd>{client.version}</dd></div>
          </dl>
        )}
      </section>

      <section className="panel">
        <span className="eyebrow">Owner-governed mutation</span>
        <h2>Create client</h2>
        <form
          className="stack-form"
          onSubmit={(event) => {
            event.preventDefault();
            const data = new FormData(event.currentTarget);
            create.mutate({
              clientId: field(data, 'clientId'),
              kind: field(data, 'kind') as 'PERSON' | 'ORGANIZATION',
              displayName: field(data, 'displayName'),
            });
          }}
        >
          <label htmlFor="client-create-id">Client ID</label>
          <input id="client-create-id" name="clientId" required placeholder="client_..." disabled={!requireTenant} />
          <label htmlFor="client-kind">Kind</label>
          <select id="client-kind" name="kind" defaultValue="PERSON" disabled={!requireTenant}>
            <option value="PERSON">Person</option>
            <option value="ORGANIZATION">Organization</option>
          </select>
          <label htmlFor="client-name">Display name</label>
          <input id="client-name" name="displayName" maxLength={200} required disabled={!requireTenant} />
          <button type="submit" disabled={!requireTenant || create.isPending}>Create</button>
        </form>
        <StatusMessage state={create.error ?? (create.data ? `${create.data.resultCode}: ${create.data.resourceId}` : null)} />
      </section>

      <section className="panel full-span">
        <span className="eyebrow">Explicit access; assignment never grants access</span>
        <h2>Client grant</h2>
        <p>Lookup a client first. The Worker remains the authorization boundary for grant and revoke.</p>
        <GrantForm
          disabled={!requireTenant || !lookupId}
          defaultVersion={client?.version ?? 1}
          busy={grant.isPending}
          onApply={(input) => grant.mutate(input)}
          onRevoke={(input) => grant.mutateAsync({ ...input, revoke: true }).then(() => undefined)}
        />
        <StatusMessage state={grant.error ?? (grant.data ? `${grant.data.resultCode}: ${grant.data.resourceId}` : null)} />
      </section>
    </div>
  );
}

function GrantForm({
  disabled,
  defaultVersion,
  busy,
  onApply,
  onRevoke,
}: {
  disabled: boolean;
  defaultVersion: number;
  busy: boolean;
  onApply: (input: { actorId: string; role: 'CLIENT_VIEWER' | 'CLIENT_EDITOR'; reason: string; expectedClientVersion: number; revoke: boolean }) => void;
  onRevoke: (input: { actorId: string; role: 'CLIENT_VIEWER' | 'CLIENT_EDITOR'; reason: string; expectedClientVersion: number; revoke: boolean }) => Promise<void>;
}) {
  const [input, setInput] = useState({ actorId: '', role: 'CLIENT_VIEWER' as const, reason: '', expectedClientVersion: defaultVersion, revoke: false });
  return (
    <div className="action-grid">
      <label>Actor ID<input value={input.actorId} onChange={(e) => setInput({ ...input, actorId: e.currentTarget.value })} disabled={disabled} /></label>
      <label>Role<select value={input.role} onChange={(e) => setInput({ ...input, role: e.currentTarget.value as 'CLIENT_VIEWER' | 'CLIENT_EDITOR' })} disabled={disabled}><option value="CLIENT_VIEWER">Viewer</option><option value="CLIENT_EDITOR">Editor</option></select></label>
      <label>Expected version<input type="number" min="1" value={input.expectedClientVersion} onChange={(e) => setInput({ ...input, expectedClientVersion: Number(e.currentTarget.value) })} disabled={disabled} /></label>
      <label className="wide">Reason<input value={input.reason} onChange={(e) => setInput({ ...input, reason: e.currentTarget.value })} disabled={disabled} /></label>
      <button type="button" disabled={disabled || busy || !input.actorId || !input.reason} onClick={() => onApply({ ...input, revoke: false })}>Apply grant</button>
      <ConfirmAction
        label="revoke client grant"
        consequence="This removes the explicit client grant. It does not change profile assignment or profile grants."
        disabled={disabled || busy || !input.actorId || !input.reason}
        onConfirm={() => onRevoke({ ...input, revoke: true })}
      />
    </div>
  );
}
