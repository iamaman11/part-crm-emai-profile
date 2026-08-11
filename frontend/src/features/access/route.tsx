import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { AccessWorkspace } from './AccessWorkspace';
import { MemberDirectory } from './MemberDirectory';

function UsersPage() {
  return (
    <div className="page-stack">
      <MemberDirectory />
      <AccessWorkspace />
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
