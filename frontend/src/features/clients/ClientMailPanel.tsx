import { useState, type FormEvent } from 'react';
import type { ClientMailSearchInput } from '../../shared/api/generated/query-mail';
import { StatusMessage } from '../../shared/ui/StatusMessage';

type Props = {
  clientId: string;
};

function field(form: FormData, name: string): string {
  return String(form.get(name) ?? '').trim();
}

export function ClientMailPanel({ clientId }: Props) {
  const [prepared, setPrepared] = useState<ClientMailSearchInput | null>(null);

  return (
    <section className="panel">
      <span className="eyebrow">Client → Mail</span>
      <h2>Mailbox query</h2>
      <p>
        Prepare the bounded provider-neutral query for this client. Execution is enabled only when
        an authorized eligible provider lane is composed; this screen does not persist search terms
        or message content in Web Storage.
      </p>
      <form
        className="stack-form"
        onSubmit={(event: FormEvent<HTMLFormElement>) => {
          event.preventDefault();
          const data = new FormData(event.currentTarget);
          setPrepared({
            mailboxBindingId: field(data, 'mailboxBindingId'),
            term: field(data, 'term') || null,
            cursor: null,
            limit: 25,
          });
        }}
      >
        <label htmlFor="client-mail-binding">Mailbox binding ID</label>
        <input
          id="client-mail-binding"
          name="mailboxBindingId"
          minLength={8}
          maxLength={96}
          placeholder="binding_..."
          required
        />
        <label htmlFor="client-mail-term">Search term</label>
        <input id="client-mail-term" name="term" maxLength={200} autoComplete="off" />
        <button type="submit">Prepare bounded query</button>
      </form>
      <StatusMessage
        state={
          prepared
            ? `Query prepared for ${clientId} and ${prepared.mailboxBindingId}; provider execution awaits an eligible composed lane.`
            : null
        }
      />
    </section>
  );
}
