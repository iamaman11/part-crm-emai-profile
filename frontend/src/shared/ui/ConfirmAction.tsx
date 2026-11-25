import { useState, type ReactNode } from 'react';

interface ConfirmActionProps {
  label: string;
  consequence: ReactNode;
  onArm?: () => Promise<boolean> | boolean;
  onConfirm: () => Promise<void> | void;
  disabled?: boolean;
}

export function ConfirmAction({ label, consequence, onArm, onConfirm, disabled = false }: ConfirmActionProps) {
  const [armed, setArmed] = useState(false);
  const [arming, setArming] = useState(false);
  const [busy, setBusy] = useState(false);

  if (!armed) {
    return <button type="button" className="danger" disabled={disabled || arming} onClick={async () => {
      setArming(true);
      try {
        if (!onArm || await onArm()) setArmed(true);
      } finally {
        setArming(false);
      }
    }}>{arming ? 'Checking current status…' : label}</button>;
  }

  return (
    <div className="confirm-action" role="group" aria-label={`${label} confirmation`}>
      <div className="consequence">{consequence}</div>
      <button
        type="button"
        className="danger"
        disabled={busy || disabled}
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
