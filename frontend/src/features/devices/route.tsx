import { Link, createRoute, type AnyRoute } from '@tanstack/react-router';

function DevicesPage() {
  return (
    <div className="page-stack">
      <section className="hero panel">
        <span className="eyebrow">Device / Bridge boundary</span>
        <h2>Device operations</h2>
        <p>
          Device claim, heartbeat, generation upload and outcome endpoints are machine-authenticated
          protocol surfaces. This operator UI does not impersonate a device or expose device credentials.
        </p>
      </section>
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
