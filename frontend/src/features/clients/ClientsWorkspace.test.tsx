import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createClient, listClients } from './api';
import { ClientsWorkspace } from './ClientsWorkspace';

vi.mock('../../app/TenantContext', () => ({ useTenant: () => ({ tenantId: 'tenant_ui', contexts: [{ tenantId: 'tenant_ui', actorId: 'actor_ui' }] }) }));
vi.mock('../../app/CapabilityContext', () => ({ useCapabilities: () => ({ enabled: () => false }) }));
vi.mock('./api', () => ({ createClient: vi.fn(), listClients: vi.fn(), getClient: vi.fn(), getClientHistory: vi.fn() }));
vi.mock('./ClientProfilesPanel', () => ({ ClientProfilesPanel: () => null }));
vi.mock('./ClientGrantPanel', () => ({ ClientGrantPanel: () => null }));
vi.mock('./ClientHistoryPanel', () => ({ ClientHistoryPanel: () => null }));
vi.mock('./ClientMutationPanels', () => ({ ClientMutationPanels: () => null }));
vi.mock('./ClientMailPanel', () => ({ ClientMailPanel: () => null }));

const mockedCreate = vi.mocked(createClient);
const mockedList = vi.mocked(listClients);

function renderWorkspace(onClientSelected = vi.fn()) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(<QueryClientProvider client={client}><ClientsWorkspace onClientSelected={onClientSelected} /></QueryClientProvider>);
  return onClientSelected;
}

describe('ClientsWorkspace new client flow', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mockedList.mockResolvedValue({ clients: [] });
  });

  it('creates from a name with a generated immutable ID', async () => {
    mockedCreate.mockResolvedValue({ resultCode: 'created', resourceId: 'client_created', aggregateVersion: 1 });
    const selected = renderWorkspace();
    fireEvent.click(screen.getByText('+ Create a client'));
    fireEvent.change(screen.getByRole('textbox', { name: 'Client name' }), { target: { value: 'Northern Sales' } });
    fireEvent.change(screen.getByRole('combobox', { name: 'Type' }), { target: { value: 'ORGANIZATION' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create client' }));

    await waitFor(() => expect(mockedCreate).toHaveBeenCalledTimes(1));
    expect(mockedCreate.mock.calls[0]![0]).toBe('tenant_ui');
    expect(mockedCreate.mock.calls[0]![1]).toEqual({ clientId: expect.stringMatching(/^client_[0-9a-f]{32}$/), displayName: 'Northern Sales', kind: 'ORGANIZATION' });
    expect(mockedCreate.mock.calls[0]![2]).toBeTruthy();
    await waitFor(() => expect(selected).toHaveBeenCalledWith('client_created'));
  });

  it('does not generate a duplicate ID or idempotency key after an ambiguous creation error', async () => {
    mockedCreate.mockRejectedValueOnce(new Error('unconfirmed network response'));
    mockedCreate.mockResolvedValue({ resultCode: 'created', resourceId: 'client_retried', aggregateVersion: 1 });
    const selected = renderWorkspace();
    fireEvent.click(screen.getByText('+ Create a client'));
    fireEvent.change(screen.getByRole('textbox', { name: 'Client name' }), { target: { value: 'Retry contact' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create client' }));

    expect((await screen.findByRole('alert')).textContent).toContain('retry the original request');
    expect((screen.getByRole('button', { name: 'Create client' }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: 'Retry client creation' }));

    await waitFor(() => expect(mockedCreate).toHaveBeenCalledTimes(2));
    expect(mockedCreate.mock.calls[1]![1]).toEqual(mockedCreate.mock.calls[0]![1]);
    expect(mockedCreate.mock.calls[1]![2]).toBe(mockedCreate.mock.calls[0]![2]);
    await waitFor(() => expect(selected).toHaveBeenCalledWith('client_retried'));
  });
});
