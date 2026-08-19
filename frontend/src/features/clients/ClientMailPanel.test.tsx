import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TenantProvider } from '../../app/TenantContext';
import { getMailboxClientAssociation, listMailboxes } from '../mailboxes';
import { ClientMailPanel } from './ClientMailPanel';

vi.mock('../mailboxes', () => ({
  getMailboxClientAssociation: vi.fn(),
  listMailboxes: vi.fn(),
}));

vi.mock('./api', () => ({
  getClientMailMessage: vi.fn(),
  searchClientMail: vi.fn(),
  sendClientMail: vi.fn(),
}));

const mockedListMailboxes = vi.mocked(listMailboxes);
const mockedAssociation = vi.mocked(getMailboxClientAssociation);

function renderPanel(outboundMailEnabled = true) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <TenantProvider>
        <ClientMailPanel
          clientId="client_current"
          outboundMailEnabled={outboundMailEnabled}
        />
      </TenantProvider>
    </QueryClientProvider>,
  );
}

describe('ClientMailPanel mailbox scoping', () => {
  beforeEach(() => {
    window.history.replaceState(null, '', '/clients/client_current?tenant=tenant_current');
    mockedListMailboxes.mockResolvedValue({
      mailboxes: [
        { bindingId: 'binding_current', provider: 'GMAIL_API', status: 'ACTIVE', version: 1 },
        { bindingId: 'binding_foreign', provider: 'IMAP', status: 'ACTIVE', version: 1 },
        { bindingId: 'binding_auth', provider: 'GMAIL_API', status: 'AUTH_REQUIRED', version: 1 },
        { bindingId: 'binding_browser', provider: 'BROWSER_FALLBACK', status: 'ACTIVE', version: 1 },
      ],
      nextCursor: null,
    });
    mockedAssociation.mockImplementation(async (_tenantId, bindingId) => {
      switch (bindingId) {
        case 'binding_current':
          return {
            bindingId,
            canManage: true,
            clientId: 'client_current',
            mailboxExecutable: true,
            relationshipVersion: 1,
          };
        case 'binding_foreign':
          return {
            bindingId,
            canManage: true,
            clientId: 'client_foreign',
            mailboxExecutable: true,
            relationshipVersion: 1,
          };
        case 'binding_auth':
          return {
            bindingId,
            canManage: true,
            clientId: 'client_current',
            mailboxExecutable: false,
            relationshipVersion: 1,
          };
        default:
          return undefined;
      }
    });
  });

  it('shows only executable mailboxes associated with the current client', async () => {
    const user = userEvent.setup();
    renderPanel();

    const searchMailbox = await screen.findByLabelText('Current-client mailbox');
    await waitFor(() => {
      expect(within(searchMailbox).getByRole('option', { name: 'binding_current · GMAIL_API' })).toBeTruthy();
    });
    const options = within(searchMailbox).getAllByRole('option').map((option) => option.textContent);
    expect(options).toEqual(['Select mailbox', 'binding_current · GMAIL_API']);
    expect(screen.queryByText(/binding_foreign/u)).toBeNull();
    expect(mockedAssociation).not.toHaveBeenCalledWith('tenant_current', 'binding_browser');

    expect(screen.getByText(/require re-authentication/u)).toBeTruthy();
    expect(screen.getByRole('link', { name: 'Mailboxes' }).getAttribute('href')).toBe('/mailboxes');

    await user.click(screen.getByRole('button', { name: 'Compose' }));
    const from = screen.getByLabelText('From');
    const fromOptions = within(from).getAllByRole('option').map((option) => option.textContent);
    expect(fromOptions).toEqual(['Select eligible mailbox', 'binding_current · GMAIL_API']);
    expect(mockedListMailboxes).toHaveBeenCalledTimes(1);
  });

  it('keeps read/search UI available while outbound mail is disabled', async () => {
    renderPanel(false);

    expect(await screen.findByLabelText('Current-client mailbox')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Search mailbox' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Compose' })).toBeNull();
    expect(screen.getByText(/Outbound mail is disabled by the active release profile/u)).toBeTruthy();
  });
});
