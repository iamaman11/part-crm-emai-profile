import { useQuery } from '@tanstack/react-query';
import { StatusMessage } from '../../shared/ui/StatusMessage';

async function probe(path: string, signal: AbortSignal): Promise<string> {
  const response = await fetch(path, {
    method: 'GET',
    credentials: 'same-origin',
    cache: 'no-store',
    signal,
    headers: { Accept: 'text/plain' },
  });
  if (!response.ok) {
    throw new Error(`Diagnostic probe failed with HTTP ${response.status}`);
  }
  return response.text();
}

export function SettingsWorkspace() {
  const health = useQuery({
    queryKey: ['settings', 'health'],
    queryFn: ({ signal }) => probe('/api/v1/health', signal),
  });
  const bindings = useQuery({
    queryKey: ['settings', 'bindings'],
    queryFn: ({ signal }) => probe('/api/v1/bindings', signal),
  });

  return (
    <div className="page-stack">
      <section className="hero panel">
        <span className="eyebrow">Safe operational diagnostics</span>
        <h2>Settings & environment</h2>
        <p>
          Only concrete repository-owned diagnostics are exposed here. Credentials, secret handles,
          provider tokens and raw environment values are deliberately absent.
        </p>
      </section>
      <section className="workspace-grid">
        <article className="panel">
          <h3>Control-plane health</h3>
          <StatusMessage state={health.error ?? (health.isPending ? 'Checking health…' : null)} />
          {health.data && <p><code>{health.data}</code></p>}
          <button type="button" onClick={() => void health.refetch()} disabled={health.isFetching}>
            Recheck health
          </button>
        </article>
        <article className="panel">
          <h3>Required bindings</h3>
          <StatusMessage state={bindings.error ?? (bindings.isPending ? 'Checking bindings…' : null)} />
          {bindings.data && <p><code>{bindings.data}</code></p>}
          <button type="button" onClick={() => void bindings.refetch()} disabled={bindings.isFetching}>
            Recheck bindings
          </button>
        </article>
      </section>
    </div>
  );
}
