import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { AccessWorkspace } from './AccessWorkspace';

export function createAccessRoute(parentRoute: AnyRoute) {
  return createRoute({
    getParentRoute: () => parentRoute,
    path: '/users',
    component: AccessWorkspace,
  });
}
