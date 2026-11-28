import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, type FormEvent } from 'react';
import { useCapabilities } from '../../app/CapabilityContext';
import { useTenant } from '../../app/TenantContext';
import { createClient, getClient, getClientHistory, listClients } from './api';
import { StatusMessage } from '../../shared/ui/StatusMessage';
import { ClientGrantPanel } from './ClientGrantPanel';
import { ClientHistoryPanel } from './ClientHistoryPanel';
import { ClientMailPanel } from './ClientMailPanel';
import { ClientMutationPanels } from './ClientMutationPanels';
import { ClientProfilesPanel } from './ClientProfilesPanel';
import { ClientRegistryList } from './ClientRegistryList';
import { useLogicalCommandMutation } from '../../shared/ui/useLogicalCommandMutation';
import { StatusBadge } from '../../shared/ui/StatusBadge';

function field(form: FormData, name: string): string {
  return String(form.get(name) ?? '').trim();
}

export function ClientsWorkspace({
  selectedClientId = null,
  onClientSelected,
}: {
  selectedClientId?: string | null;
  onClientSelected: (clientId: string) => void;
}) {
  const { tenantId, contexts } = useTenant();
  const actorId = contexts.find((context) => context.tenantId === tenantId)?.actorId ?? null;
  const { enabled } = useCapabilities();
  const queryClient = useQueryClient();
  const createDetails = useRef<HTMLDetailsElement>(null);
  const requireTenant = tenantId.length > 0;

  const list = useQuery({
    queryKey: ['client-registry', tenantId],
    queryFn: ({ signal }) => listClients(tenantId, signal),
    enabled: requireTenant,
  });
  const detail = useQuery({
    queryKey: ['client-registry', tenantId, selectedClientId, 'detail'],
    queryFn: () => getClient(tenantId, selectedClientId ?? ''),
    enabled: requireTenant && selectedClientId !== null,
  });
  const history = useQuery({
    queryKey: ['client-registry', tenantId, selectedClientId, 'history'],
    queryFn: ({ signal }) => getClientHistory(tenantId, selectedClientId ?? '', signal),
    enabled: requireTenant && selectedClientId !== null,
  });
  const create = useLogicalCommandMutation(
    (input: { clientId: string; kind: 'PERSON' | 'ORGANIZATION'; displayName: string }, idempotencyKey) =>
      createClient(tenantId, input, idempotencyKey),
    { onSuccess: async (receipt) => {
      await queryClient.invalidateQueries({ queryKey: ['client-registry', tenantId] });
      if (createDetails.current) createDetails.current.open = false;
      if (receipt?.resourceId) {
        onClientSelected(receipt.resourceId);
      }
    } },
  );

  const refreshSelected = async () => {
    await queryClient.invalidateQueries({ queryKey: ['client-registry', tenantId] });
  };

  const clients = list.data?.clients ?? [];
  const selectedClient = detail.data;

  return (
    <div className="workspace-grid">
      <ClientRegistryList
        clients={clients}
        selectedClientId={selectedClientId}
        onSelect={onClientSelected}
      />

      <section className="panel client-overview">
        <span className="eyebrow">Client overview</span>
        <h2>{selectedClient?.displayName ?? 'Choose a client'}</h2>
        <StatusMessage
          state={
            list.error
            ?? detail.error
            ?? history.error
            ?? (list.isPending ? 'Loading visible clients…' : null)
            ?? (selectedClientId && detail.isPending ? 'Loading client…' : null)
            ?? (selectedClientId && history.isPending ? 'Loading history…' : null)
          }
        />
        {!selectedClientId && <p>Select a client to see its browser profiles and details.</p>}
        {selectedClientId && !detail.isPending && !selectedClient && (
          <p>The selected resource is no longer visible. Refreshing the registry removes stale access.</p>
        )}
        {selectedClient && (
          <>
            <StatusBadge status={selectedClient.status} />
            <dl className="projection"><div><dt>Type</dt><dd>{selectedClient.kind === 'PERSON' ? 'Person' : 'Organization'}</dd></div></dl>
            <details className="relationship-tools">
              <summary>Technical details</summary>
              <dl className="projection"><div><dt>ID</dt><dd>{selectedClient.clientId}</dd></div><div><dt>Version</dt><dd>{selectedClient.version}</dd></div></dl>
            </details>
          </>
        )}
      </section>

      {selectedClient && (
        <>
          <ClientProfilesPanel
            key={`${tenantId}:${selectedClient.clientId}`}
            tenantId={tenantId}
            actorId={actorId}
            clientId={selectedClient.clientId}
            onMutated={refreshSelected}
          />
          <ClientMutationPanels
            key={`${selectedClient.clientId}:${selectedClient.version}`}
            tenantId={tenantId}
            client={selectedClient}
            onMutated={refreshSelected}
          />
          <ClientGrantPanel
            tenantId={tenantId}
            client={selectedClient}
            onMutated={refreshSelected}
          />
          {enabled('mailbox_read') ? (
            <ClientMailPanel
              clientId={selectedClient.clientId}
              outboundMailEnabled={enabled('outbound_mail')}
            />
          ) : null}
        </>
      )}

      <details id="create-client" ref={createDetails} className="panel full-span client-create-panel">
        <summary>+ Create a client</summary>
        <p>Name the person or organization. The technical identifier is generated automatically. New client access is granted only to its creator.</p>
        <form className="stack-form" onSubmit={(event: FormEvent<HTMLFormElement>) => {
          event.preventDefault();
          if (create.isPending || create.isError) return;
          const data = new FormData(event.currentTarget);
          const displayName = field(data, 'displayName');
          if (!displayName) return;
          create.mutate({
            clientId: `client_${globalThis.crypto.randomUUID().replaceAll('-', '')}`,
            kind: field(data, 'kind') as 'PERSON' | 'ORGANIZATION',
            displayName,
          });
        }}>
          <label htmlFor="client-name">Client name</label>
          <input id="client-name" name="displayName" maxLength={200} required disabled={!requireTenant || create.isPending || create.isError} placeholder="Name or organization" />
          <label htmlFor="client-kind">Type</label>
          <select id="client-kind" name="kind" defaultValue="PERSON" disabled={!requireTenant || create.isPending || create.isError}>
            <option value="PERSON">Person</option><option value="ORGANIZATION">Organization</option>
          </select>
          <button type="submit" disabled={!requireTenant || create.isPending || create.isError}>Create client</button>
        </form>
        {create.isError && <div role="alert">
          <p>Creation was not confirmed. To avoid duplicates, retry the original request with the same identifier.</p>
          <button type="button" disabled={create.isPending} onClick={() => { void create.retryLatestCommand().catch(() => undefined); }}>Retry client creation</button>
        </div>}
        <StatusMessage state={create.isError ? null : (create.data ? 'Client created.' : null)} />
      </details>

      <ClientHistoryPanel history={selectedClient ? history.data : undefined} />
    </div>
  );
}
