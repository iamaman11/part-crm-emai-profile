const displayNames: Record<string, string> = {
  READY: 'Ready', DRAFT: 'Draft', IN_USE: 'In use', DIRTY_LOCAL: 'Not synced',
  SYNCING: 'Syncing', QUARANTINED: 'Needs attention', SUSPENDED: 'Suspended',
  DELETED: 'Deleted', DELETING: 'Deleting', ACTIVE: 'Active', ARCHIVED: 'Archived',
};

export function StatusBadge({ status }: { status: string }) {
  const tone = status === 'READY' || status === 'ACTIVE' ? 'success'
    : status === 'IN_USE' || status === 'SYNCING' ? 'info'
      : status === 'DIRTY_LOCAL' || status === 'QUARANTINED' || status === 'SUSPENDED' || status === 'DELETING' ? 'warning'
        : 'neutral';
  return <span className={`status-pill status-${tone}`}>{displayNames[status] ?? status.replaceAll('_', ' ').toLowerCase()}</span>;
}
