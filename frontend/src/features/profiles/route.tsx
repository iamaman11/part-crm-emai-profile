import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { ProfilesWorkspace } from './ProfilesWorkspace';

export function createProfilesRoute(parentRoute: AnyRoute) {
  return createRoute({
    getParentRoute: () => parentRoute,
    path: '/profiles',
    component: ProfilesWorkspace,
  });
}
