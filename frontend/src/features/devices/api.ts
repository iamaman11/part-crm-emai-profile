import {
  authorizeDevicePairing as authorizeDevicePairingOperation,
  getAuthenticatedSession as getAuthenticatedSessionOperation,
} from '../../shared/api/generated/operations';

export function getAuthenticatedDevicePairingSession(tenantId: string, signal?: AbortSignal) {
  return getAuthenticatedSessionOperation({
    tenantId,
    ...(signal === undefined ? {} : { signal }),
  });
}

export function authorizeDevicePairing(
  tenantId: string,
  pairingToken: string,
  signal?: AbortSignal,
) {
  return authorizeDevicePairingOperation({
    tenantId,
    body: { pairingToken },
    ...(signal === undefined ? {} : { signal }),
  });
}
