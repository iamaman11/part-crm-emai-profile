import {
  authorizeDevicePairing as authorizeDevicePairingOperation,
  getAuthenticatedSession as getAuthenticatedSessionOperation,
  getDeviceRegistration as getDeviceRegistrationOperation,
} from '../../shared/api/generated/operations';

export async function getDeviceRegistration(tenantId: string, deviceId: string, signal?: AbortSignal) {
  const projection = await getDeviceRegistrationOperation({
    tenantId,
    deviceId,
    ...(signal === undefined ? {} : { signal }),
  });
  if (projection.tenantId !== tenantId || projection.deviceId !== deviceId) {
    throw new Error('Device registration response failed validation.');
  }
  return projection;
}

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
