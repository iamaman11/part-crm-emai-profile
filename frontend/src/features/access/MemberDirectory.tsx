import { useQuery } from '@tanstack/react-query';
import { useTenant } from '../../app/TenantContext';
import { listMembers } from '../../shared/api/endpoints';
import { StatusMessage } from '../../shared/ui/StatusMessage';

export function MemberDirectory() {
  const { tenantId } = useTenant();
  const query = useQuery({
    queryKey: ['operator-query', tenantId, 'members'],
    queryFn: ({ signal }) => listMembers(tenantId, signal),
    enabled: tenantId.length > 0,
  });
  const members = query.data?.members ?? [];

  return (
    <section className="panel full-span" aria-labelledby="member-directory-title">
      <span className="eyebrow">Live tenant membership</span>
      <h2 id="member-directory-title">Users & access directory</h2>
      <StatusMessage state={query.error ?? (query.isPending ? 'Loading tenant members…' : null)} />
      {!query.isPending && !query.error && members.length === 0 && (
        <p>No members are visible for the active tenant.</p>
      )}
      {members.length > 0 && (
        <div className="table-scroll">
          <table>
            <thead>
              <tr>
                <th scope="col">Actor</th>
                <th scope="col">Role</th>
                <th scope="col">Status</th>
              </tr>
            </thead>
            <tbody>
              {members.map((member) => (
                <tr key={member.actorId}>
                  <td><code>{member.actorId}</code></td>
                  <td>{member.role}</td>
                  <td>{member.status}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {query.data?.nextCursor && (
        <p className="muted">More members are available through the bounded server cursor.</p>
      )}
    </section>
  );
}
