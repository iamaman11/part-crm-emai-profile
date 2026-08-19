import { useMutation, useQuery } from '@tanstack/react-query';
import { useState, type FormEvent } from 'react';
import { useTenant } from '../../app/TenantContext';
import type { ClientMailSendOperationDto } from '../../shared/api/generated/client-mail-send';
import type { MailboxClientAssociationProjectionDto } from '../../shared/api/generated/mailbox-client-association';
import type { MailboxListItemDto } from '../../shared/api/generated/operator-query';
import type {
  ClientMailSearchInput,
  MailMessageBodyDto,
  MailboxMessageReferenceDto,
} from '../../shared/api/generated/query-mail';
import { SafeMailBody } from '../../shared/mail/SafeMailBody';
import { StatusMessage } from '../../shared/ui/StatusMessage';
import { getMailboxClientAssociation, listMailboxes } from '../mailboxes';
import { getClientMailMessage, searchClientMail } from './api';
import {
  ClientMailComposer,
  type ClientMailComposerMailbox,
} from './ClientMailComposer';

type Props = {
  clientId: string;
  outboundMailEnabled: boolean;
};

type RelatedMailbox = {
  mailbox: MailboxListItemDto;
  association: MailboxClientAssociationProjectionDto;
};

type ComposerState = {
  operation: ClientMailSendOperationDto;
  source: MailMessageBodyDto | null;
};

const MAX_MAILBOX_PAGES = 10;
const MAILBOX_PAGE_SIZE = 100;

function field(form: FormData, name: string): string {
  return String(form.get(name) ?? '').trim();
}

async function loadRelatedMailboxes(
  tenantId: string,
  clientId: string,
  signal: AbortSignal,
): Promise<RelatedMailbox[]> {
  const mailboxes: MailboxListItemDto[] = [];
  let cursor: string | null = null;

  for (let pageNumber = 0; pageNumber < MAX_MAILBOX_PAGES; pageNumber += 1) {
    const page = await listMailboxes(tenantId, signal, cursor, MAILBOX_PAGE_SIZE);
    if (!page) break;
    mailboxes.push(...page.mailboxes);
    cursor = page.nextCursor;
    if (!cursor) break;
    if (pageNumber === MAX_MAILBOX_PAGES - 1) {
      throw new Error('Mailbox list exceeded the bounded Client Mail read window.');
    }
  }

  const cloudMailboxes = mailboxes.filter(
    (mailbox) => mailbox.provider === 'GMAIL_API' || mailbox.provider === 'IMAP',
  );
  const related = await Promise.all(
    cloudMailboxes.map(async (mailbox) => {
      const association = await getMailboxClientAssociation(tenantId, mailbox.bindingId);
      if (!association || association.clientId !== clientId) return null;
      return { mailbox, association } satisfies RelatedMailbox;
    }),
  );
  return related.filter((item): item is RelatedMailbox => item !== null);
}

export function ClientMailPanel({ clientId, outboundMailEnabled }: Props) {
  const { tenantId } = useTenant();
  const [lastInput, setLastInput] = useState<ClientMailSearchInput | null>(null);
  const [composer, setComposer] = useState<ComposerState | null>(null);
  const mailboxQuery = useQuery({
    queryKey: ['client-mail', tenantId, clientId, 'mailboxes'],
    queryFn: ({ signal }) => loadRelatedMailboxes(tenantId, clientId, signal),
    enabled: tenantId.length > 0 && clientId.length > 0,
  });
  const search = useMutation({
    mutationFn: (input: ClientMailSearchInput) => searchClientMail(tenantId, clientId, input),
  });
  const message = useMutation({
    mutationFn: (reference: MailboxMessageReferenceDto) =>
      getClientMailMessage(tenantId, clientId, reference),
  });

  const relatedMailboxes = mailboxQuery.data ?? [];
  const eligibleMailboxes = relatedMailboxes.filter(
    ({ mailbox, association }) => mailbox.status === 'ACTIVE' && association.mailboxExecutable,
  );
  const authRequiredMailboxes = relatedMailboxes.filter(
    ({ mailbox }) => mailbox.status === 'AUTH_REQUIRED',
  );
  const composerMailboxes: ClientMailComposerMailbox[] = eligibleMailboxes.map(({ mailbox }) => ({
    bindingId: mailbox.bindingId,
    provider: mailbox.provider,
  }));
  const page = search.data;

  function execute(input: ClientMailSearchInput) {
    setLastInput(input);
    message.reset();
    search.mutate(input);
  }

  function openComposer(operation: ClientMailSendOperationDto, source: MailMessageBodyDto | null) {
    if (!outboundMailEnabled) return;
    setComposer({ operation, source });
  }

  return (
    <section className="panel full-span" aria-labelledby="client-mail-title">
      <span className="eyebrow">Client → Mail</span>
      <h2 id="client-mail-title">Mail</h2>
      <p>
        Search, message bodies, and drafts remain transient in browser memory. Confidential terms,
        provider references, and message content are sent only in authenticated request bodies and
        are not persisted in Web Storage or telemetry.
      </p>

      <StatusMessage state={mailboxQuery.error ?? null} />
      {outboundMailEnabled ? (
        <button
          type="button"
          disabled={mailboxQuery.isPending || composerMailboxes.length === 0}
          onClick={() => openComposer('NEW', null)}
        >
          Compose
        </button>
      ) : (
        <p className="muted">Outbound mail is disabled by the active release profile.</p>
      )}

      {authRequiredMailboxes.length > 0 && (
        <p className="muted">
          {authRequiredMailboxes.length} mailbox{authRequiredMailboxes.length === 1 ? '' : 'es'} associated with this client require re-authentication. Restore authorization in{' '}
          <a href="/mailboxes">Mailboxes</a>; credentials and secret handles are never shown here.
        </p>
      )}

      {!mailboxQuery.isPending && composerMailboxes.length === 0 && (
        <p className="muted">
          No active Gmail API or IMAP mailbox is executable for this client. Browser-fallback mailbox execution remains device/Bridge-owned rather than being impersonated by this web operator surface.
        </p>
      )}

      {outboundMailEnabled && composer && (
        <ClientMailComposer
          key={`${composer.operation}:${composer.source?.summary.reference.providerReference ?? 'new'}`}
          tenantId={tenantId}
          clientId={clientId}
          operation={composer.operation}
          source={composer.source}
          mailboxes={composerMailboxes}
          onClose={() => setComposer(null)}
        />
      )}

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
        <label htmlFor="client-mail-binding">Current-client mailbox</label>
        <select
          id="client-mail-binding"
          name="mailboxBindingId"
          required
          disabled={mailboxQuery.isPending || eligibleMailboxes.length === 0}
        >
          <option value="">Select mailbox</option>
          {eligibleMailboxes.map(({ mailbox }) => (
            <option key={mailbox.bindingId} value={mailbox.bindingId}>
              {mailbox.bindingId} · {mailbox.provider}
            </option>
          ))}
        </select>
        <label htmlFor="client-mail-term">Search term</label>
        <input id="client-mail-term" name="term" maxLength={200} autoComplete="off" />
        <button type="submit" disabled={search.isPending || eligibleMailboxes.length === 0}>
          {search.isPending ? 'Searching…' : 'Search mailbox'}
        </button>
      </form>

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
          {outboundMailEnabled ? (
            <div>
              <button type="button" onClick={() => openComposer('REPLY', message.data ?? null)}>
                Reply
              </button>{' '}
              <button type="button" onClick={() => openComposer('REPLY_ALL', message.data ?? null)}>
                Reply all
              </button>{' '}
              <button type="button" onClick={() => openComposer('FORWARD', message.data ?? null)}>
                Forward
              </button>
            </div>
          ) : null}
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
