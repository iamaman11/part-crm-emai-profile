import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TenantProvider, useTenant } from '../../app/TenantContext';
import { getMailboxClientAssociation, listMailboxes } from '../mailboxes';
import { getClientMailMessage, searchClientMail } from './api';
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
const mockedSearch = vi.mocked(searchClientMail);
const mockedMessage = vi.mocked(getClientMailMessage);

function ChangeTenantForTest() {
  const { selectTenant } = useTenant();
  return <button type="button" onClick={() => selectTenant('tenant_other')}>Switch organization</button>;
}

function renderPanel(outboundMailEnabled = true, initialClientId = 'client_current') {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const loadTenantContexts = async () => ({
    tenants: [
      { tenantId: 'tenant_current', displayName: 'Current organization', actorId: 'actor_current', role: 'TENANT_OWNER' as const },
      { tenantId: 'tenant_other', displayName: 'Other organization', actorId: 'actor_other', role: 'TENANT_OWNER' as const },
    ],
  });
  const panel = (clientId: string) => (
    <QueryClientProvider client={queryClient}>
      <TenantProvider loadTenantContexts={loadTenantContexts}>
        <ChangeTenantForTest />
        <ClientMailPanel
          clientId={clientId}
          outboundMailEnabled={outboundMailEnabled}
        />
      </TenantProvider>
    </QueryClientProvider>
  );
  const mounted = render(panel(initialClientId));
  return { ...mounted, rerenderClient: (clientId: string) => mounted.rerender(panel(clientId)) };
}

describe('ClientMailPanel mailbox scoping', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    window.history.replaceState(null, '', '/clients/client_current?tenant=tenant_current');
    mockedListMailboxes.mockResolvedValue({
      mailboxes: [
        { bindingId: 'binding_current', provider: 'GMAIL_API', status: 'ACTIVE', version: 1 },
        { bindingId: 'binding_graph', provider: 'MICROSOFT_GRAPH', status: 'ACTIVE', version: 1 },
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
        case 'binding_graph':
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
          throw new Error(`unexpected mailbox association lookup: ${bindingId}`);
      }
    });
  });

  it('shows only executable mailboxes associated with the current client', async () => {
    const user = userEvent.setup();
    renderPanel();

    await screen.findByLabelText('Current-client mailbox');
    await waitFor(() => {
      expect(within(screen.getByLabelText('Current-client mailbox')).getByRole('option', { name: 'binding_current · GMAIL_API' })).toBeTruthy();
    });
    const searchMailbox = screen.getByLabelText('Current-client mailbox');
    const options = within(searchMailbox).getAllByRole('option').map((option) => option.textContent);
    expect(options).toEqual([
      'Select mailbox',
      'binding_current · GMAIL_API',
      'binding_graph · MICROSOFT_GRAPH',
    ]);
    expect(screen.queryByText(/binding_foreign/u)).toBeNull();
    expect(mockedAssociation).not.toHaveBeenCalledWith('tenant_current', 'binding_browser');

    expect(screen.getByText(/require re-authentication/u)).toBeTruthy();
    expect(screen.getByRole('link', { name: 'Mailboxes' }).getAttribute('href')).toBe('/mailboxes');

    await user.click(screen.getByRole('button', { name: 'Compose' }));
    const from = screen.getByLabelText('From');
    const fromOptions = within(from).getAllByRole('option').map((option) => option.textContent);
    expect(fromOptions).toEqual([
      'Select eligible mailbox',
      'binding_current · GMAIL_API',
      'binding_graph · MICROSOFT_GRAPH',
    ]);
    expect(mockedListMailboxes).toHaveBeenCalledTimes(1);
  });

  it('keeps read/search UI available while outbound mail is disabled', async () => {
    renderPanel(false);

    expect(await screen.findByLabelText('Current-client mailbox')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Search mailbox' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Compose' })).toBeNull();
    expect(screen.getByText(/Outbound mail is disabled by the active release profile/u)).toBeTruthy();
  });

  it('does not carry search results, opened messages or search terms across clients', async () => {
    const user = userEvent.setup();
    mockedSearch.mockResolvedValueOnce({
      messages: [{
        reference: { mailboxBindingId: 'binding_current', providerReference: 'private_message' },
        receivedAtMs: 1760000000000,
        sender: 'private@example.test',
        subject: 'Private client subject',
      }],
      nextCursor: 'private_cursor',
    });
    mockedMessage.mockResolvedValueOnce({
      summary: {
        reference: { mailboxBindingId: 'binding_current', providerReference: 'private_message' },
        receivedAtMs: 1760000000000,
        sender: 'private@example.test',
        subject: 'Private client subject',
      },
      textBody: 'Private client message body',
      htmlBody: null,
    });
    const panel = renderPanel();
    await waitFor(() => expect(within(screen.getByLabelText('Current-client mailbox')).queryByRole('option', { name: 'binding_current · GMAIL_API' })).toBeTruthy());
    await user.selectOptions(screen.getByLabelText('Current-client mailbox'), 'binding_current');
    await user.type(screen.getByLabelText('Search term'), 'private query');
    await user.click(screen.getByRole('button', { name: 'Search mailbox' }));
    expect(await screen.findByText('Private client subject')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Load next page' })).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Open safely' }));
    expect(await screen.findByText('Private client message body')).toBeTruthy();

    panel.rerenderClient('client_foreign');
    expect(screen.queryByText('Private client subject')).toBeNull();
    expect(screen.queryByText('Private client message body')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Load next page' })).toBeNull();
    expect((screen.getByLabelText('Search term') as HTMLInputElement).value).toBe('');
    await waitFor(() => {
      expect(within(screen.getByLabelText('Current-client mailbox')).queryByRole('option', { name: 'binding_foreign · IMAP' })).toBeTruthy();
    });
    expect(mockedSearch).toHaveBeenCalledWith('tenant_current', 'client_current', expect.objectContaining({ term: 'private query' }));
  });

  it('drops a transient draft when the organization changes', async () => {
    const user = userEvent.setup();
    renderPanel();
    await waitFor(() => expect(within(screen.getByLabelText('Current-client mailbox')).queryByRole('option', { name: 'binding_current · GMAIL_API' })).toBeTruthy());
    await user.click(screen.getByRole('button', { name: 'Compose' }));
    await user.type(screen.getByLabelText('To'), 'former-client@example.test');
    await user.type(screen.getByLabelText('Subject'), 'Former client private draft');
    await user.type(screen.getByLabelText('Message'), 'Former client private body');

    await user.click(screen.getByRole('button', { name: 'Switch organization' }));
    expect(screen.queryByLabelText('To')).toBeNull();
    expect(screen.queryByText('Former client private draft')).toBeNull();
    await waitFor(() => expect(mockedAssociation).toHaveBeenCalledWith('tenant_other', 'binding_current'));
    await user.click(screen.getByRole('button', { name: 'Compose' }));
    expect((screen.getByLabelText('To') as HTMLInputElement).value).toBe('');
    expect((screen.getByLabelText('Subject') as HTMLInputElement).value).toBe('');
    expect((screen.getByLabelText('Message') as HTMLTextAreaElement).value).toBe('');
  });

  it('ignores a search reply that arrives after switching clients', async () => {
    const user = userEvent.setup();
    let resolvePriorSearch: ((value: Awaited<ReturnType<typeof searchClientMail>>) => void) | undefined;
    mockedSearch.mockImplementationOnce(() => new Promise((resolve) => { resolvePriorSearch = resolve; }));
    const panel = renderPanel();
    await waitFor(() => expect(within(screen.getByLabelText('Current-client mailbox')).queryByRole('option', { name: 'binding_current · GMAIL_API' })).toBeTruthy());
    await user.selectOptions(screen.getByLabelText('Current-client mailbox'), 'binding_current');
    await user.click(screen.getByRole('button', { name: 'Search mailbox' }));
    expect(mockedSearch).toHaveBeenCalledTimes(1);
    panel.rerenderClient('client_foreign');

    await act(async () => resolvePriorSearch?.({
      messages: [{
        reference: { mailboxBindingId: 'binding_current', providerReference: 'late_private' },
        receivedAtMs: 1760000000000,
        sender: 'private@example.test',
        subject: 'Late private client subject',
      }],
      nextCursor: null,
    }));
    expect(screen.queryByText('Late private client subject')).toBeNull();
    await waitFor(() => expect(within(screen.getByLabelText('Current-client mailbox')).queryByRole('option', { name: 'binding_foreign · IMAP' })).toBeTruthy());
  });
});
