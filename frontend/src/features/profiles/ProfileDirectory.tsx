import { useQuery } from '@tanstack/react-query';
import { useTenant } from '../../app/TenantContext';
import { listProfiles } from './api';
import { StatusMessage } from '../../shared/ui/StatusMessage';

export function ProfileDirectory({
  selectedProfileId,
  onSelect,
}: {
  selectedProfileId?: string | null;
  onSelect: (profileId: string) => void;
}) {
  const { tenantId } = useTenant();
  const query = useQuery({
    queryKey: ['operator-query', tenantId, 'profiles'],
    queryFn: ({ signal }) => listProfiles(tenantId, signal),
    enabled: tenantId.length > 0,
  });
  const profiles = query.data?.profiles ?? [];

  return (
    <section className="panel full-span" aria-labelledby="profile-directory-title">
      <span className="eyebrow">Authorized read model</span>
      <h2 id="profile-directory-title">Visible profiles</h2>
      <StatusMessage state={query.error ?? (query.isPending ? 'Loading visible profiles…' : null)} />
      {!query.isPending && !query.error && profiles.length === 0 && (
        <p>No profiles are visible to the active actor in this tenant.</p>
      )}
      {profiles.length > 0 && (
        <div className="table-scroll">
          <table>
            <thead>
              <tr>
                <th scope="col">Profile</th>
                <th scope="col">Status</th>
                <th scope="col">Client</th>
                <th scope="col">Generation</th>
                <th scope="col">Version</th>
                <th scope="col">Action</th>
              </tr>
            </thead>
            <tbody>
              {profiles.map((profile) => (
                <tr key={profile.profileId} aria-current={profile.profileId === selectedProfileId ? 'true' : undefined}>
                  <td><code>{profile.profileId}</code></td>
                  <td>{profile.status}</td>
                  <td>{profile.linkedClientId ?? 'Unassigned'}</td>
                  <td>{profile.activeGenerationId ?? 'None'}</td>
                  <td>{profile.version}</td>
                  <td>
                    <button type="button" onClick={() => onSelect(profile.profileId)}>
                      {profile.profileId === selectedProfileId ? 'Opened' : 'Open'}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {query.data?.nextCursor && (
        <p className="muted">More profiles are available. Phase 2H pagination controls remain bounded by the server cursor.</p>
      )}
    </section>
  );
}
