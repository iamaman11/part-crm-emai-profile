import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { ClientsWorkspace } from './ClientsWorkspace';

export function createClientsRoute(parentRoute: AnyRoute) {
  return createRoute({
    getParentRoute: () => parentRoute,
    path: '/clients',
    component: ClientsWorkspace,
  });
}
