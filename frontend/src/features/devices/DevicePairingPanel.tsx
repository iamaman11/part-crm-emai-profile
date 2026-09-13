import { useMemo, useState } from 'react';
import { useTenant } from '../../app/TenantContext';
import { StatusMessage } from '../../shared/ui/StatusMessage';
import {
  authorizeDevicePairing,
  getAuthenticatedDevicePairingSession,
} from './api';

const OPAQUE_ID_PATTERN = /^[A-Za-z0-9_-]{8,96}$/;
const LOWER_HEX_64_PATTERN = /^[0-9a-f]{64}$/;
const MAX_CHALLENGE_LIFETIME_MS = 120_000;

type DeviceFragment =
  | { kind: 'none' }
  | { kind: 'invalid' }
  | { kind: 'pending'; pairingToken: string; deviceId: string }
  | { kind: 'connected'; deviceId: string };

function newDeviceId(): string {
  const bytes = new Uint8Array(16);
  globalThis.crypto.getRandomValues(bytes);
  const suffix = Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
  return `device_${suffix}`;
}

function parseDeviceFragment(hash: string): DeviceFragment {
  if (!hash) return { kind: 'none' };
  if (!hash.startsWith('#')) return { kind: 'invalid' };

  const params = new URLSearchParams(hash.slice(1));
  const entries = [...params.entries()];

  if (
    entries.length === 2
    && params.getAll('pairing').length === 1
    && params.getAll('device').length === 1
  ) {
    const pairingToken = params.get('pairing') ?? '';
    const deviceId = params.get('device') ?? '';
    if (LOWER_HEX_64_PATTERN.test(pairingToken) && OPAQUE_ID_PATTERN.test(deviceId)) {
      return { kind: 'pending', pairingToken, deviceId };
    }
    return { kind: 'invalid' };
  }

  if (entries.length === 1 && params.getAll('connected').length === 1) {
    const deviceId = params.get('connected') ?? '';
    return OPAQUE_ID_PATTERN.test(deviceId)
      ? { kind: 'connected', deviceId }
      : { kind: 'invalid' };
  }

  return { kind: 'invalid' };
}

function cleanFragmentFromAddressBar() {
  const url = new URL(window.location.href);
  url.hash = '';
  window.history.replaceState(null, '', `${url.pathname}${url.search}`);
}

function validChallengeExpiry(expiresAtMs: number): boolean {
  const now = Date.now();
  return Number.isSafeInteger(expiresAtMs)
    && expiresAtMs > now
    && expiresAtMs - now <= MAX_CHALLENGE_LIFETIME_MS;
}

function validateOpaqueId(value: string): boolean {
  return OPAQUE_ID_PATTERN.test(value);
}

export function DevicePairingPanel() {
  const { tenantId } = useTenant();
  const fragment = parseDeviceFragment(window.location.hash);
  const deviceId = useMemo(() => (validateOpaqueId(tenantId) ? newDeviceId() : ''), [tenantId]);
  const [status, setStatus] = useState<unknown>(null);
  const [finishUri, setFinishUri] = useState<string | null>(null);
  const [authorizing, setAuthorizing] = useState(false);

  if (!tenantId) {
    return <StatusMessage state="Choose a tenant before connecting this computer." />;
  }
  if (!validateOpaqueId(tenantId)) {
    return <StatusMessage state="The selected tenant identifier is invalid." />;
  }
  if (fragment.kind === 'invalid') {
    return <StatusMessage state="The device pairing callback is invalid." />;
  }
  if (fragment.kind === 'connected') {
    return (
      <section className="panel" aria-label="Connected device">
        <span className="eyebrow">Profile Bridge</span>
        <h3>This computer is connected</h3>
        <p>Device identity: <strong>{fragment.deviceId}</strong></p>
      </section>
    );
  }

  const startUri = `profilebridge://pair/start/${tenantId}/${deviceId}`;

  async function authorizePendingPairing() {
    if (fragment.kind !== 'pending' || authorizing) return;
    setAuthorizing(true);
    setStatus('Authorizing this computer…');
    try {
      const session = await getAuthenticatedDevicePairingSession(tenantId);
      if (
        session.tenantId !== tenantId
        || !validateOpaqueId(session.actorId)
        || !validateOpaqueId(fragment.deviceId)
      ) {
        throw new Error('Device pairing response failed validation.');
      }

      const challenge = await authorizeDevicePairing(tenantId, fragment.pairingToken);
      if (
        challenge.deviceId !== fragment.deviceId
        || !LOWER_HEX_64_PATTERN.test(challenge.challengeToken)
        || !LOWER_HEX_64_PATTERN.test(challenge.nonceHex)
        || !validChallengeExpiry(challenge.expiresAtMs)
      ) {
        throw new Error('Device pairing response failed validation.');
      }

      const completion = [
        'profilebridge://pair/complete',
        tenantId,
        session.actorId,
        fragment.deviceId,
        fragment.pairingToken,
        challenge.challengeToken,
        challenge.nonceHex,
        String(challenge.expiresAtMs),
      ].join('/');

      cleanFragmentFromAddressBar();
      setFinishUri(completion);
      setStatus('Authorization confirmed. Finish connecting on this computer.');
    } catch (error) {
      setFinishUri(null);
      setStatus(error);
    } finally {
      setAuthorizing(false);
    }
  }

  if (finishUri !== null) {
    return (
      <section className="panel" aria-label="Finish device pairing">
        <span className="eyebrow">Profile Bridge</span>
        <h3>Finish connecting this computer</h3>
        <p>The browser authorization is complete. Return the one-time proof to Profile Bridge.</p>
        <a href={finishUri}>Finish connecting</a>
        <StatusMessage state={status} />
      </section>
    );
  }

  if (fragment.kind === 'pending') {
    return (
      <section className="panel" aria-label="Authorize device pairing">
        <span className="eyebrow">Profile Bridge</span>
        <h3>Authorize this computer</h3>
        <p>Confirm the signed-in actor may bind this non-exportable device key to the selected tenant.</p>
        <button type="button" onClick={authorizePendingPairing} disabled={authorizing}>
          {authorizing ? 'Authorizing…' : 'Authorize this computer'}
        </button>
        <StatusMessage state={status} />
      </section>
    );
  }

  return (
    <section className="panel" aria-label="Connect this computer">
      <span className="eyebrow">Profile Bridge</span>
      <h3>Connect this computer</h3>
      <p>
        Start a one-time browser pairing. The device private key remains non-exportable on this Windows computer.
      </p>
      <a href={startUri}>Connect this computer</a>
      <StatusMessage state={status} />
    </section>
  );
}
