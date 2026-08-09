import type { ClientProjection } from '../../shared/api/types';

export function ClientRegistryList({
  clients,
  selectedClientId,
  onSelect,
}: {
  clients: ReadonlyArray<ClientProjection>;
  selectedClientId: string | null;
  onSelect: (clientId: string) => void;
}) {
  return (
    <section className="panel">
      <span className="eyebrow">Live grant-filtered projection</span>
      <h2>Client Registry</h2>
      {clients.length === 0 ? (
        <p>No clients are visible to the active actor.</p>
      ) : (
        <div className="action-grid">
          {clients.map((client) => (
            <button
              key={client.clientId}
              type="button"
              aria-pressed={selectedClientId === client.clientId}
              onClick={() => onSelect(client.clientId)}
            >
              {client.displayName} · {client.kind} · {client.status} · v{client.version}
            </button>
          ))}
        </div>
      )}
    </section>
  );
}
