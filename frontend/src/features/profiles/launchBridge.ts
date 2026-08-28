const PROFILE_BRIDGE_SCHEME = 'profilebridge://claim/';

export function invokeProfileBridgeLaunch(launchUri: string): void {
  if (!launchUri.startsWith(PROFILE_BRIDGE_SCHEME)) {
    throw new Error('Profile Bridge launch response failed closed');
  }
  globalThis.location.assign(launchUri);
}
