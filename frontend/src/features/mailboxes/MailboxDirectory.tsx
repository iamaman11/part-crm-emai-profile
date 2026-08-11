import { useQuery } from '@tanstack/react-query';
import { useTenant } from '../../app/TenantContext';
import { listMailboxes } from '../../shared/api/endpoints';
import { StatusMessage } from '../../shared/ui/StatusMessage';

export function MailboxDirectory() {
  const { tenantId } = useTenant();
  const query = useQuery({
    queryKey: ['operator-query', tenantId, 'mailboxes'],
    queryFn: ({ signal }) => listMailboxes(tenantId, signal),
    enabled: tenantId.length > 0,
  });
  const mailboxes = query.data?.mailboxes ?? [];

  return (
    <section className="panel full-span" aria-labelledby="mailbox-directory-title">
      <span className="eyebrow">Authorized metadata projection</span>
      <h2 id="mailbox-directory-title">Mailbox bindings</h2>
      <StatusMessage state={query.error ?? (query.isPending ? 'Loading mailbox bindings…' : null)} />
      {!query.isPending && !query.error && mailboxes.length === 0 && (
        <p>No mailbox bindings are visible to the active actor.</p>
      )}
      {mailboxes.length > 0 && (
        <div className="table-scroll">
          <table>
            <thead>
              <tr>
                <th scope="col">Binding</th>
                <th scope="col">Provider</th>
                <th scope="col">Status</th>
                <th scope="col">Version</th>
              </tr>
            </thead>
            <tbody>
              {mailboxes.map((mailbox) => (
                <tr key={mailbox.bindingId}>
                  <td><code>{mailbox.bindingId}</code></td>
                  <td>{mailbox.provider}</td>
                  <td>{mailbox.status}</td>
                  <td>{mailbox.version}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {query.data?.nextCursor && (
        <p className="muted">More mailbox bindings are available through the bounded server cursor.</p>
      )}
    </section>
  );
}
