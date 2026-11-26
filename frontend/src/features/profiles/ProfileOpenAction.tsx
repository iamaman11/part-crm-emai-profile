import { useId, useState } from 'react';
import { invokeProfileBridgeDiagnostics } from '../../shared/bridgeDiagnostics';
import type { dispatchBrowserProfileOpen, ProfileListPageDto } from './api';

type ListedProfile = ProfileListPageDto['profiles'][number];

/** A list-row user gesture. The installed Bridge owns launch admission; the
 * CRM read projection only explains whether this actor may request Open.
 */
export function ProfileOpenAction({
  tenantId,
  actorId,
  profile,
  dispatchOpen,
  onError,
}: {
  tenantId: string;
  actorId: string | null;
  profile: ListedProfile;
  dispatchOpen: typeof dispatchBrowserProfileOpen;
  onError: (error: Error | null) => void;
}) {
  const reasonId = useId();
  const [openRequested, setOpenRequested] = useState(false);
  const unavailable = !actorId ? 'Organization access is unavailable.'
    : !profile.canOperate ? 'Operator permission is required.'
      : profile.status !== 'DRAFT' && profile.status !== 'READY'
        ? `Cannot open a ${profile.status} profile.` : null;

  const open = () => {
    // No asynchronous preflight: the protocol dispatch remains in the actual
    // click and must use this exact row's tenant, actor and profile version.
    if (!actorId || !profile.canOperate || (profile.status !== 'DRAFT' && profile.status !== 'READY')) return;
    onError(null);
    setOpenRequested(false);
    try {
      dispatchOpen(tenantId, actorId, profile.profileId, profile.version, profile.status);
      // Protocol handoff is not a witness of BrowserReady, Close or cloud Saved.
      setOpenRequested(true);
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
      <button
        type="button"
        className="profile-open-diagnostics"
        onClick={invokeProfileBridgeDiagnostics}
        title="Copy the installed Bridge's redacted local observations; nothing is uploaded"
      >
        Copy diagnostics
      </button>
      {openRequested && (
        <small className="profile-open-reason" role="status">
          Windows Open requested. This page cannot confirm BrowserReady or cloud Saved; check the Bridge diagnostics if no window appears.
        </small>
      )}
      {unavailable && <small className="profile-open-reason" id={reasonId}>{unavailable}</small>}
    </div>
  );
}
