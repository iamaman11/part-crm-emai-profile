import { Link, Outlet, createRootRoute, createRoute, createRouter } from '@tanstack/react-router';
import { TenantChooser } from './TenantContext';
import { createAccessRoute } from '../features/access';
import { createClientsRoute } from '../features/clients';
import { createMailboxesRoute } from '../features/mailboxes';
import { createProfilesRoute } from '../features/profiles';
import { SessionPanel } from '../features/session';

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
      <nav className="primary-nav" aria-label="Primary navigation">
        <Link to="/" activeOptions={{ exact: true }}>Dashboard</Link>
        <Link to="/clients">Clients</Link>
        <Link to="/profiles">Profiles</Link>
        <Link to="/mailboxes">Mailboxes</Link>
        <Link to="/users">Users & access</Link>
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
        <h2>Explicit-resource workflows, fail-closed by default.</h2>
        <p>This UI intentionally does not invent catalog list endpoints. Use opaque IDs to resolve resources the Worker authorizes for the active actor.</p>
      </section>
      <SessionPanel />
      <section className="workspace-grid">
        <article className="panel"><h3>Clients</h3><p>Create or resolve a client by ID, then manage explicit client grants.</p><Link to="/clients">Open clients</Link></article>
        <article className="panel"><h3>Profiles</h3><p>Resolve profile state, assignment, grants, generations and coordinator projection.</p><Link to="/profiles">Open profiles</Link></article>
        <article className="panel"><h3>Mailboxes</h3><p>Operate metadata-only bindings and bounded jobs without exposing mailbox payloads.</p><Link to="/mailboxes">Open mailboxes</Link></article>
      </section>
    </div>
  );
}

const rootRoute = createRootRoute({ component: Shell });
const indexRoute = createRoute({ getParentRoute: () => rootRoute, path: '/', component: Dashboard });
const clientsRoute = createClientsRoute(rootRoute);
const profilesRoute = createProfilesRoute(rootRoute);
const mailboxesRoute = createMailboxesRoute(rootRoute);
const usersRoute = createAccessRoute(rootRoute);

const routeTree = rootRoute.addChildren([indexRoute, clientsRoute, profilesRoute, mailboxesRoute, usersRoute]);
export const router = createRouter({ routeTree, defaultPreload: 'intent' });

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router;
  }
}
