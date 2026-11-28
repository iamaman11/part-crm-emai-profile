import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { ClientProjection } from './api';
import { ClientRegistryList } from './ClientRegistryList';

const clients: ClientProjection[] = [
  { clientId: 'client_b', displayName: 'Beta Sales', kind: 'ORGANIZATION', status: 'ACTIVE', version: 1 },
  { clientId: 'client_a', displayName: 'Alpha Contact', kind: 'PERSON', status: 'ACTIVE', version: 2 },
  { clientId: 'client_old', displayName: 'Old Project', kind: 'ORGANIZATION', status: 'ARCHIVED', version: 3 },
];

describe('ClientRegistryList browsing controls', () => {
  it('sorts by name, filters locally, and leaves selection to the parent', () => {
    const onSelect = vi.fn();
    render(<ClientRegistryList clients={clients} selectedClientId="client_b" onSelect={onSelect} />);

    const rows = screen.getAllByRole('button');
    expect(rows.map((row) => row.textContent)).toEqual([
      expect.stringContaining('Alpha Contact'),
      expect.stringContaining('Beta Sales'),
    ]);
    expect(screen.queryByText('Old Project')).toBeNull();
    expect(screen.getByText('Beta Sales').closest('button')?.getAttribute('aria-pressed')).toBe('true');

    fireEvent.change(screen.getByRole('searchbox', { name: 'Search clients' }), { target: { value: 'alpha' } });
    expect(screen.getByText('Alpha Contact')).toBeTruthy();
    expect(screen.queryByText('Beta Sales')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: /Alpha Contact/ }));
    expect(onSelect).toHaveBeenCalledExactlyOnceWith('client_a');
  });

  it('keeps an already selected archived client visible and can show other archived clients', () => {
    render(<ClientRegistryList clients={clients} selectedClientId="client_old" onSelect={vi.fn()} />);

    expect(screen.getByText('Old Project')).toBeTruthy();
    fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'client_old' } });
    expect(screen.getByText('Old Project')).toBeTruthy();
    fireEvent.change(screen.getByRole('searchbox'), { target: { value: '' } });
    fireEvent.click(screen.getByRole('checkbox', { name: 'Show archived (1)' }));
    expect(screen.getAllByRole('button')).toHaveLength(3);
  });
});
