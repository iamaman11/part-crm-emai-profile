import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { getBindingProbe, getHealth } from './api';
import { SettingsWorkspace } from './SettingsWorkspace';

vi.mock('./api', () => ({
  getHealth: vi.fn(),
  getBindingProbe: vi.fn(),
}));

describe('Settings native diagnostics', () => {
  it('uses the existing Windows URI handler and never requests profile data', () => {
    vi.mocked(getHealth).mockImplementation(() => new Promise(() => undefined));
    vi.mocked(getBindingProbe).mockImplementation(() => new Promise(() => undefined));
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <SettingsWorkspace />
      </QueryClientProvider>,
    );
    const link = screen.getByRole('link', { name: 'Скопировать диагностику' });
    expect(link.getAttribute('href')).toBe('profilebridge://diagnostics/copy');
    expect(screen.getByText(/Nothing is uploaded/)).toBeTruthy();
  });
});
