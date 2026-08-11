import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { SettingsWorkspace } from './SettingsWorkspace';

export function createSettingsRoute(parentRoute: AnyRoute) {
  return createRoute({
    getParentRoute: () => parentRoute,
    path: '/settings',
    component: SettingsWorkspace,
  });
}
