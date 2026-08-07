import { ApiProblem } from '../api/client';

export function StatusMessage({ state }: { state: unknown }) {
  if (state === null || state === undefined || state === '') return null;
  if (state instanceof ApiProblem) {
    const neutral = state.code === 'not_found' || state.code === 'forbidden';
    return (
      <div className="status-message" role="status" aria-live="polite">
        <strong>{neutral ? 'Resource unavailable' : state.message}</strong>
        <span>Code: {state.code}</span>
        <span>Correlation: {state.correlationId}</span>
      </div>
    );
  }
  if (state instanceof Error) {
    return <div className="status-message" role="alert">{state.message}</div>;
  }
  return <div className="status-message" role="status" aria-live="polite">{String(state)}</div>;
}
