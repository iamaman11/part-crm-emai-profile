import { useMutation } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { setClientGrant } from './api';
import type { ClientProjection } from './api';
import { ConfirmAction } from '../../shared/ui/ConfirmAction';
import { StatusMessage } from '../../shared/ui/StatusMessage';

type ClientGrantInput = {
  actorId: string;
  role: 'CLIENT_VIEWER' | 'CLIENT_EDITOR';
  reason: string;
  expectedClientVersion: number;
};

export function ClientGrantPanel({
  tenantId,
  client,
  onMutated,
}: {
  tenantId: string;
  client: ClientProjection;
  onMutated: () => Promise<void>;
}) {
  const [input, setInput] = useState<ClientGrantInput>({
    actorId: '',
    role: 'CLIENT_VIEWER',
    reason: '',
    expectedClientVersion: client.version,
  });
  useEffect(() => {
    setInput((current) => ({ ...current, expectedClientVersion: client.version }));
  }, [client.version]);

  const apply = useMutation({
    mutationFn: (value: ClientGrantInput) => setClientGrant(
      tenantId,
      client.clientId,
      value.actorId,
      {
        role: value.role,
        reason: value.reason,
        expectedClientVersion: value.expectedClientVersion,
      },
      false,
    ),
    onSuccess: onMutated,
  });
  const revoke = useMutation({
    mutationFn: (value: ClientGrantInput) => setClientGrant(
      tenantId,
      client.clientId,
      value.actorId,
      {
        role: value.role,
        reason: value.reason,
        expectedClientVersion: value.expectedClientVersion,
      },
      true,
    ),
    onSuccess: onMutated,
  });

  const disabled = client.status === 'MERGED';
  return (
    <section className="panel full-span">
      <span className="eyebrow">Explicit access; assignment never grants access</span>
      <h2>Client grant</h2>
      <div className="action-grid">
        <label>
          Actor ID
          <input
            value={input.actorId}
            onChange={(event) => setInput({ ...input, actorId: event.currentTarget.value })}
            disabled={disabled}
          />
        </label>
        <label>
          Role
          <select
            value={input.role}
            onChange={(event) => setInput({
              ...input,
              role: event.currentTarget.value as ClientGrantInput['role'],
            })}
            disabled={disabled}
          >
            <option value="CLIENT_VIEWER">Viewer</option>
            <option value="CLIENT_EDITOR">Editor</option>
          </select>
        </label>
        <label>
          Expected client version
          <input
            type="number"
            min="1"
            value={input.expectedClientVersion}
            onChange={(event) => setInput({
              ...input,
              expectedClientVersion: Number(event.currentTarget.value),
            })}
            disabled={disabled}
          />
        </label>
        <label className="wide">
          Reason
          <input
            value={input.reason}
            onChange={(event) => setInput({ ...input, reason: event.currentTarget.value })}
            disabled={disabled}
          />
        </label>
        <button
          type="button"
          disabled={disabled || apply.isPending || revoke.isPending || !input.actorId.trim() || !input.reason.trim()}
          onClick={() => apply.mutate(input)}
        >
          Apply grant
        </button>
        <ConfirmAction
          label="revoke client grant"
          consequence="This removes the explicit client grant. It does not change profile assignment or profile grants."
          disabled={disabled || apply.isPending || revoke.isPending || !input.actorId.trim() || !input.reason.trim()}
          onConfirm={() => revoke.mutateAsync(input).then(() => undefined)}
        />
      </div>
      <StatusMessage state={apply.error ?? revoke.error ?? apply.data?.resultCode ?? revoke.data?.resultCode ?? null} />
    </section>
  );
}
