import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { MailboxDirectory } from './MailboxDirectory';
import { MailboxesWorkspace } from './MailboxesWorkspace';

function MailboxesPage() {
  return (
    <div className="page-stack">
      <MailboxDirectory />
      <MailboxesWorkspace />
    </div>
  );
}

export function createMailboxesRoute(parentRoute: AnyRoute) {
  return createRoute({
    getParentRoute: () => parentRoute,
    path: '/mailboxes',
    component: MailboxesPage,
  });
}
