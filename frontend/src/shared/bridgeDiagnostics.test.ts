import { describe, expect, it } from 'vitest';
import { COPY_LOCAL_DIAGNOSTICS_URI } from './bridgeDiagnostics';

describe('local Bridge diagnostics scheme', () => {
  it('has one immutable local command with no web origin, actor, profile or token', () => {
    expect(COPY_LOCAL_DIAGNOSTICS_URI).toBe('profilebridge://diagnostics/copy');
    expect(COPY_LOCAL_DIAGNOSTICS_URI).not.toMatch(/[?#]/);
    expect(COPY_LOCAL_DIAGNOSTICS_URI).not.toContain('https://');
    expect(COPY_LOCAL_DIAGNOSTICS_URI).not.toContain('tenant_');
    expect(COPY_LOCAL_DIAGNOSTICS_URI).not.toContain('claim_');
  });
});
