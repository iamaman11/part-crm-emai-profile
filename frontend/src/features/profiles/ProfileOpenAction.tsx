import { useId } from 'react';
import { dispatchBrowserProfileOpen, type ProfileListPageDto } from './api';

type ListedProfile = ProfileListPageDto['profiles'][number];

/** A list-row user gesture. The installed Bridge owns launch admission; the
 * CRM read projection only explains whether this actor may request Open.
 */
export function ProfileOpenAction({
  tenantId,
  actorId,
  profile,
  onError,
}: {
  tenantId: string;
  actorId: string | null;
  profile: ListedProfile;
  onError: (error: Error | null) => void;
}) {
  const reasonId = useId();
  const unavailable = !actorId ? 'Organization access is unavailable.'
    : !profile.canOperate ? 'Operator permission is required.'
      : profile.status !== 'DRAFT' && profile.status !== 'READY'
        ? `Cannot open a ${profile.status} profile.` : null;

  const open = () => {
    // No asynchronous preflight: the protocol dispatch remains in the actual
    // click and must use this exact row's tenant, actor and profile version.
    if (!actorId || !profile.canOperate || (profile.status !== 'DRAFT' && profile.status !== 'READY')) return;
    onError(null);
    try {
      dispatchBrowserProfileOpen(tenantId, actorId, profile.profileId, profile.version, profile.status);
    } catch {
      onError(new Error(`Could not hand off ${profile.displayName} to the installed Profile Bridge. Check the installation and try again.`));
    }
  };

  return (
    <div className="profile-open-control">
      <button
        type="button"
        aria-label={`Open browser for ${profile.displayName}`}
        disabled={unavailable !== null}
        aria-describedby={unavailable ? reasonId : undefined}
        title={unavailable ?? 'Open with installed Profile Bridge'}
        onClick={open}
      >
        Open browser
      </button>
      {unavailable && <small className="profile-open-reason" id={reasonId}>{unavailable}</small>}
    </div>
  );
}
