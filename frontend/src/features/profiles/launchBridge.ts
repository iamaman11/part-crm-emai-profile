const PROFILE_BRIDGE_OPEN = /^profilebridge:\/\/open\/v2\/[A-Za-z0-9_-]{8,96}\/[A-Za-z0-9_-]{8,96}\/[A-Za-z0-9_-]{8,96}\/[1-9][0-9]{0,18}\/(?:DRAFT|READY)$/;

export function invokeProfileBridgeLaunch(launchUri: string): void {
  if (!PROFILE_BRIDGE_OPEN.test(launchUri)) throw new Error('Profile Bridge open response failed closed');
  globalThis.location.assign(launchUri);
}
