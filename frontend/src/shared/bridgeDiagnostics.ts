// A user gesture opens the installed local Windows bridge. No HTTP request,
// profile identifiers, claims, cookies or diagnostics cross the web origin.
export const COPY_LOCAL_DIAGNOSTICS_URI = 'profilebridge://diagnostics/copy';

export function invokeProfileBridgeDiagnostics(): void {
  globalThis.location.assign(COPY_LOCAL_DIAGNOSTICS_URI);
}
