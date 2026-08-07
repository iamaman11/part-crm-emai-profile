import { createContext, useContext, useMemo, useState, type FormEvent, type ReactNode } from 'react';

interface TenantContextValue {
  tenantId: string;
  setTenantId: (tenantId: string) => void;
}

const TenantContext = createContext<TenantContextValue | null>(null);

function tenantFromUrl(): string {
  return new URLSearchParams(window.location.search).get('tenant')?.trim() ?? '';
}

export function TenantProvider({ children }: { children: ReactNode }) {
  const [tenantId, setTenantState] = useState(tenantFromUrl);
  const value = useMemo<TenantContextValue>(() => ({
    tenantId,
    setTenantId: (next) => {
      const normalized = next.trim();
      const url = new URL(window.location.href);
      if (normalized) url.searchParams.set('tenant', normalized);
      else url.searchParams.delete('tenant');
      window.history.replaceState(null, '', url);
      setTenantState(normalized);
    },
  }), [tenantId]);
  return <TenantContext.Provider value={value}>{children}</TenantContext.Provider>;
}

export function useTenant(): TenantContextValue {
  const value = useContext(TenantContext);
  if (value === null) throw new Error('TenantProvider is missing');
  return value;
}

export function TenantChooser() {
  const { tenantId, setTenantId } = useTenant();
  const [draft, setDraft] = useState(tenantId);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    setTenantId(draft);
  };
  return (
    <form className="tenant-chooser" onSubmit={submit}>
      <label htmlFor="tenant-id">Tenant ID</label>
      <input
        id="tenant-id"
        name="tenantId"
        value={draft}
        onChange={(event) => setDraft(event.currentTarget.value)}
        autoComplete="off"
        placeholder="tenant_..."
        required
      />
      <button type="submit">Use tenant</button>
    </form>
  );
}
