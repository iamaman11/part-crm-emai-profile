import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { ConfirmAction } from './ConfirmAction';

describe('ConfirmAction', () => {
  it('requires a separate consequence preview before applying a destructive action', async () => {
    const user = userEvent.setup();
    const onConfirm = vi.fn(async () => undefined);
    render(
      <ConfirmAction
        label="Revoke access"
        consequence="The target actor will lose explicit access."
        onConfirm={onConfirm}
      />,
    );

    await user.click(screen.getByRole('button', { name: 'Revoke access' }));
    expect(onConfirm).not.toHaveBeenCalled();
    expect(screen.getByText('The target actor will lose explicit access.')).toBeTruthy();

    await user.click(screen.getByRole('button', { name: 'Confirm Revoke access' }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('button', { name: 'Revoke access' })).toBeTruthy();
  });

  it('keeps consent closed when the optional pre-confirmation read rejects the action', async () => {
    const user = userEvent.setup();
    const onArm = vi.fn(async () => false);
    const onConfirm = vi.fn();
    render(<ConfirmAction label="Remove local copy" consequence="Permanent local cleanup." onArm={onArm} onConfirm={onConfirm} />);
    await user.click(screen.getByRole('button', { name: 'Remove local copy' }));
    expect(onArm).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('button', { name: 'Confirm Remove local copy' })).toBeNull();
    expect(onConfirm).not.toHaveBeenCalled();
  });
});
