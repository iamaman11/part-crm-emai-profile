import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { ClientMailSendReceiptDto } from '../../shared/api/generated/client-mail-send';
import type { MailMessageBodyDto } from '../../shared/api/generated/query-mail';
import { sendClientMail } from './api';
import { ClientMailComposer } from './ClientMailComposer';

vi.mock('./api', () => ({
  sendClientMail: vi.fn(),
}));

const mockedSendClientMail = vi.mocked(sendClientMail);
const mailboxes = [{ bindingId: 'binding_01JMAILSEND', provider: 'GMAIL_API' as const }];
const source: MailMessageBodyDto = {
  htmlBody: null,
  textBody: 'Source body',
  summary: {
    receivedAtMs: 1,
    reference: {
      mailboxBindingId: 'binding_01JMAILSEND',
      providerReference: 'gmail:source-message-1',
    },
    sender: 'sender@example.test',
    subject: 'Source subject',
  },
};

function receipt(state: ClientMailSendReceiptDto['state']): ClientMailSendReceiptDto {
  return {
    intentId: 'intent_01JMAILSEND',
    state,
    attemptCount: 1,
    replayed: false,
  };
}

function renderComposer(
  operation: 'NEW' | 'REPLY' | 'REPLY_ALL' | 'FORWARD',
  message: MailMessageBodyDto | null = null,
) {
  const queryClient = new QueryClient({
    defaultOptions: { mutations: { retry: false }, queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <ClientMailComposer
        tenantId="tenant_01JMAILSEND"
        clientId="client_01JMAILSEND"
        operation={operation}
        source={message}
        mailboxes={mailboxes}
        onClose={vi.fn()}
      />
    </QueryClientProvider>,
  );
}

describe('ClientMailComposer', () => {
  it('builds a provider-neutral compose request from explicit recipients', async () => {
    mockedSendClientMail.mockResolvedValueOnce(receipt('SENT'));
    const user = userEvent.setup();
    renderComposer('NEW');

    await user.type(screen.getByLabelText('To'), 'client@example.test');
    await user.type(screen.getByLabelText('Message'), 'Hello client');
    await user.click(screen.getByRole('button', { name: 'Send' }));

    await waitFor(() => expect(mockedSendClientMail).toHaveBeenCalledTimes(1));
    expect(mockedSendClientMail).toHaveBeenCalledWith(
      'tenant_01JMAILSEND',
      'client_01JMAILSEND',
      {
        mailboxBindingId: 'binding_01JMAILSEND',
        operation: 'NEW',
        sourceProviderReference: null,
        to: ['client@example.test'],
        cc: [],
        bcc: [],
        subject: null,
        textBody: 'Hello client',
        htmlBody: null,
      },
      expect.any(String),
    );
  });

  it.each(['REPLY', 'REPLY_ALL'] as const)(
    '%s keeps recipients server-owned and binds the source mailbox',
    async (operation) => {
      mockedSendClientMail.mockResolvedValueOnce(receipt('SENT'));
      const user = userEvent.setup();
      renderComposer(operation, source);

      expect(screen.queryByLabelText('To')).toBeNull();
      expect(screen.queryByLabelText('Cc')).toBeNull();
      expect(screen.queryByLabelText('Bcc')).toBeNull();
      await user.type(screen.getByLabelText('Message'), 'Reply body');
      await user.click(screen.getByRole('button', { name: 'Send' }));

      await waitFor(() => expect(mockedSendClientMail).toHaveBeenCalledTimes(1));
      expect(mockedSendClientMail).toHaveBeenCalledWith(
        'tenant_01JMAILSEND',
        'client_01JMAILSEND',
        expect.objectContaining({
          mailboxBindingId: 'binding_01JMAILSEND',
          operation,
          sourceProviderReference: 'gmail:source-message-1',
          to: [],
          cc: [],
          bcc: [],
          subject: null,
          textBody: 'Reply body',
        }),
        expect.any(String),
      );
    },
  );

  it('forwards from the source mailbox with explicit deduplicated recipients', async () => {
    mockedSendClientMail.mockResolvedValueOnce(receipt('SENT'));
    const user = userEvent.setup();
    renderComposer('FORWARD', source);

    expect((screen.getByLabelText('From') as HTMLSelectElement).disabled).toBe(true);
    expect((screen.getByLabelText('Subject') as HTMLInputElement).value).toBe('Fwd: Source subject');
    await user.type(screen.getByLabelText('To'), 'client@example.test');
    await user.type(screen.getByLabelText('Cc'), 'CLIENT@example.test, other@example.test');
    await user.type(screen.getByLabelText('Message'), 'Forward body');
    await user.click(screen.getByRole('button', { name: 'Send' }));

    await waitFor(() => expect(mockedSendClientMail).toHaveBeenCalledTimes(1));
    expect(mockedSendClientMail).toHaveBeenCalledWith(
      'tenant_01JMAILSEND',
      'client_01JMAILSEND',
      expect.objectContaining({
        mailboxBindingId: 'binding_01JMAILSEND',
        operation: 'FORWARD',
        sourceProviderReference: 'gmail:source-message-1',
        to: ['client@example.test'],
        cc: ['other@example.test'],
        bcc: [],
        subject: 'Fwd: Source subject',
        textBody: 'Forward body',
      }),
      expect.any(String),
    );
  });

  it('retries RETRYABLE with the exact same protected request and idempotency key', async () => {
    mockedSendClientMail
      .mockResolvedValueOnce(receipt('RETRYABLE'))
      .mockResolvedValueOnce(receipt('SENT'));
    const user = userEvent.setup();
    renderComposer('NEW');

    await user.type(screen.getByLabelText('To'), 'client@example.test');
    await user.type(screen.getByLabelText('Message'), 'Retry-safe body');
    await user.click(screen.getByRole('button', { name: 'Send' }));

    const retry = await screen.findByRole('button', { name: 'Retry same protected attempt' });
    const firstCall = mockedSendClientMail.mock.calls[0];
    expect(firstCall).toBeDefined();
    await user.click(retry);
    await waitFor(() => expect(mockedSendClientMail).toHaveBeenCalledTimes(2));
    const secondCall = mockedSendClientMail.mock.calls[1];
    expect(secondCall).toBeDefined();
    expect(secondCall?.[2]).toEqual(firstCall?.[2]);
    expect(secondCall?.[3]).toBe(firstCall?.[3]);
  });

  it.each(['PENDING', 'DISPATCHING'] as const)(
    '%s locks the current attempt instead of exposing a duplicate send',
    async (state) => {
      mockedSendClientMail.mockResolvedValueOnce(receipt(state));
      const user = userEvent.setup();
      renderComposer('NEW');

      await user.type(screen.getByLabelText('To'), 'client@example.test');
      await user.type(screen.getByLabelText('Message'), 'Still processing');
      await user.click(screen.getByRole('button', { name: 'Send' }));

      expect(await screen.findByText(/still being processed/u)).toBeTruthy();
      expect((screen.getByRole('button', { name: 'Send' }) as HTMLButtonElement).disabled).toBe(true);
      expect((screen.getByLabelText('Message') as HTMLTextAreaElement).disabled).toBe(true);
      expect(screen.queryByRole('button', { name: 'Retry same protected attempt' })).toBeNull();
      expect(mockedSendClientMail).toHaveBeenCalledTimes(1);
    },
  );

  it('requires an edit after REJECTED before creating a new idempotent attempt', async () => {
    mockedSendClientMail
      .mockResolvedValueOnce(receipt('REJECTED'))
      .mockResolvedValueOnce(receipt('SENT'));
    const user = userEvent.setup();
    renderComposer('NEW');

    await user.type(screen.getByLabelText('To'), 'client@example.test');
    await user.type(screen.getByLabelText('Message'), 'Rejected body');
    await user.click(screen.getByRole('button', { name: 'Send' }));

    expect(await screen.findByText(/send was rejected/u)).toBeTruthy();
    const firstCall = mockedSendClientMail.mock.calls[0];
    expect(firstCall).toBeDefined();
    expect((screen.getByRole('button', { name: 'Send' }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByLabelText('Message') as HTMLTextAreaElement).disabled).toBe(false);

    await user.type(screen.getByLabelText('Message'), ' edited');
    expect((screen.getByRole('button', { name: 'Send' }) as HTMLButtonElement).disabled).toBe(false);
    await user.click(screen.getByRole('button', { name: 'Send' }));
    await waitFor(() => expect(mockedSendClientMail).toHaveBeenCalledTimes(2));
    const secondCall = mockedSendClientMail.mock.calls[1];
    expect(secondCall?.[3]).not.toBe(firstCall?.[3]);
  });

  it('never exposes retry after an AMBIGUOUS outcome', async () => {
    mockedSendClientMail.mockResolvedValueOnce(receipt('AMBIGUOUS'));
    const user = userEvent.setup();
    renderComposer('NEW');

    await user.type(screen.getByLabelText('To'), 'client@example.test');
    await user.type(screen.getByLabelText('Message'), 'Potentially sent body');
    await user.click(screen.getByRole('button', { name: 'Send' }));

    expect(await screen.findByText(/Delivery is uncertain/u)).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Retry same protected attempt' })).toBeNull();
    expect(screen.getByText(/uncontrolled resend could duplicate delivery/u)).toBeTruthy();
  });
});
