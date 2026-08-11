import { useQuery } from '@tanstack/react-query';
import { getSession } from './api';
import { StatusMessage } from '../../shared/ui/StatusMessage';
import { useTenant } from '../../app/TenantContext';

export function SessionPanel() {
  const { tenantId } = useTenant();
  const query = useQuery({
    queryKey: ['session', tenantId],
    queryFn: ({ signal }) => getSession(tenantId, signal),
    enabled: tenantId.length > 0,
    retry: false,
    staleTime: 30_000,
  });

  if (!tenantId) return <StatusMessage state="Choose a tenant to resolve the authenticated session." />;
  if (query.isPending) return <StatusMessage state="Resolving authenticated session…" />;
  if (query.error) return <StatusMessage state={query.error} />;
  if (!query.data) return <StatusMessage state="Session response was empty." />;

  return (
    <section className="session-card" aria-label="Authenticated session">
      <div><span className="eyebrow">Authenticated actor</span><strong>{query.data.actorId}</strong></div>
      <div><span className="eyebrow">Role</span><strong>{query.data.role}</strong></div>
      <div><span className="eyebrow">Tenant</span><strong>{query.data.tenantId}</strong></div>
    </section>
  );
}
