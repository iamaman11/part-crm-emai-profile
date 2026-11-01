import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { AccessWorkspace } from './AccessWorkspace';
import { MemberDirectory } from './MemberDirectory';
import { useTenant } from '../../app/TenantContext';

// The existing command API requires owner-only proof inputs (HMAC, opaque IDs
// and optimistic versions). It is not an ordinary self-service membership UI.
// Do not expose high-impact owner ceremonies as routine member operations.
export function UsersPage() {
  const { tenantId, contexts } = useTenant();
  const isOwner = contexts.some((entry) => entry.tenantId === tenantId && entry.role === 'TENANT_OWNER');
  return (
    <div className="page-stack">
      <MemberDirectory />
      {isOwner ? (
        <>
          <section className="panel" aria-label="Advanced owner operations">
            <h2>Advanced owner operations</h2>
            <p>
              The commands below require verified identifiers, HMAC and current server versions.
              They are not ordinary invite or membership self-service. Do not guess versions or
              compute cryptographic proof by hand for daily operations.
            </p>
          </section>
          <AccessWorkspace />
        </>
      ) : (
        <section className="panel" aria-label="Membership operations unavailable">
          <h2>Membership directory</h2>
          <p>
            This view is read-only. Invitations, role changes and owner transfer are
            restricted to the existing owner-governed command path; no ordinary
            member action or approval is implied by this directory.
          </p>
        </section>
      )}
    </div>
  );
}

export function createAccessRoute(parentRoute: AnyRoute) {
  return createRoute({
    getParentRoute: () => parentRoute,
    path: '/users',
    component: UsersPage,
  });
}
