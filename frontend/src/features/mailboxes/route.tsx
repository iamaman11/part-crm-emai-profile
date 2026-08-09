import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { MailboxesWorkspace } from './MailboxesWorkspace';

export function createMailboxesRoute(parentRoute: AnyRoute) {
  return createRoute({
    getParentRoute: () => parentRoute,
    path: '/mailboxes',
    component: MailboxesWorkspace,
  });
}
