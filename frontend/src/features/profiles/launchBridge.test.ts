import { describe, expect, it } from 'vitest';
import { localProfileDeleteUri, localProfileOpenUri } from './launchBridge';

describe('local native ProfileBridge Open', () => {
  it('uses exactly the installed v2 URI and needs no network prefetch', () => {
    expect(localProfileOpenUri(
      'tenant_test', 'actor_test', 'profile_test', 7, 'READY',
    )).toBe('profilebridge://open/v2/tenant_test/actor_test/profile_test/7/READY');
    expect(localProfileOpenUri(
      'tenant_test', 'actor_test', 'profile_draft', 1, 'DRAFT',
    )).toBe('profilebridge://open/v2/tenant_test/actor_test/profile_draft/1/DRAFT');
  });

  it('does not pass malformed context, URI injections, or invalid versions to the OS', () => {
    const ready = (tenant: string, actor: string, profile: string, version: number) =>
      localProfileOpenUri(tenant, actor, profile, version, 'READY');

    for (const bad of [0, -1, 1.5, Number.NaN, Infinity, Number.MAX_SAFE_INTEGER + 1]) {
      expect(() => ready('tenant_test', 'actor_test', 'profile_test', bad)).toThrow();
    }
    for (const bad of ['other/segment', '', 'a', 'tenant?x=1', 'x'.repeat(100)]) {
      expect(() => ready(bad, 'actor_test', 'profile_test', 7)).toThrow();
      expect(() => ready('tenant_test', bad, 'profile_test', 7)).toThrow();
      expect(() => ready('tenant_test', 'actor_test', bad, 7)).toThrow();
    }
  });
});

describe('native terminal ProfileBridge Delete', () => {
  it('constrains terminal deletion to exact tenant/actor/profile and positive version', () => {
    expect(localProfileDeleteUri('tenant_test', 'actor_test', 'profile_test', 4))
      .toBe('profilebridge://delete/v1/tenant_test/actor_test/profile_test/4');
    for (const version of [0, -1, 1.5, Number.NaN, Infinity, Number.MAX_SAFE_INTEGER + 1]) {
      expect(() => localProfileDeleteUri('tenant_test', 'actor_test', 'profile_test', version))
        .toThrow();
    }
    for (const invalid of ['', 'x', 'other/segment', 'tenant?x=1', 'x'.repeat(100)]) {
      expect(() => localProfileDeleteUri(invalid, 'actor_test', 'profile_test', 4)).toThrow();
      expect(() => localProfileDeleteUri('tenant_test', invalid, 'profile_test', 4)).toThrow();
      expect(() => localProfileDeleteUri('tenant_test', 'actor_test', invalid, 4)).toThrow();
    }
  });
});
