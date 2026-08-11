import {
  createRoute,
  type AnyRoute,
  useNavigate,
  useParams,
} from '@tanstack/react-router';
import { ClientsWorkspace } from './ClientsWorkspace';

function useClientSelectionNavigation() {
  const navigate = useNavigate();
  return (clientId: string) => {
    void navigate({ to: '/clients/$clientId', params: { clientId } });
  };
}

function ClientsIndexPage() {
  return <ClientsWorkspace onClientSelected={useClientSelectionNavigation()} />;
}

function ClientDetailPage() {
  const params = useParams({ strict: false });
  const selectedClientId = typeof params.clientId === 'string' ? params.clientId : null;
  return (
    <ClientsWorkspace
      selectedClientId={selectedClientId}
      onClientSelected={useClientSelectionNavigation()}
    />
  );
}

export function createClientsRoutes(parentRoute: AnyRoute) {
  return [
    createRoute({
      getParentRoute: () => parentRoute,
      path: '/clients',
      component: ClientsIndexPage,
    }),
    createRoute({
      getParentRoute: () => parentRoute,
      path: '/clients/$clientId',
      component: ClientDetailPage,
    }),
  ] as const;
}
