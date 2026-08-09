import { useMutation } from '@tanstack/react-query';
import { useState, type FormEvent } from 'react';
import {
  archiveClient,
  archiveClientContact,
  mergeClient,
  updateClient,
  upsertClientContact,
} from '../../shared/api/endpoints';
import type { ClientProjection } from '../../shared/api/types';
import { ConfirmAction } from '../../shared/ui/ConfirmAction';
import { StatusMessage } from '../../shared/ui/StatusMessage';

function field(form: FormData, name: string): string {
  return String(form.get(name) ?? '').trim();
}

export function ClientMutationPanels({
  tenantId,
  client,
  onMutated,
}: {
  tenantId: string;
  client: ClientProjection;
  onMutated: () => Promise<void>;
}) {
  const [contactKind, setContactKind] = useState<'EMAIL' | 'PHONE' | 'URL'>('EMAIL');
  const update = useMutation({
    mutationFn: (displayName: string) => updateClient(tenantId, client.clientId, {
      displayName,
      expectedClientVersion: client.version,
    }),
    onSuccess: onMutated,
  });
  const archive = useMutation({
    mutationFn: () => archiveClient(tenantId, client.clientId, {
      expectedClientVersion: client.version,
    }),
    onSuccess: onMutated,
  });
  const contact = useMutation({
    mutationFn: (input: { contactPointId: string; kind: 'EMAIL' | 'PHONE' | 'URL'; value: string }) =>
      upsertClientContact(tenantId, client.clientId, input.contactPointId, {
        kind: input.kind,
        value: input.value,
        expectedClientVersion: client.version,
      }),
    onSuccess: onMutated,
  });
  const contactArchive = useMutation({
    mutationFn: (input: { contactPointId: string; kind: 'EMAIL' | 'PHONE' | 'URL' }) =>
      archiveClientContact(tenantId, client.clientId, input.contactPointId, {
        kind: input.kind,
        expectedClientVersion: client.version,
      }),
    onSuccess: onMutated,
  });
  const merge = useMutation({
    mutationFn: (input: { targetClientId: string; targetVersion: number; reason: string }) =>
      mergeClient(tenantId, client.clientId, {
        targetClientId: input.targetClientId,
        expectedSourceVersion: client.version,
        expectedTargetVersion: input.targetVersion,
        reason: input.reason,
      }),
    onSuccess: onMutated,
  });

  const ownerMutationDisabled = client.status !== 'ACTIVE';
  return (
    <>
      <section className="panel">
        <span className="eyebrow">Checked lifecycle mutation</span>
        <h2>Update or archive</h2>
        <form
          className="stack-form"
          onSubmit={(event) => {
            event.preventDefault();
            update.mutate(field(new FormData(event.currentTarget), 'displayName'));
          }}
        >
          <label htmlFor="client-update-name">Display name</label>
          <input
            id="client-update-name"
            name="displayName"
            defaultValue={client.displayName}
            maxLength={200}
            required
            disabled={ownerMutationDisabled || update.isPending}
          />
          <button type="submit" disabled={ownerMutationDisabled || update.isPending}>Update client</button>
        </form>
        <ConfirmAction
          label="archive client"
          consequence="The client becomes inactive. This does not merge the client and does not grant access elsewhere."
          disabled={ownerMutationDisabled || archive.isPending}
          onConfirm={async () => { await archive.mutateAsync(); }}
        />
        <StatusMessage state={update.error ?? archive.error ?? update.data?.resultCode ?? archive.data?.resultCode ?? null} />
      </section>

      <section className="panel">
        <span className="eyebrow">Protected contact mutation</span>
        <h2>Contact point</h2>
        <form
          className="stack-form"
          onSubmit={(event: FormEvent<HTMLFormElement>) => {
            event.preventDefault();
            const data = new FormData(event.currentTarget);
            contact.mutate({
              contactPointId: field(data, 'contactPointId'),
              kind: field(data, 'kind') as 'EMAIL' | 'PHONE' | 'URL',
              value: field(data, 'value'),
            });
          }}
        >
          <label htmlFor="client-contact-id">Contact point ID</label>
          <input id="client-contact-id" name="contactPointId" placeholder="contact_..." required disabled={ownerMutationDisabled} />
          <label htmlFor="client-contact-kind">Kind</label>
          <select
            id="client-contact-kind"
            name="kind"
            value={contactKind}
            onChange={(event) => setContactKind(event.currentTarget.value as 'EMAIL' | 'PHONE' | 'URL')}
            disabled={ownerMutationDisabled}
          >
            <option value="EMAIL">Email</option>
            <option value="PHONE">Phone</option>
            <option value="URL">URL</option>
          </select>
          <label htmlFor="client-contact-value">Value</label>
          <input id="client-contact-value" name="value" required disabled={ownerMutationDisabled || contact.isPending} />
          <button type="submit" disabled={ownerMutationDisabled || contact.isPending}>Protect and save contact</button>
        </form>
        <ContactArchiveAction
          disabled={ownerMutationDisabled || contactArchive.isPending}
          defaultKind={contactKind}
          onArchive={(contactPointId, kind) => contactArchive.mutateAsync({ contactPointId, kind }).then(() => undefined)}
        />
        <StatusMessage state={contact.error ?? contactArchive.error ?? contact.data?.resultCode ?? contactArchive.data?.resultCode ?? null} />
      </section>

      <section className="panel">
        <span className="eyebrow">One-way merge; no grant transfer</span>
        <h2>Merge client</h2>
        <form
          className="stack-form"
          onSubmit={(event) => {
            event.preventDefault();
            const data = new FormData(event.currentTarget);
            merge.mutate({
              targetClientId: field(data, 'targetClientId'),
              targetVersion: Number(field(data, 'targetVersion')),
              reason: field(data, 'reason'),
            });
          }}
        >
          <label htmlFor="client-merge-target">Target client ID</label>
          <input id="client-merge-target" name="targetClientId" placeholder="client_..." required disabled={ownerMutationDisabled} />
          <label htmlFor="client-merge-version">Target expected version</label>
          <input id="client-merge-version" name="targetVersion" type="number" min="1" defaultValue="1" required disabled={ownerMutationDisabled} />
          <label htmlFor="client-merge-reason">Reason</label>
          <input id="client-merge-reason" name="reason" maxLength={500} required disabled={ownerMutationDisabled} />
          <ConfirmAction
            label="merge source client"
            consequence="The source becomes permanently MERGED. Active profile assignments must be reassigned first. Source grants are removed and never transferred to the target."
            disabled={ownerMutationDisabled || merge.isPending}
            onConfirm={() => {
              const form = document.getElementById('client-merge-target')?.closest('form');
              form?.requestSubmit();
              return Promise.resolve();
            }}
          />
        </form>
        <StatusMessage state={merge.error ?? merge.data?.resultCode ?? null} />
      </section>
    </>
  );
}

function ContactArchiveAction({
  disabled,
  defaultKind,
  onArchive,
}: {
  disabled: boolean;
  defaultKind: 'EMAIL' | 'PHONE' | 'URL';
  onArchive: (contactPointId: string, kind: 'EMAIL' | 'PHONE' | 'URL') => Promise<void>;
}) {
  const [contactPointId, setContactPointId] = useState('');
  const [kind, setKind] = useState(defaultKind);
  return (
    <div className="action-grid">
      <label>
        Contact ID to archive
        <input value={contactPointId} onChange={(event) => setContactPointId(event.currentTarget.value)} disabled={disabled} />
      </label>
      <label>
        Kind
        <select value={kind} onChange={(event) => setKind(event.currentTarget.value as typeof kind)} disabled={disabled}>
          <option value="EMAIL">Email</option>
          <option value="PHONE">Phone</option>
          <option value="URL">URL</option>
        </select>
      </label>
      <ConfirmAction
        label="archive contact point"
        consequence="The protected contact remains in history but is no longer active."
        disabled={disabled || !contactPointId.trim()}
        onConfirm={() => onArchive(contactPointId.trim(), kind)}
      />
    </div>
  );
}
