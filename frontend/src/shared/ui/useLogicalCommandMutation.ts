import { useCallback, useRef } from 'react';
import { useMutation } from '@tanstack/react-query';
import { newIdempotencyKey } from '../api/idempotency';

type LogicalCommand<TInput> = {
  readonly input: TInput;
  readonly idempotencyKey: string;
};

type Options<TData, TInput> = {
  readonly onSuccess?: (data: TData, input: TInput) => void | Promise<void>;
  readonly onError?: (error: Error, input: TInput) => void | Promise<void>;
};

export function useLogicalCommandMutation<TInput, TData>(
  execute: (input: TInput, idempotencyKey: string) => Promise<TData>,
  options: Options<TData, TInput> = {},
) {
  const latest = useRef<LogicalCommand<TInput> | null>(null);
  const mutation = useMutation({
    mutationFn: (command: LogicalCommand<TInput>) => execute(command.input, command.idempotencyKey),
    retry: false,
    onSuccess: (data, command) => options.onSuccess?.(data, command.input),
    onError: (error, command) => options.onError?.(error, command.input),
  });

  const command = useCallback((input: TInput): LogicalCommand<TInput> => {
    const next = { input, idempotencyKey: newIdempotencyKey() };
    latest.current = next;
    return next;
  }, []);

  return {
    ...mutation,
    mutate: (input: TInput) => mutation.mutate(command(input)),
    mutateAsync: (input: TInput) => mutation.mutateAsync(command(input)),
    retryLatestCommand: () => {
      if (latest.current === null) return Promise.reject(new Error('No logical command is available to retry.'));
      return mutation.mutateAsync(latest.current);
    },
  };
}
