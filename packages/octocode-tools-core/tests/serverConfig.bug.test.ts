/**
 * TDD proof for Bug #4: `getTokenSource()` re-resolves the GitHub token on
 * every call even after `initialize()` has already cached it in `config`.
 * This file should be RED before the fix, GREEN after.
 */
import { afterEach, describe, it, expect, vi } from 'vitest';

describe('getTokenSource - Bug #4: re-resolves token after initialize()', () => {
  afterEach(() => {
    vi.resetModules();
  });

  it('reads tokenSource from the cached config without invoking the resolver again', async () => {
    const sc = await import('../src/serverConfig.js');

    let resolveCallCount = 0;
    sc._setTokenResolvers({
      resolveTokenFull: async () => {
        resolveCallCount++;
        return { token: 'mock-token', source: 'env:GH_TOKEN' };
      },
    });

    await sc.initialize();
    // reset counter; initialize() is allowed one resolve call
    resolveCallCount = 0;

    // BUG: getTokenSource() calls resolveGitHubToken() unconditionally, so
    // each call below increments resolveCallCount.  After the fix it must be 0.
    await sc.getTokenSource();
    await sc.getTokenSource();

    expect(resolveCallCount).toBe(0);
  });

  it('returns the same source value that initialize() cached', async () => {
    const sc = await import('../src/serverConfig.js');

    sc._setTokenResolvers({
      resolveTokenFull: async () => ({
        token: 'tok',
        source: 'env:OCTOCODE_TOKEN',
      }),
    });

    await sc.initialize();

    const source = await sc.getTokenSource();
    expect(source).toBe('env:OCTOCODE_TOKEN');
  });

  it('still resolves live when called before initialize()', async () => {
    // Before initialize() there is no cached config; the live resolver must run.
    const sc = await import('../src/serverConfig.js');

    let called = false;
    sc._setTokenResolvers({
      resolveTokenFull: async () => {
        called = true;
        return { token: 'tok', source: 'gh-cli' };
      },
    });

    const source = await sc.getTokenSource();
    expect(called).toBe(true);
    expect(source).toBe('gh-cli');
  });
});
