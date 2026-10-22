import {
  authorizeDevicePairing as authorizeDevicePairingOperation,
  getAuthenticatedSession as getAuthenticatedSessionOperation,
  getDeviceRegistration as getDeviceRegistrationOperation,
  getDeviceInstaller as getDeviceInstallerOperation,
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

export async function getDeviceInstaller(tenantId: string, signal?: AbortSignal) {
  const projection = await getDeviceInstallerOperation({
    tenantId,
    ...(signal === undefined ? {} : { signal }),
  });
  const expectedUrl = `https://github.com/iamaman11/part-crm-emai-profile/releases/download/${projection.releaseSetId}/ProfileBridgeSetup.exe`;
  if (projection.schemaVersion !== 1
    || !/^release-set-v3-sha256-[0-9a-f]{64}$/.test(projection.releaseSetId)
    || projection.downloadUrl !== expectedUrl) {
    throw new Error('Windows installer response failed validation.');
  }
  return projection;
}
