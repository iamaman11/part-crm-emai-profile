import { useQueryClient } from '@tanstack/react-query';
import { useEffect } from 'react';
import { useTenant } from '../../app/TenantContext';
import { parseRealtimeMessage, realtimeWebSocketUrl, RealtimeEventDeduper } from './notifications';

const RECONNECT_DELAYS_MS = [1_000, 2_000, 4_000, 8_000, 15_000] as const;
const POLICY_REVOKED_CLOSE_CODE = 1008;

export function NotificationRealtimeBridge() {
  const { tenantId } = useTenant();
  const queryClient = useQueryClient();

  useEffect(() => {
    if (!tenantId) return undefined;

    let disposed = false;
    let reconnectAttempt = 0;
    let reconnectTimer: number | undefined;
    let socket: WebSocket | undefined;
    const deduper = new RealtimeEventDeduper();

    const scheduleReconnect = () => {
      if (disposed || reconnectTimer !== undefined) return;
      const delay = RECONNECT_DELAYS_MS[Math.min(reconnectAttempt, RECONNECT_DELAYS_MS.length - 1)];
      reconnectAttempt += 1;
      reconnectTimer = window.setTimeout(() => {
        reconnectTimer = undefined;
        connect();
      }, delay);
    };

    const connect = () => {
      if (disposed) return;
      const next = new WebSocket(realtimeWebSocketUrl(tenantId, window.location));
      socket = next;
      next.onopen = () => {
        reconnectAttempt = 0;
      };
      next.onmessage = (event) => {
        if (typeof event.data !== 'string') return;
        const signal = parseRealtimeMessage(event.data);
        if (signal === null || !deduper.accept(signal.eventId)) return;

        // Realtime is only a cache-invalidation hint. Canonical values always come back through
        // the existing authenticated HTTPS query functions; the signal itself never populates data.
        void queryClient.invalidateQueries({
          predicate: (query) => query.queryKey.includes(tenantId),
        });
      };
      next.onerror = () => {
        next.close();
      };
      next.onclose = (event) => {
        if (socket === next) socket = undefined;
        // 1008 is emitted by the server when current membership authorization is revoked or cannot
        // be re-established. Do not churn reconnect attempts until tenant context changes.
        if (!disposed && event.code !== POLICY_REVOKED_CLOSE_CODE) scheduleReconnect();
      };
    };

    connect();
    return () => {
      disposed = true;
      if (reconnectTimer !== undefined) window.clearTimeout(reconnectTimer);
      socket?.close(1000, 'tenant context changed');
    };
  }, [queryClient, tenantId]);

  return null;
}
