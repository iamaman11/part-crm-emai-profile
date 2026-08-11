import { useMutation, useQuery } from '@tanstack/react-query';
import { useState, type FormEvent } from 'react';
import { useTenant } from '../../app/TenantContext';
import { getClientMailMessage, searchClientMail } from '../../shared/api/clientMail';
import { listMailboxes } from '../../shared/api/endpoints';
import type {
  ClientMailSearchInput,
  MailboxMessageReferenceDto,
} from '../../shared/api/generated/query-mail';
import { SafeMailBody } from '../../shared/mail/SafeMailBody';
import { StatusMessage } from '../../shared/ui/StatusMessage';

type Props = {
  clientId: string;
};

function field(form: FormData, name: string): string {
  return String(form.get(name) ?? '').trim();
}

export function ClientMailPanel({ clientId }: Props) {
  const { tenantId } = useTenant();
  const [lastInput, setLastInput] = useState<ClientMailSearchInput | null>(null);
  const mailboxQuery = useQuery({
    queryKey: ['operator-query', tenantId, 'mailboxes'],
    queryFn: ({ signal }) => listMailboxes(tenantId, signal),
    enabled: tenantId.length > 0,
  });
  const search = useMutation({
    mutationFn: (input: ClientMailSearchInput) => searchClientMail(tenantId, clientId, input),
  });
  const message = useMutation({
    mutationFn: (reference: MailboxMessageReferenceDto) =>
      getClientMailMessage(tenantId, clientId, reference),
  });

  const cloudMailboxes = (mailboxQuery.data?.mailboxes ?? []).filter(
    (mailbox) => mailbox.status === 'ACTIVE' && mailbox.provider !== 'BROWSER_FALLBACK',
  );
  const page = search.data;

  function execute(input: ClientMailSearchInput) {
    setLastInput(input);
    message.reset();
    search.mutate(input);
  }

  return (
    <section className="panel full-span" aria-labelledby="client-mail-title">
      <span className="eyebrow">Client → Mail</span>
      <h2 id="client-mail-title">Authorized mailbox query</h2>
      <p>
        Searches and full message bodies are transient. The browser sends confidential search terms
        and provider references in authenticated POST bodies rather than URLs, and does not persist
        them in Web Storage or telemetry.
      </p>
      <StatusMessage state={mailboxQuery.error ?? null} />
      <form
        className="stack-form"
        onSubmit={(event: FormEvent<HTMLFormElement>) => {
          event.preventDefault();
          const data = new FormData(event.currentTarget);
          execute({
            mailboxBindingId: field(data, 'mailboxBindingId'),
            term: field(data, 'term') || null,
            cursor: null,
            limit: 25,
          });
        }}
      >
        <label htmlFor="client-mail-binding">Active cloud mailbox</label>
        <select
          id="client-mail-binding"
          name="mailboxBindingId"
          required
          disabled={mailboxQuery.isPending || cloudMailboxes.length === 0}
        >
          <option value="">Select mailbox</option>
          {cloudMailboxes.map((mailbox) => (
            <option key={mailbox.bindingId} value={mailbox.bindingId}>
              {mailbox.bindingId} · {mailbox.provider}
            </option>
          ))}
        </select>
        <label htmlFor="client-mail-term">Search term</label>
        <input id="client-mail-term" name="term" maxLength={200} autoComplete="off" />
        <button type="submit" disabled={search.isPending || cloudMailboxes.length === 0}>
          {search.isPending ? 'Searching…' : 'Search mailbox'}
        </button>
      </form>

      {!mailboxQuery.isPending && cloudMailboxes.length === 0 && (
        <p className="muted">
          No active Gmail API or IMAP mailbox is visible. Browser-fallback mailbox execution remains
          device/Bridge-owned rather than being impersonated by this web operator surface.
        </p>
      )}
      <StatusMessage state={search.error ?? message.error ?? null} />

      {page && page.messages.length === 0 && <p>No authorized messages matched this bounded query.</p>}
      {page && page.messages.length > 0 && (
        <div className="table-scroll">
          <table>
            <thead>
              <tr>
                <th scope="col">Received</th>
                <th scope="col">Sender</th>
                <th scope="col">Subject</th>
                <th scope="col">Body</th>
              </tr>
            </thead>
            <tbody>
              {page.messages.map((item) => (
                <tr key={`${item.reference.mailboxBindingId}:${item.reference.providerReference}`}>
                  <td>{new Date(item.receivedAtMs).toLocaleString()}</td>
                  <td>{item.sender ?? 'Unknown sender'}</td>
                  <td>{item.subject ?? '(no subject)'}</td>
                  <td>
                    <button
                      type="button"
                      disabled={message.isPending}
                      onClick={() => message.mutate(item.reference)}
                    >
                      Open safely
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {page?.nextCursor && lastInput && (
        <button
          type="button"
          disabled={search.isPending}
          onClick={() => execute({ ...lastInput, cursor: page.nextCursor })}
        >
          Load next page
        </button>
      )}

      {message.data && (
        <article className="mail-message-detail" aria-live="polite">
          <h3>{message.data.summary.subject ?? '(no subject)'}</h3>
          <p className="muted">
            {message.data.summary.sender ?? 'Unknown sender'} ·{' '}
            {new Date(message.data.summary.receivedAtMs).toLocaleString()}
          </p>
          <SafeMailBody
            textBody={message.data.textBody}
            htmlBody={message.data.htmlBody}
            title={message.data.summary.subject ?? 'Mail message body'}
          />
        </article>
      )}
    </section>
  );
}
