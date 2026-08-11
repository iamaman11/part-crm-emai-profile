import { Link, Outlet, createRootRoute, createRoute, createRouter } from '@tanstack/react-router';
import { TenantChooser } from './TenantContext';
import { createAccessRoute } from '../features/access';
import { createAuditRoute } from '../features/audit';
import { createClientsRoutes } from '../features/clients';
import { createDevicesRoute } from '../features/devices';
import { createMailboxesRoute } from '../features/mailboxes';
import { createProfilesRoutes } from '../features/profiles';
import { createSessionRoute, SessionPanel } from '../features/session';
import { createSettingsRoute } from '../features/settings';
import { NetworkStatusBanner } from '../shared/ui/NetworkStatusBanner';

function Shell() {
  return (
    <div className="app-shell">
      <header className="app-header">
        <div>
          <span className="eyebrow">Repository-local control plane</span>
          <h1>Profile Operations</h1>
        </div>
        <TenantChooser />
      </header>
      <NetworkStatusBanner />
      <nav className="primary-nav" aria-label="Primary navigation">
        <Link to="/" activeOptions={{ exact: true }}>Dashboard</Link>
        <Link to="/clients">Clients</Link>
        <Link to="/profiles">Profiles</Link>
        <Link to="/users">Users & access</Link>
        <Link to="/mailboxes">Mailboxes</Link>
        <Link to="/sessions">Sessions</Link>
        <Link to="/devices">Devices</Link>
        <Link to="/audit">Audit</Link>
        <Link to="/settings">Settings</Link>
      </nav>
      <main id="main-content"><Outlet /></main>
      <footer>
        Authorization and lifecycle decisions remain server-side. UI state is not production evidence.
      </footer>
    </div>
  );
}

function Dashboard() {
  return (
    <div className="page-stack">
      <section className="hero panel">
        <span className="eyebrow">Operator dashboard</span>
        <h2>Discoverable standalone workflows, fail-closed by default.</h2>
        <p>
          Authorized server projections drive Clients, Profiles, Users and Mailboxes. Resource detail
          routes remain canonical, and confidential Client Mail data is transient rather than browser-persisted.
        </p>
      </section>
      <SessionPanel />
      <section className="workspace-grid">
        <article className="panel"><h3>Clients</h3><p>Browse the live grant-filtered Client Registry and open canonical detail and Client Mail workflows.</p><Link to="/clients">Open clients</Link></article>
        <article className="panel"><h3>Profiles</h3><p>Browse visible profiles and operate assignment, grants, generations and coordinator state.</p><Link to="/profiles">Open profiles</Link></article>
        <article className="panel"><h3>Mailboxes</h3><p>Browse binding metadata and operate bounded jobs without exposing credentials or message payloads.</p><Link to="/mailboxes">Open mailboxes</Link></article>
        <article className="panel"><h3>Administration</h3><p>Review access, current session, device boundaries, sanitized audit surfaces and safe diagnostics.</p><Link to="/users">Open administration</Link></article>
      </section>
    </div>
  );
}

const rootRoute = createRootRoute({ component: Shell });
const indexRoute = createRoute({ getParentRoute: () => rootRoute, path: '/', component: Dashboard });
const clientsRoutes = createClientsRoutes(rootRoute);
const profilesRoutes = createProfilesRoutes(rootRoute);
const mailboxesRoute = createMailboxesRoute(rootRoute);
const usersRoute = createAccessRoute(rootRoute);
const sessionsRoute = createSessionRoute(rootRoute);
const devicesRoute = createDevicesRoute(rootRoute);
const auditRoute = createAuditRoute(rootRoute);
const settingsRoute = createSettingsRoute(rootRoute);

const routeTree = rootRoute.addChildren([
  indexRoute,
  ...clientsRoutes,
  ...profilesRoutes,
  usersRoute,
  mailboxesRoute,
  sessionsRoute,
  devicesRoute,
  auditRoute,
  settingsRoute,
]);
export const router = createRouter({ routeTree, defaultPreload: 'intent' });

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router;
  }
}
