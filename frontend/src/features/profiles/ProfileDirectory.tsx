import { useInfiniteQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { useTenant } from '../../app/TenantContext';
import { dispatchBrowserProfileOpen, listProfiles } from './api';
import { ProfileOpenAction } from './ProfileOpenAction';
import { StatusMessage } from '../../shared/ui/StatusMessage';
import { StatusBadge } from '../../shared/ui/StatusBadge';

export function ProfileDirectory({
  selectedProfileId,
  onSelect,
}: {
  selectedProfileId?: string | null;
  onSelect: (profileId: string) => void;
}) {
  const { tenantId, contexts } = useTenant();
  const activeTenant = contexts.find((context) => context.tenantId === tenantId);
  const query = useInfiniteQuery({
    queryKey: ['operator-query', tenantId, 'profiles'],
    queryFn: ({ signal, pageParam }) => listProfiles(tenantId, signal, pageParam),
    initialPageParam: null as string | null,
    getNextPageParam: (lastPage) => lastPage.nextCursor ?? undefined,
    enabled: tenantId.length > 0,
  });
  const [showDeleted, setShowDeleted] = useState(false);
  const [search, setSearch] = useState('');
  const [statusFilter, setStatusFilter] = useState('all');
  const [openError, setOpenError] = useState<{ tenantId: string; error: Error } | null>(null);
  const profiles = query.data?.pages.flatMap((page) => page.profiles) ?? [];
  const deletedCount = profiles.filter((profile) => profile.status === 'DELETED').length;
  const term = search.trim().toLocaleLowerCase();
  const visibleProfiles = profiles.filter((profile) => {
    if (!showDeleted && profile.status === 'DELETED') return false;
    if (statusFilter === 'ready' && profile.status !== 'READY') return false;
    if (statusFilter === 'in_use' && profile.status !== 'IN_USE') return false;
    if (statusFilter === 'attention' && !['DIRTY_LOCAL', 'QUARANTINED', 'SUSPENDED'].includes(profile.status)) return false;
    return profile.displayName.toLocaleLowerCase().includes(term)
      || profile.profileId.toLocaleLowerCase().includes(term)
      || (profile.linkedClientId?.toLocaleLowerCase().includes(term) ?? false);
  });

  if (!tenantId) {
    return <section className="panel full-span" aria-labelledby="profile-directory-title"><h2 id="profile-directory-title">Visible profiles</h2><p>Select an authorized organization before loading profiles.</p></section>;
  }

  return (
    <section className="panel full-span" aria-labelledby="profile-directory-title">
      <span className="eyebrow">Your browser workspace</span>
      <h2 id="profile-directory-title">Browser profiles</h2>
      <div className="list-toolbar">
        <label className="search-field">Search loaded profiles
          <input type="search" value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Name or identifier" />
        </label>
        <label>Status
          <select aria-label="Filter profiles by status" value={statusFilter} onChange={(event) => setStatusFilter(event.target.value)}>
            <option value="all">All statuses</option><option value="ready">Ready</option><option value="in_use">In use</option><option value="attention">Needs attention</option>
          </select>
        </label>
      </div>
      {deletedCount > 0 && (
        <button type="button" aria-pressed={showDeleted} onClick={() => setShowDeleted((value) => !value)}>
          {showDeleted ? 'Hide deleted profiles' : `Show deleted profiles (${deletedCount})`}
        </button>
      )}
      <StatusMessage state={query.error ?? (query.isPending ? 'Loading visible profiles…' : null)} />
      <StatusMessage state={openError?.tenantId === tenantId ? openError.error : null} />
      {!query.isPending && query.data && visibleProfiles.length === 0 && (term || statusFilter !== 'all') && (
        <p>No loaded profiles match these filters. Clear the search or load another page.</p>
      )}
      {!query.isPending && query.data && visibleProfiles.length === 0 && !term && statusFilter === 'all' && (
        <p>{profiles.length === 0
          ? 'No profiles are visible to the active actor in this tenant.'
          : query.hasNextPage
            ? 'No active profiles loaded. Show deleted profiles or load more.'
            : 'No active profiles loaded. Show deleted profiles.'}</p>
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
                  <td><strong>{profile.displayName}</strong><details className="profile-row-id"><summary>Identifier</summary><code>{profile.profileId}</code></details></td>
                  <td><StatusBadge status={profile.status} /></td>
                  <td>{profile.linkedClientId ? <><span>Assigned</span><details className="profile-row-id"><summary>Client ID</summary><code>{profile.linkedClientId}</code></details></> : <span className="muted">Unassigned</span>}</td>
                  <td><div className="profile-row-actions">
                    <ProfileOpenAction
                      tenantId={tenantId}
                      actorId={activeTenant?.actorId ?? null}
                      profile={profile}
                      dispatchOpen={dispatchBrowserProfileOpen}
                      onError={(error) => setOpenError(error ? { tenantId, error } : null)}
                    />
                    <button className="profile-row-details" type="button" onClick={() => onSelect(profile.profileId)}>
                      {profile.profileId === selectedProfileId ? 'Selected' : 'View details'}
                    </button>
                  </div></td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {query.hasNextPage && (
        <button type="button" disabled={query.isFetchingNextPage} onClick={() => { void query.fetchNextPage(); }}>
          {query.isFetchingNextPage ? 'Loading more profiles…' : 'Show more profiles'}
        </button>
      )}
    </section>
  );
}
