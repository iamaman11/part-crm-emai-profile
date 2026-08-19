import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { type FormEvent } from 'react';
import { useCapabilities } from '../../app/CapabilityContext';
import { useTenant } from '../../app/TenantContext';
import { createClient, getClient, getClientHistory, listClients } from './api';
import { StatusMessage } from '../../shared/ui/StatusMessage';
import { ClientGrantPanel } from './ClientGrantPanel';
import { ClientHistoryPanel } from './ClientHistoryPanel';
import { ClientMailPanel } from './ClientMailPanel';
import { ClientMutationPanels } from './ClientMutationPanels';
import { ClientRegistryList } from './ClientRegistryList';

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
  const { tenantId } = useTenant();
  const { enabled } = useCapabilities();
  const queryClient = useQueryClient();
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
  const create = useMutation({
    mutationFn: (input: { clientId: string; kind: 'PERSON' | 'ORGANIZATION'; displayName: string }) =>
      createClient(tenantId, input),
    onSuccess: async (receipt) => {
      await queryClient.invalidateQueries({ queryKey: ['client-registry', tenantId] });
      if (receipt?.resourceId) {
        onClientSelected(receipt.resourceId);
      }
    },
  });

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

      <section className="panel">
        <span className="eyebrow">Member self-service · explicit creator grant</span>
        <h2>Create client</h2>
        <p>Active members may create a client. Access is granted only to the creator; this does not make the client tenant-wide.</p>
        <form
          className="stack-form"
          onSubmit={(event: FormEvent<HTMLFormElement>) => {
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
        <span className="eyebrow">Selected grant-filtered resource</span>
        <h2>Client detail</h2>
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
        {!selectedClientId && <p>Select a client from the live registry projection.</p>}
        {selectedClientId && !detail.isPending && !selectedClient && (
          <p>The selected resource is no longer visible. Refreshing the registry removes stale access.</p>
        )}
        {selectedClient && (
          <dl className="projection">
            <div><dt>ID</dt><dd>{selectedClient.clientId}</dd></div>
            <div><dt>Name</dt><dd>{selectedClient.displayName}</dd></div>
            <div><dt>Kind</dt><dd>{selectedClient.kind}</dd></div>
            <div><dt>Status</dt><dd>{selectedClient.status}</dd></div>
            <div><dt>Version</dt><dd>{selectedClient.version}</dd></div>
          </dl>
        )}
      </section>

      {selectedClient && (
        <>
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

      <ClientHistoryPanel history={selectedClient ? history.data : undefined} />
    </div>
  );
}
