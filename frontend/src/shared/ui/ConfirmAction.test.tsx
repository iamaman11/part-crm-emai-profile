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
});
