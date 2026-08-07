import { useMutation } from '@tanstack/react-query';
import { type FormEvent } from 'react';
import { useTenant } from '../../app/TenantContext';
import { acceptInvitation, createInvitation, transferOwner, updateMembershipStatus } from '../../shared/api/endpoints';
import { ConfirmAction } from '../../shared/ui/ConfirmAction';
import { StatusMessage } from '../../shared/ui/StatusMessage';

function field(data: FormData, name: string): string {
  return String(data.get(name) ?? '').trim();
}

export function AccessWorkspace() {
  const { tenantId } = useTenant();
  const invitation = useMutation({ mutationFn: (input: { invitationId: string; invitedContactHmac: string; expiresAtMs: number; expectedTenantVersion: number }) => createInvitation(tenantId, input) });
  const accept = useMutation({ mutationFn: (input: { invitationId: string; identityId: string; actorId: string }) => acceptInvitation(tenantId, input.invitationId, { identityId: input.identityId, actorId: input.actorId }) });
  const membership = useMutation({ mutationFn: (input: { actorId: string; status: 'ACTIVE' | 'SUSPENDED' | 'REVOKED'; expectedVersion: number }) => updateMembershipStatus(tenantId, input.actorId, { status: input.status, expectedVersion: input.expectedVersion }) });
  const owner = useMutation({ mutationFn: (input: { nextOwnerActorId: string; currentOwnerVersion: number; nextOwnerVersion: number }) => transferOwner(tenantId, input) });
  const enabled = tenantId.length > 0;

  return (
    <div className="workspace-grid">
      <section className="panel">
        <span className="eyebrow">Owner-governed command</span><h2>Create invitation</h2>
        <form className="stack-form" onSubmit={(event) => {
          event.preventDefault();
          const data = new FormData(event.currentTarget);
          invitation.mutate({ invitationId: field(data, 'invitationId'), invitedContactHmac: field(data, 'contactHmac'), expiresAtMs: Number(field(data, 'expiresAtMs')), expectedTenantVersion: Number(field(data, 'version')) });
        }}>
          <label>Invitation ID<input name="invitationId" required disabled={!enabled} /></label>
          <label>Invited contact HMAC<input name="contactHmac" required disabled={!enabled} /></label>
          <label>Expires at (Unix ms)<input name="expiresAtMs" type="number" min="1" required disabled={!enabled} /></label>
          <label>Expected tenant version<input name="version" type="number" min="1" required defaultValue="1" disabled={!enabled} /></label>
          <button type="submit" disabled={!enabled || invitation.isPending}>Create invitation</button>
        </form>
        <StatusMessage state={invitation.error ?? (invitation.data ? `${invitation.data.resultCode}: ${invitation.data.resourceId}` : null)} />
      </section>

      <section className="panel">
        <span className="eyebrow">Verified identity acceptance</span><h2>Accept invitation</h2>
        <form className="stack-form" onSubmit={(event: FormEvent<HTMLFormElement>) => {
          event.preventDefault();
          const data = new FormData(event.currentTarget);
          accept.mutate({ invitationId: field(data, 'invitationId'), identityId: field(data, 'identityId'), actorId: field(data, 'actorId') });
        }}>
          <label>Invitation ID<input name="invitationId" required disabled={!enabled} /></label>
          <label>Identity ID<input name="identityId" required disabled={!enabled} /></label>
          <label>Actor ID<input name="actorId" required disabled={!enabled} /></label>
          <button type="submit" disabled={!enabled || accept.isPending}>Accept invitation</button>
        </form>
        <StatusMessage state={accept.error ?? (accept.data ? `${accept.data.resultCode}: ${accept.data.resourceId}` : null)} />
      </section>

      <section className="panel">
        <span className="eyebrow">Membership lifecycle</span><h2>Update member</h2>
        <form className="stack-form" onSubmit={(event) => {
          event.preventDefault();
          const data = new FormData(event.currentTarget);
          membership.mutate({ actorId: field(data, 'actorId'), status: field(data, 'status') as 'ACTIVE' | 'SUSPENDED' | 'REVOKED', expectedVersion: Number(field(data, 'version')) });
        }}>
          <label>Actor ID<input name="actorId" required disabled={!enabled} /></label>
          <label>Status<select name="status" defaultValue="SUSPENDED" disabled={!enabled}><option value="ACTIVE">Active</option><option value="SUSPENDED">Suspended</option><option value="REVOKED">Revoked</option></select></label>
          <label>Expected version<input name="version" type="number" min="1" defaultValue="1" required disabled={!enabled} /></label>
          <button type="submit" disabled={!enabled || membership.isPending}>Apply membership status</button>
        </form>
        <StatusMessage state={membership.error ?? (membership.data ? `${membership.data.resultCode}: ${membership.data.resourceId}` : null)} />
      </section>

      <OwnerTransferPanel disabled={!enabled || owner.isPending} onTransfer={(input) => owner.mutateAsync(input).then(() => undefined)} />
      <StatusMessage state={owner.error ?? (owner.data ? `${owner.data.resultCode}: ${owner.data.resourceId}` : null)} />
    </div>
  );
}

function OwnerTransferPanel({ disabled, onTransfer }: { disabled: boolean; onTransfer: (input: { nextOwnerActorId: string; currentOwnerVersion: number; nextOwnerVersion: number }) => Promise<void> }) {
  let pending: { nextOwnerActorId: string; currentOwnerVersion: number; nextOwnerVersion: number } | null = null;
  return (
    <section className="panel">
      <span className="eyebrow">High-impact ceremony</span><h2>Transfer owner</h2>
      <form className="stack-form" onSubmit={(event) => event.preventDefault()}>
        <label>Next owner actor ID<input name="nextOwnerActorId" required disabled={disabled} /></label>
        <label>Current owner version<input name="currentOwnerVersion" type="number" min="1" defaultValue="1" required disabled={disabled} /></label>
        <label>Next owner version<input name="nextOwnerVersion" type="number" min="1" defaultValue="1" required disabled={disabled} /></label>
        <ConfirmAction
          label="transfer tenant ownership"
          consequence="Ownership is singular. This command changes the tenant owner and must only be confirmed with the expected versions shown by trusted operational evidence."
          disabled={disabled}
          onConfirm={async () => {
            const form = document.activeElement?.closest('form');
            if (!(form instanceof HTMLFormElement)) throw new Error('Owner transfer form is unavailable');
            const data = new FormData(form);
            pending = { nextOwnerActorId: field(data, 'nextOwnerActorId'), currentOwnerVersion: Number(field(data, 'currentOwnerVersion')), nextOwnerVersion: Number(field(data, 'nextOwnerVersion')) };
            await onTransfer(pending);
          }}
        />
      </form>
    </section>
  );
}
