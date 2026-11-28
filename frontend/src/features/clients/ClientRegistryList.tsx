import type { ClientProjection } from './api';
import { useState } from 'react';
import { StatusBadge } from '../../shared/ui/StatusBadge';

export function ClientRegistryList({
  clients,
  selectedClientId,
  onSelect,
}: {
  clients: ReadonlyArray<ClientProjection>;
  selectedClientId: string | null;
  onSelect: (clientId: string) => void;
}) {
  const [search, setSearch] = useState('');
  const [showArchived, setShowArchived] = useState(false);
  const term = search.trim().toLocaleLowerCase();
  const visible = clients
    .filter((client) => (showArchived || client.status === 'ACTIVE' || client.clientId === selectedClientId) && (
      client.displayName.toLocaleLowerCase().includes(term)
      || client.clientId.toLocaleLowerCase().includes(term)
    ))
    .sort((a, b) => a.displayName.localeCompare(b.displayName));
  const archivedCount = clients.filter((client) => client.status !== 'ACTIVE').length;

  return (
    <section className="panel">
      <span className="eyebrow">Your workspace</span>
      <h2>Clients</h2>
      <div className="list-toolbar">
        <label className="search-field">Search clients
          <input type="search" value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Name or ID" />
        </label>
      </div>
      {archivedCount > 0 && <label className="muted">
        <input type="checkbox" checked={showArchived} onChange={(event) => setShowArchived(event.target.checked)} />
        Show archived ({archivedCount})
      </label>}
      {clients.length === 0 ? (
        <p>No clients are available in this organization.</p>
      ) : visible.length === 0 ? (
        <p>No clients match this search or filter.</p>
      ) : (
        <div className="client-registry-list">
          {visible.map((client) => (
            <button
              className="client-picker"
              key={client.clientId}
              type="button"
              aria-pressed={selectedClientId === client.clientId}
              onClick={() => onSelect(client.clientId)}
            >
              <span><span className="client-name">{client.displayName}</span><span className="client-kind">{client.kind === 'PERSON' ? 'Person' : 'Organization'}</span></span>
              <StatusBadge status={client.status} />
            </button>
          ))}
        </div>
      )}
      <p className="muted"><a href="#create-client">+ Create client</a></p>
    </section>
  );
}
