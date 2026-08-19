import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import { useTenant } from './TenantContext';

export type ActivationUnit =
  | 'foundation'
  | 'identity'
  | 'clients'
  | 'browser_profiles'
  | 'profile_runtime'
  | 'camoufox'
  | 'notifications'
  | 'mailbox_admin'
  | 'mailbox_client_binding'
  | 'mailbox_browser_binding'
  | 'mailbox_read'
  | 'mailbox_jobs'
  | 'outbound_mail';

interface CapabilityContextValue {
  profileId: string | null;
  profileDigest: string | null;
  capabilities: ReadonlySet<ActivationUnit>;
  ready: boolean;
  error: string | null;
  enabled: (unit: ActivationUnit) => boolean;
}

const CapabilityContext = createContext<CapabilityContextValue | null>(null);
const KNOWN_ACTIVATION_UNITS = new Set<ActivationUnit>([
  'foundation',
  'identity',
  'clients',
  'browser_profiles',
  'profile_runtime',
  'camoufox',
  'notifications',
  'mailbox_admin',
  'mailbox_client_binding',
  'mailbox_browser_binding',
  'mailbox_read',
  'mailbox_jobs',
  'outbound_mail',
]);

function requestId(): string {
  return `corr_${crypto.randomUUID().replaceAll('-', '')}`;
}

function parseCapabilities(raw: string | null): Set<ActivationUnit> {
  if (raw === null || raw.trim() === '') return new Set();
  const values = raw.split(',').map((value) => value.trim()).filter(Boolean);
  const result = new Set<ActivationUnit>();
  for (const value of values) {
    if (!KNOWN_ACTIVATION_UNITS.has(value as ActivationUnit)) {
      throw new TypeError(`Unknown capability projection: ${value}`);
    }
    result.add(value as ActivationUnit);
  }
  return result;
}

export function CapabilityProvider({ children }: { children: ReactNode }) {
  const { tenantId } = useTenant();
  const [profileId, setProfileId] = useState<string | null>(null);
  const [profileDigest, setProfileDigest] = useState<string | null>(null);
  const [capabilities, setCapabilities] = useState<ReadonlySet<ActivationUnit>>(new Set());
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const controller = new AbortController();
    setReady(false);
    setError(null);
    setProfileId(null);
    setProfileDigest(null);
    setCapabilities(new Set());

    if (!tenantId) {
      return () => controller.abort();
    }

    void (async () => {
      try {
        const response = await fetch('/api/v1/session', {
          method: 'GET',
          headers: {
            Accept: 'application/json',
            'X-Tenant-Id': tenantId,
            'X-Correlation-Id': requestId(),
          },
          credentials: 'same-origin',
          redirect: 'error',
          signal: controller.signal,
        });
        if (!response.ok) throw new TypeError(`Capability projection unavailable (${response.status})`);
        const nextProfileId = response.headers.get('x-release-profile');
        const nextProfileDigest = response.headers.get('x-release-profile-digest');
        if (!nextProfileId || !nextProfileDigest) {
          throw new TypeError('Capability projection headers are missing');
        }
        const nextCapabilities = parseCapabilities(
          response.headers.get('x-effective-capabilities'),
        );
        if (!nextCapabilities.has('foundation')) {
          throw new TypeError('Capability projection is missing foundation');
        }
        setProfileId(nextProfileId);
        setProfileDigest(nextProfileDigest);
        setCapabilities(nextCapabilities);
        setReady(true);
      } catch (caught) {
        if (controller.signal.aborted) return;
        setError(caught instanceof Error ? caught.message : 'Capability projection unavailable');
        setReady(false);
      }
    })();

    return () => controller.abort();
  }, [tenantId]);

  const value = useMemo<CapabilityContextValue>(() => ({
    profileId,
    profileDigest,
    capabilities,
    ready,
    error,
    enabled: (unit) => ready && capabilities.has(unit),
  }), [profileId, profileDigest, capabilities, ready, error]);

  return <CapabilityContext.Provider value={value}>{children}</CapabilityContext.Provider>;
}

export function useCapabilities(): CapabilityContextValue {
  const value = useContext(CapabilityContext);
  if (value === null) throw new Error('CapabilityProvider is missing');
  return value;
}

export function CapabilityBoundary({
  unit,
  children,
}: {
  unit: ActivationUnit;
  children: ReactNode;
}) {
  const { ready, error, enabled } = useCapabilities();
  if (!ready) {
    return (
      <section className="panel" role="status">
        <h2>Capability unavailable</h2>
        <p>{error ?? 'Select a tenant and wait for the authenticated capability projection.'}</p>
      </section>
    );
  }
  if (!enabled(unit)) {
    return (
      <section className="panel" role="status">
        <h2>Capability unavailable</h2>
        <p>This capability is not enabled by the active release profile.</p>
      </section>
    );
  }
  return children;
}
