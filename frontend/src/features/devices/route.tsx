import { Link, createRoute, type AnyRoute } from '@tanstack/react-router';
import { DevicePairingPanel } from './DevicePairingPanel';

function DevicesPage() {
  return (
    <div className="page-stack">
      <section className="hero panel">
        <span className="eyebrow">Device / Bridge boundary</span>
        <h2>Device operations</h2>
        <p>
          Device claim, heartbeat, generation upload and outcome endpoints remain machine-authenticated
          protocol surfaces. Browser participation is limited to the explicit one-time authorization used
          to bind a non-exportable Profile Bridge device key to the signed-in actor.
        </p>
      </section>
      <DevicePairingPanel />
      <section className="workspace-grid">
        <article className="panel">
          <h3>Profile execution state</h3>
          <p>Inspect canonical profile generations, coordinator ownership and recovery state.</p>
          <Link to="/profiles">Open profiles</Link>
        </article>
        <article className="panel">
          <h3>Browser mailbox lane</h3>
          <p>Inspect governed mailbox binding metadata without turning browser execution into a web command.</p>
          <Link to="/mailboxes">Open mailboxes</Link>
        </article>
      </section>
    </div>
  );
}

export function createDevicesRoute(parentRoute: AnyRoute) {
  return createRoute({
    getParentRoute: () => parentRoute,
    path: '/devices',
    component: DevicesPage,
  });
}
