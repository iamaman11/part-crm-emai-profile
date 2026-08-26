import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { useLogicalCommandMutation } from '../shared/ui/useLogicalCommandMutation';

function Harness({ execute }: { execute: (input: string, key: string) => Promise<void> }) {
  const mutation = useLogicalCommandMutation(execute);
  return (
    <>
      <button type="button" onClick={() => mutation.mutate('command')}>start</button>
      <button type="button" onClick={() => void mutation.retryLatestCommand()}>retry</button>
    </>
  );
}

describe('useLogicalCommandMutation', () => {
  it('retries the same logical command with its original idempotency key', async () => {
    const execute = vi.fn<(_: string, __: string) => Promise<void>>()
      .mockRejectedValueOnce(new Error('network interruption'))
      .mockResolvedValueOnce();
    const client = new QueryClient();
    render(
      <QueryClientProvider client={client}>
        <Harness execute={execute} />
      </QueryClientProvider>,
    );

    await act(async () => screen.getByRole('button', { name: 'start' }).click());
    await waitFor(() => expect(execute).toHaveBeenCalledTimes(1));
    await act(async () => screen.getByRole('button', { name: 'retry' }).click());
    await waitFor(() => expect(execute).toHaveBeenCalledTimes(2));

    expect(execute.mock.calls[0]?.[0]).toBe('command');
    expect(execute.mock.calls[1]?.[0]).toBe('command');
    expect(execute.mock.calls[1]?.[1]).toBe(execute.mock.calls[0]?.[1]);
  });
});
