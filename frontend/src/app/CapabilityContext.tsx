import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import { getSession, type ActivationUnit } from '../features/session/api';
import { useTenant } from './TenantContext';

export type { ActivationUnit };

interface CapabilityContextValue {
  profileId: string | null;
  profileDigest: string | null;
  capabilities: ReadonlySet<ActivationUnit>;
  ready: boolean;
  error: string | null;
  enabled: (unit: ActivationUnit) => boolean;
}

const CapabilityContext = createContext<CapabilityContextValue | null>(null);
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
        const session = await getSession(tenantId, controller.signal);
        const nextCapabilities = new Set<ActivationUnit>(session.capabilities);
        if (!nextCapabilities.has('foundation')) {
          throw new TypeError('Capability projection is missing foundation');
        }
        setProfileId(session.profileId);
        setProfileDigest(session.profileDigest);
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
