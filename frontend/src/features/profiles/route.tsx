import {
  createRoute,
  type AnyRoute,
  useNavigate,
  useParams,
} from '@tanstack/react-router';
import { ProfileDirectory } from './ProfileDirectory';
import { ProfilesWorkspace } from './ProfilesWorkspace';

function useProfileSelectionNavigation() {
  const navigate = useNavigate();
  return (profileId: string) => {
    void navigate({ to: '/profiles/$profileId', params: { profileId } });
  };
}

function ProfilesIndexPage() {
  const onSelect = useProfileSelectionNavigation();
  return (
    <div className="page-stack">
      <ProfileDirectory onSelect={onSelect} />
      <ProfilesWorkspace onProfileSelected={onSelect} />
    </div>
  );
}

function ProfileDetailPage() {
  const params = useParams({ strict: false });
  const selectedProfileId = typeof params.profileId === 'string' ? params.profileId : null;
  const onSelect = useProfileSelectionNavigation();
  return (
    <div className="page-stack">
      <ProfileDirectory selectedProfileId={selectedProfileId} onSelect={onSelect} />
      <ProfilesWorkspace selectedProfileId={selectedProfileId} onProfileSelected={onSelect} />
    </div>
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
