import { Link, createRoute, type AnyRoute } from '@tanstack/react-router';

function AuditPage() {
  return (
    <div className="page-stack">
      <section className="hero panel">
        <span className="eyebrow">Sanitized governance evidence</span>
        <h2>Audit & history</h2>
        <p>
          The standalone product exposes privacy-reviewed resource history, not raw internal audit tables.
          Tenant-wide raw audit export remains unavailable until a dedicated bounded public projection is accepted.
        </p>
      </section>
      <section className="workspace-grid">
        <article className="panel">
          <h3>Client history</h3>
          <p>Open a client to inspect the accepted resource-local history and grant-safe lifecycle evidence.</p>
          <Link to="/clients">Open clients</Link>
        </article>
        <article className="panel">
          <h3>Profile lifecycle evidence</h3>
          <p>Inspect generation and coordinator state from the profile-owned operator surface.</p>
          <Link to="/profiles">Open profiles</Link>
        </article>
      </section>
    </div>
  );
}

export function createAuditRoute(parentRoute: AnyRoute) {
  return createRoute({
    getParentRoute: () => parentRoute,
    path: '/audit',
    component: AuditPage,
  });
}
