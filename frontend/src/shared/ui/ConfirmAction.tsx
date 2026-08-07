import { useState, type ReactNode } from 'react';

interface ConfirmActionProps {
  label: string;
  consequence: ReactNode;
  onConfirm: () => Promise<void> | void;
  disabled?: boolean;
}

export function ConfirmAction({ label, consequence, onConfirm, disabled = false }: ConfirmActionProps) {
  const [armed, setArmed] = useState(false);
  const [busy, setBusy] = useState(false);

  if (!armed) {
    return <button type="button" className="danger" disabled={disabled} onClick={() => setArmed(true)}>{label}</button>;
  }

  return (
    <div className="confirm-action" role="group" aria-label={`${label} confirmation`}>
      <div className="consequence">{consequence}</div>
      <button
        type="button"
        className="danger"
        disabled={busy}
        onClick={async () => {
          setBusy(true);
          try {
            await onConfirm();
            setArmed(false);
          } finally {
            setBusy(false);
          }
        }}
      >
        {busy ? 'Applying…' : `Confirm ${label}`}
      </button>
      <button type="button" disabled={busy} onClick={() => setArmed(false)}>Cancel</button>
    </div>
  );
}
