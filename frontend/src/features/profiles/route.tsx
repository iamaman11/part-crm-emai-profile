import {
  createRoute,
  type AnyRoute,
  useNavigate,
  useParams,
} from '@tanstack/react-router';
import { ProfilesWorkspace } from './ProfilesWorkspace';

function useProfileSelectionNavigation() {
  const navigate = useNavigate();
  return (profileId: string) => {
    void navigate({ to: '/profiles/$profileId', params: { profileId } });
  };
}

function ProfilesIndexPage() {
  return <ProfilesWorkspace onProfileSelected={useProfileSelectionNavigation()} />;
}

function ProfileDetailPage() {
  const params = useParams({ strict: false });
  const selectedProfileId = typeof params.profileId === 'string' ? params.profileId : null;
  return (
    <ProfilesWorkspace
      selectedProfileId={selectedProfileId}
      onProfileSelected={useProfileSelectionNavigation()}
    />
  );
}

export function createProfilesRoutes(parentRoute: AnyRoute) {
  return [
    createRoute({
      getParentRoute: () => parentRoute,
      path: '/profiles',
      component: ProfilesIndexPage,
    }),
    createRoute({
      getParentRoute: () => parentRoute,
      path: '/profiles/$profileId',
      component: ProfileDetailPage,
    }),
  ] as const;
}
