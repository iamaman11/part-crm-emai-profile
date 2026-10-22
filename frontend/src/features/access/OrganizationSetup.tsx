import { useState, type FormEvent } from 'react';
import { useTenant } from '../../app/TenantContext';
import { StatusMessage } from '../../shared/ui/StatusMessage';
import { useLogicalCommandMutation } from '../../shared/ui/useLogicalCommandMutation';
import { bootstrapOwner } from './api';

export function OrganizationSetup() {
  const { state, canBootstrapOwner, retry, refresh } = useTenant();
  const [name, setName] = useState('');
  const setup = useLogicalCommandMutation(bootstrapOwner, { onSuccess: refresh });

  if (state === 'loading') return <p role="status">Loading your organization access…</p>;
  if (state === 'error') return <section className="panel" role="alert"><h2>Organization access could not be loaded</h2><button onClick={retry}>Retry</button></section>;
  if (state === 'selecting') return <section className="panel"><h2>Choose your organization</h2><p>Select an organization above to continue.</p></section>;
  if (state === 'ready') return null;
  if (!canBootstrapOwner) {
    return (
      <section className="panel">
        <h2>Organization access required</h2>
        <p>Your sign-in succeeded. An organization owner must grant you access before you can continue.</p>
        <button type="button" onClick={retry}>Check access again</button>
        <p><a href="/cdn-cgi/access/logout">Sign out to use another account</a></p>
      </section>
    );
  }

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!name.trim() || setup.isPending) return;
    setup.mutate({ tenantDisplayName: name.trim() });
  }

  return (
    <section className="panel">
      <h2>Create your organization</h2>
      <p>Your account is authorized for initial setup. Choose an organization name to start using clients and browser profiles.</p>
      <form className="stack-form" onSubmit={submit}>
        <label htmlFor="organization-name">Organization name</label>
        <input id="organization-name" value={name} onChange={(event) => setName(event.currentTarget.value)} maxLength={200} required disabled={setup.isPending} autoComplete="organization" />
        <button type="submit" disabled={!name.trim() || setup.isPending}>{setup.isPending ? 'Creating organization…' : 'Create organization'}</button>
      </form>
      <StatusMessage state={setup.error} />
      {setup.isError && <button type="button" disabled={setup.isPending} onClick={() => { void setup.retryLatestCommand().catch(() => undefined); }}>Retry previous setup request</button>}
    </section>
  );
}
