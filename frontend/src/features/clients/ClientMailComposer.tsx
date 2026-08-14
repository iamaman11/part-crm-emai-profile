import { useMutation } from '@tanstack/react-query';
import { useState, type FormEvent } from 'react';
import { newIdempotencyKey } from '../../shared/api/client';
import type {
  ClientMailSendOperationDto,
  ClientMailSendReceiptDto,
  ClientMailSendRequestDto,
} from '../../shared/api/generated/client-mail-send';
import type { OperatorMailboxProvider } from '../../shared/api/generated/operator-query';
import type { MailMessageBodyDto } from '../../shared/api/generated/query-mail';
import { StatusMessage } from '../../shared/ui/StatusMessage';
import { sendClientMail } from './api';

export interface ClientMailComposerMailbox {
  bindingId: string;
  provider: OperatorMailboxProvider;
}

type Props = {
  tenantId: string;
  clientId: string;
  operation: ClientMailSendOperationDto;
  source: MailMessageBodyDto | null;
  mailboxes: ReadonlyArray<ClientMailComposerMailbox>;
  onClose: () => void;
};

type SendAttempt = {
  idempotencyKey: string;
  request: ClientMailSendRequestDto;
};

type RecipientFields = {
  to: string[];
  cc: string[];
  bcc: string[];
};

const MAX_RECIPIENTS = 100;
const MAX_ADDRESS_LENGTH = 320;
const ADDRESS_SHAPE = /^[^\s@]+@[^\s@]+$/;

function isSourceOperation(operation: ClientMailSendOperationDto): boolean {
  return operation !== 'NEW';
}

function needsExplicitRecipients(operation: ClientMailSendOperationDto): boolean {
  return operation === 'NEW' || operation === 'FORWARD';
}

function operationLabel(operation: ClientMailSendOperationDto): string {
  switch (operation) {
    case 'NEW':
      return 'Compose';
    case 'REPLY':
      return 'Reply';
    case 'REPLY_ALL':
      return 'Reply all';
    case 'FORWARD':
      return 'Forward';
  }
}

function parseAddressList(raw: string, seen: Set<string>): string[] {
  const output: string[] = [];
  for (const token of raw.split(/[;,]/u)) {
    const address = token.trim();
    if (!address) continue;
    if (
      address.length > MAX_ADDRESS_LENGTH ||
      !ADDRESS_SHAPE.test(address) ||
      Array.from(address).some((character) => /[\u0000-\u001F\u007F]/u.test(character))
    ) {
      throw new Error(`Invalid recipient address: ${address}`);
    }
    const canonical = address.toLocaleLowerCase('en-US');
    if (seen.has(canonical)) continue;
    seen.add(canonical);
    output.push(address);
  }
  return output;
}

function parseRecipients(to: string, cc: string, bcc: string): RecipientFields {
  const seen = new Set<string>();
  const recipients = {
    to: parseAddressList(to, seen),
    cc: parseAddressList(cc, seen),
    bcc: parseAddressList(bcc, seen),
  };
  const count = recipients.to.length + recipients.cc.length + recipients.bcc.length;
  if (count === 0) throw new Error('At least one recipient is required.');
  if (count > MAX_RECIPIENTS) throw new Error(`A maximum of ${MAX_RECIPIENTS} recipients is allowed.`);
  return recipients;
}

function receiptMessage(receipt: ClientMailSendReceiptDto | undefined): string | null {
  if (!receipt) return null;
  switch (receipt.state) {
    case 'PENDING':
    case 'DISPATCHING':
      return 'This send attempt is still being processed. Do not create a duplicate send.';
    case 'RETRYABLE':
      return 'The provider confirmed that this attempt was not sent. Retrying this protected attempt is safe.';
    case 'SENT':
      return receipt.replayed
        ? 'Sent. The existing idempotent send receipt was replayed without another provider send.'
        : 'Sent successfully.';
    case 'AMBIGUOUS':
      return 'Delivery is uncertain. Do not resend from this screen; verify delivery or reconcile the mailbox first.';
    case 'REJECTED':
      return 'The send was rejected. Change the mailbox or message fields before creating a new protected attempt.';
  }
}

export function ClientMailComposer({
  tenantId,
  clientId,
  operation,
  source,
  mailboxes,
  onClose,
}: Props) {
  const sourceBindingId = source?.summary.reference.mailboxBindingId ?? '';
  const sourceEligible = !isSourceOperation(operation) || mailboxes.some((mailbox) => mailbox.bindingId === sourceBindingId);
  const [mailboxBindingId, setMailboxBindingId] = useState(
    isSourceOperation(operation) ? sourceBindingId : (mailboxes[0]?.bindingId ?? ''),
  );
  const [to, setTo] = useState('');
  const [cc, setCc] = useState('');
  const [bcc, setBcc] = useState('');
  const [subject, setSubject] = useState(
    operation === 'FORWARD' && source?.summary.subject ? `Fwd: ${source.summary.subject}` : '',
  );
  const [textBody, setTextBody] = useState('');
  const [validation, setValidation] = useState<string | null>(null);
  const [attempt, setAttempt] = useState<SendAttempt | null>(null);

  const send = useMutation({
    mutationFn: (next: SendAttempt) =>
      sendClientMail(tenantId, clientId, next.request, next.idempotencyKey),
  });

  function invalidateAttempt() {
    setAttempt(null);
    setValidation(null);
    send.reset();
  }

  function buildRequest(): ClientMailSendRequestDto {
    if (!mailboxBindingId) throw new Error('Select an eligible mailbox.');
    if (isSourceOperation(operation) && (!source || !sourceEligible)) {
      throw new Error('The source mailbox is not currently eligible for this client.');
    }
    if (textBody.length === 0) throw new Error('Message body is required.');

    const recipients = needsExplicitRecipients(operation)
      ? parseRecipients(to, cc, bcc)
      : { to: [], cc: [], bcc: [] };

    return {
      mailboxBindingId,
      operation,
      sourceProviderReference: isSourceOperation(operation)
        ? (source?.summary.reference.providerReference ?? null)
        : null,
      to: recipients.to,
      cc: recipients.cc,
      bcc: recipients.bcc,
      subject: operation === 'REPLY' || operation === 'REPLY_ALL' ? null : (subject.trim() || null),
      textBody,
      htmlBody: null,
    };
  }

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    try {
      const request = buildRequest();
      const next = { request, idempotencyKey: newIdempotencyKey() };
      setValidation(null);
      setAttempt(next);
      send.mutate(next);
    } catch (error) {
      setValidation(error instanceof Error ? error.message : 'Invalid message.');
    }
  }

  const receipt = send.data;
  const canRetryReceipt = receipt?.state === 'RETRYABLE' && attempt !== null;
  const canRetryTransport = send.isError && attempt !== null;
  const lockedAttempt =
    receipt?.state === 'PENDING' ||
    receipt?.state === 'DISPATCHING' ||
    receipt?.state === 'SENT' ||
    receipt?.state === 'AMBIGUOUS';
  const rejectedNeedsEdit = receipt?.state === 'REJECTED';
  const fieldsDisabled = send.isPending || lockedAttempt;

  return (
    <section className="mail-message-detail" aria-labelledby="client-mail-compose-title">
      <div className="page-header">
        <div>
          <span className="eyebrow">Client → Mail</span>
          <h3 id="client-mail-compose-title">{operationLabel(operation)}</h3>
        </div>
        <button type="button" onClick={onClose}>Close composer</button>
      </div>

      {isSourceOperation(operation) && source && (
        <p className="muted">
          Source: {source.summary.subject ?? '(no subject)'} · {source.summary.sender ?? 'Unknown sender'}
        </p>
      )}
      {!sourceEligible && (
        <StatusMessage state="The source mailbox is no longer eligible for this client. Re-authenticate or correct the client association in Mailboxes before sending." />
      )}

      <form className="stack-form" onSubmit={submit}>
        <label htmlFor="client-mail-compose-from">From</label>
        <select
          id="client-mail-compose-from"
          value={mailboxBindingId}
          required
          disabled={isSourceOperation(operation) || fieldsDisabled}
          onChange={(event) => {
            setMailboxBindingId(event.currentTarget.value);
            invalidateAttempt();
          }}
        >
          <option value="">Select eligible mailbox</option>
          {mailboxes.map((mailbox) => (
            <option key={mailbox.bindingId} value={mailbox.bindingId}>
              {mailbox.bindingId} · {mailbox.provider}
            </option>
          ))}
        </select>

        {needsExplicitRecipients(operation) && (
          <>
            <label htmlFor="client-mail-compose-to">To</label>
            <input
              id="client-mail-compose-to"
              value={to}
              autoComplete="off"
              disabled={fieldsDisabled}
              onChange={(event) => {
                setTo(event.currentTarget.value);
                invalidateAttempt();
              }}
              placeholder="client@example.com"
            />
            <label htmlFor="client-mail-compose-cc">Cc</label>
            <input
              id="client-mail-compose-cc"
              value={cc}
              autoComplete="off"
              disabled={fieldsDisabled}
              onChange={(event) => {
                setCc(event.currentTarget.value);
                invalidateAttempt();
              }}
            />
            <label htmlFor="client-mail-compose-bcc">Bcc</label>
            <input
              id="client-mail-compose-bcc"
              value={bcc}
              autoComplete="off"
              disabled={fieldsDisabled}
              onChange={(event) => {
                setBcc(event.currentTarget.value);
                invalidateAttempt();
              }}
            />
            <p className="muted">
              Separate addresses with commas or semicolons. Client-side validation is convenience only; the backend independently validates recipients and mailbox ownership.
            </p>
          </>
        )}

        {operation !== 'REPLY' && operation !== 'REPLY_ALL' && (
          <>
            <label htmlFor="client-mail-compose-subject">Subject</label>
            <input
              id="client-mail-compose-subject"
              value={subject}
              maxLength={998}
              disabled={fieldsDisabled}
              onChange={(event) => {
                setSubject(event.currentTarget.value);
                invalidateAttempt();
              }}
            />
          </>
        )}

        <label htmlFor="client-mail-compose-body">Message</label>
        <textarea
          id="client-mail-compose-body"
          value={textBody}
          maxLength={1_048_576}
          required
          disabled={fieldsDisabled}
          onChange={(event) => {
            setTextBody(event.currentTarget.value);
            invalidateAttempt();
          }}
          rows={10}
        />

        <button
          type="submit"
          disabled={
            send.isPending ||
            lockedAttempt ||
            rejectedNeedsEdit ||
            mailboxes.length === 0 ||
            !sourceEligible
          }
        >
          {send.isPending ? 'Sending protected attempt…' : 'Send'}
        </button>
      </form>

      <StatusMessage state={validation ?? send.error ?? receiptMessage(receipt)} />

      {(canRetryReceipt || canRetryTransport) && attempt && (
        <button
          type="button"
          disabled={send.isPending}
          onClick={() => send.mutate(attempt)}
        >
          Retry same protected attempt
        </button>
      )}

      {receipt?.state === 'AMBIGUOUS' && (
        <p className="muted">
          Retry is intentionally unavailable for an ambiguous outcome because an uncontrolled resend could duplicate delivery.
        </p>
      )}
    </section>
  );
}
