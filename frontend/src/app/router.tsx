import { Link, Outlet, createRootRoute, createRoute, createRouter } from '@tanstack/react-router';
import { CapabilityBoundary, useCapabilities } from './CapabilityContext';
import { TenantChooser, useTenant } from './TenantContext';
import { createAccessRoute, OrganizationSetup } from '../features/access';
import { createAuditRoute } from '../features/audit';
import { createClientsRoutes } from '../features/clients';
import { createDevicesRoute } from '../features/devices';
import { createMailboxesRoute } from '../features/mailboxes';
import { createProfilesRoutes } from '../features/profiles';
import { createSessionRoute, SessionPanel } from '../features/session';
import { createSettingsRoute } from '../features/settings';
import { NetworkStatusBanner } from '../shared/ui/NetworkStatusBanner';
import { ThemeToggle } from './ThemeToggle';

function Shell() {
  const { enabled } = useCapabilities();
  const { state } = useTenant();
  return (
    <div className="app-shell">
      <header className="app-header">
        <div className="brand-heading">
          <span className="brand-mark" aria-hidden="true">P</span>
          <div>
            <span className="eyebrow">Profile Operations</span>
            <h1>Workspace</h1>
          </div>
        </div>
        <div className="header-controls"><TenantChooser /><ThemeToggle /></div>
      </header>
      <NetworkStatusBanner />
      <nav className="primary-nav" aria-label="Primary navigation">
        <Link to="/" activeOptions={{ exact: true }}>Dashboard</Link>
        {state === 'ready' ? (
          <>
            {enabled('clients') ? <Link to="/clients">Clients</Link> : null}
            {enabled('browser_profiles') ? <Link to="/profiles">Profiles</Link> : null}
            {enabled('identity') ? <Link to="/users">Users & access</Link> : null}
            {enabled('mailbox_admin') ? <Link to="/mailboxes">Mailboxes</Link> : null}
            <Link to="/sessions">Sessions</Link>
            {enabled('profile_runtime') ? <Link to="/devices">Devices</Link> : null}
            <Link to="/audit">Audit</Link>
            <Link to="/settings">Settings</Link>
          </>
        ) : null}
      </nav>
      <main id="main-content"><Outlet /></main>
      <footer><span>Profile Operations · Secure workspace</span><span>Technical details are available in the relevant settings.</span></footer>
    </div>
  );
}

function Dashboard() {
  const { enabled } = useCapabilities();
  const { state } = useTenant();
  if (state !== 'ready') return <OrganizationSetup />;
  return (
    <div className="page-stack">
      <section className="hero panel">
        <span className="eyebrow">Overview</span>
        <h2>Everything you need to get started.</h2>
        <p>Choose a client, open a browser profile, or check an active session.</p>
      </section>
      <SessionPanel />
      <section className="workspace-grid">
        {enabled('clients') ? <article className="panel navigation-card"><h3>Clients</h3><p>People, organizations and their browser profiles.</p><Link to="/clients">View clients →</Link></article> : null}
        {enabled('browser_profiles') ? <article className="panel navigation-card"><h3>Browser profiles</h3><p>Open, organize and check your profiles.</p><Link to="/profiles">View profiles →</Link></article> : null}
        {enabled('mailbox_admin') ? <article className="panel navigation-card"><h3>Mailboxes</h3><p>Connected mailboxes and account status.</p><Link to="/mailboxes">View mailboxes →</Link></article> : null}
        {enabled('identity') ? <article className="panel navigation-card"><h3>Administration</h3><p>Access, devices, security and diagnostics.</p><Link to="/users">View administration →</Link></article> : null}
      </section>
    </div>
  );
}

function MailboxesRouteBoundary() {
  return (
    <CapabilityBoundary unit="mailbox_admin">
      <Outlet />
    </CapabilityBoundary>
  );
}

const rootRoute = createRootRoute({ component: Shell });
const indexRoute = createRoute({ getParentRoute: () => rootRoute, path: '/', component: Dashboard });
const clientsRoutes = createClientsRoutes(rootRoute);
const profilesRoutes = createProfilesRoutes(rootRoute);
const mailboxesBoundaryRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: 'mailboxes-capability-boundary',
  component: MailboxesRouteBoundary,
});
const mailboxesRoute = createMailboxesRoute(mailboxesBoundaryRoute);
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
  mailboxesBoundaryRoute.addChildren([mailboxesRoute]),
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
