import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';
import { useQuery } from '@tanstack/react-query';
import { getTenantContexts, type TenantContextsProjection } from '../features/session/api';

interface TenantContextValue {
  tenantId: string;
  contexts: TenantContextsProjection['tenants'];
  state: 'loading' | 'error' | 'empty' | 'selecting' | 'ready';
  selectTenant: (tenantId: string) => void;
  retry: () => void;
}

type TenantContextsLoader = (signal?: AbortSignal) => Promise<TenantContextsProjection>;

const TenantContext = createContext<TenantContextValue | null>(null);

function tenantFromUrl(): string {
  return new URLSearchParams(window.location.search).get('tenant')?.trim() ?? '';
}

export function TenantProvider({
  children,
  loadTenantContexts = getTenantContexts,
}: {
  children: ReactNode;
  loadTenantContexts?: TenantContextsLoader;
}) {
  const [tenantId, setTenantState] = useState('');
  const query = useQuery({
    queryKey: ['authenticated-tenant-contexts'],
    queryFn: ({ signal }) => loadTenantContexts(signal),
    retry: false,
  });
  const contexts = query.data?.tenants ?? [];
  const state: TenantContextValue['state'] = query.isPending ? 'loading' : query.error ? 'error' : contexts.length === 0 ? 'empty' : tenantId ? 'ready' : 'selecting';

  useEffect(() => {
    if (!query.data) return;
    const requested = tenantFromUrl();
    const allowed = contexts.some((context) => context.tenantId === requested);
    const next = allowed ? requested : contexts.length === 1 ? (contexts[0]?.tenantId ?? '') : '';
    setTenantState(next);
    const url = new URL(window.location.href);
    if (next) url.searchParams.set('tenant', next);
    else url.searchParams.delete('tenant');
    window.history.replaceState(null, '', url);
  }, [query.data, contexts]);

  const value = useMemo<TenantContextValue>(() => ({
    tenantId,
    contexts,
    state,
    selectTenant: (next) => {
      if (!contexts.some((context) => context.tenantId === next)) return;
      const url = new URL(window.location.href);
      url.searchParams.set('tenant', next);
      window.history.replaceState(null, '', url);
      setTenantState(next);
    },
    retry: () => { void query.refetch(); },
  }), [tenantId, contexts, state, query]);
  return <TenantContext.Provider value={value}>{children}</TenantContext.Provider>;
}

export function useTenant(): TenantContextValue {
  const value = useContext(TenantContext);
  if (value === null) throw new Error('TenantProvider is missing');
  return value;
}

export function TenantChooser() {
  const { tenantId, contexts, state, selectTenant, retry } = useTenant();
  if (state === 'loading') return <p className="tenant-chooser" role="status">Resolving your organizations…</p>;
  if (state === 'error') return <div className="tenant-chooser" role="alert">Unable to resolve your organizations. <button type="button" onClick={retry}>Retry</button></div>;
  if (state === 'empty') return <p className="tenant-chooser" role="status">No organization access is active for this identity.</p>;
  if (contexts.length === 1) return <p className="tenant-chooser" role="status">Organization: {contexts[0]?.displayName ?? ''}</p>;
  return (
    <label className="tenant-chooser" htmlFor="tenant-context">Organization
      <select id="tenant-context" value={tenantId} onChange={(event) => selectTenant(event.currentTarget.value)}>
        <option value="" disabled>Select an organization</option>
        {contexts.map((context) => <option key={context.tenantId} value={context.tenantId}>{context.displayName}</option>)}
      </select>
    </label>
  );
}
