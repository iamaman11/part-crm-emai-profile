const PROFILE_BRIDGE_OPEN = /^profilebridge:\/\/open\/v2\/[A-Za-z0-9_-]{8,96}\/[A-Za-z0-9_-]{8,96}\/[A-Za-z0-9_-]{8,96}\/[1-9][0-9]{0,18}\/(?:DRAFT|READY)$/;

/** One installed native Bridge URI, assembled from the already loaded
 * authenticated tenant context and selected CRM profile projection.
 * No per-Open cloud command or extra state owner is required.
 */
export function localProfileOpenUri(
  tenantId: string,
  actorId: string,
  profileId: string,
  version: number,
  status: 'DRAFT' | 'READY',
): string {
  if (!Number.isSafeInteger(version) || version <= 0) {
    throw new Error('Invalid local profile version');
  }
  const uri = `profilebridge://open/v2/${tenantId}/${actorId}/${profileId}/${version}/${status}`;
  if (!PROFILE_BRIDGE_OPEN.test(uri)) {
    throw new Error('Invalid native profile Open context');
  }
  return uri;
}

export function invokeProfileBridgeLaunch(launchUri: string): void {
  if (!PROFILE_BRIDGE_OPEN.test(launchUri)) throw new Error('Profile Bridge open response failed closed');
  globalThis.location.assign(launchUri);
}
