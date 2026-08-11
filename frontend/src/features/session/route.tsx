import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { SessionPanel } from './SessionPanel';

function SessionsPage() {
  return (
    <div className="page-stack">
      <section className="hero panel">
        <span className="eyebrow">Authenticated application context</span>
        <h2>Current session</h2>
        <p>
          This page shows only the server-resolved actor and tenant context. Profile writer/coordinator
          sessions remain owned by each profile and are administered from the profile detail route.
        </p>
      </section>
      <SessionPanel />
    </div>
  );
}

export function createSessionRoute(parentRoute: AnyRoute) {
  return createRoute({
    getParentRoute: () => parentRoute,
    path: '/sessions',
    component: SessionsPage,
  });
}
