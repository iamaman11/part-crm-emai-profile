import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
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
  const [showDeleted, setShowDeleted] = useState(false);
  const profiles = query.data?.profiles ?? [];
  const deletedCount = profiles.filter((profile) => profile.status === 'DELETED').length;
  const visibleProfiles = showDeleted ? profiles : profiles.filter((profile) => profile.status !== 'DELETED');

  if (!tenantId) {
    return <section className="panel full-span" aria-labelledby="profile-directory-title"><h2 id="profile-directory-title">Visible profiles</h2><p>Select an authorized organization before loading profiles.</p></section>;
  }

  return (
    <section className="panel full-span" aria-labelledby="profile-directory-title">
      <span className="eyebrow">Authorized read model</span>
      <h2 id="profile-directory-title">Browser profiles</h2>
      {deletedCount > 0 && (
        <button type="button" aria-pressed={showDeleted} onClick={() => setShowDeleted((value) => !value)}>
          {showDeleted ? 'Hide deleted profiles' : `Show deleted profiles (${deletedCount})`}
        </button>
      )}
      <StatusMessage state={query.error ?? (query.isPending ? 'Loading visible profiles…' : null)} />
      {!query.isPending && !query.error && visibleProfiles.length === 0 && (
        <p>{profiles.length === 0 ? 'No profiles are visible to the active actor in this tenant.' : 'No active profiles on this page. Show deleted profiles to see archived entries.'}</p>
      )}
      {visibleProfiles.length > 0 && (
        <div className="table-scroll">
          <table>
            <thead>
              <tr>
                <th scope="col">Profile</th>
                <th scope="col">Status</th>
                <th scope="col">Client</th>
                <th scope="col">Action</th>
              </tr>
            </thead>
            <tbody>
              {visibleProfiles.map((profile) => (
                <tr key={profile.profileId} aria-current={profile.profileId === selectedProfileId ? 'true' : undefined}>
                  <td><code>{profile.profileId}</code></td>
                  <td>{profile.status}</td>
                  <td>{profile.linkedClientId ?? 'Unassigned'}</td>
                  <td>
                    <button type="button" onClick={() => onSelect(profile.profileId)}>
                      {profile.profileId === selectedProfileId ? 'Selected' : 'View details'}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {query.data?.nextCursor && (
        <p className="muted">More profiles are available on the server.</p>
      )}
    </section>
  );
}
