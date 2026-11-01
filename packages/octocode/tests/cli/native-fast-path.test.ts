import { beforeEach, describe, expect, it, vi } from 'vitest';

// Every CLI call pays the launcher's startup, so a resolvable platform package
// must not evaluate the registry fallback (crypto, zlib, tar) or config's
// settings table. vi.mock factories run when a module is first loaded, so they
// record exactly which modules the launch evaluated.
const loaded = vi.hoisted(() => ({ fetcher: 0, config: 0 }));
const mocks = vi.hoisted(() => ({
  resolve: vi.fn<() => string>(),
  fetch: vi.fn<(options: Record<string, unknown>) => Promise<string>>(),
  allowed: vi.fn<() => boolean>(),
}));

vi.mock('../../../octocode-native/bin/resolve-binary.cjs', () => ({
  PLATFORM_PACKAGE_MISSING: 'OCTOCODE_PLATFORM_PACKAGE_MISSING',
  resolveNativeBinaryPath: mocks.resolve,
}));
/** A fresh module graph with counting fakes for the lazily loaded modules. */
async function freshDelegate() {
  vi.resetModules();
  loaded.fetcher = 0;
  loaded.config = 0;
  vi.doMock('../../../octocode-native/bin/fetch-binary.cjs', () => {
    loaded.fetcher += 1;
    return { fetchNativeBinary: mocks.fetch };
  });
  vi.doMock('@octocodeai/config', () => {
    loaded.config += 1;
    return {
      devOverridesAllowed: mocks.allowed,
      getOctocodeHome: () => '/home/.octocode',
    };
  });
  return import('../../src/cli/native-delegate.js');
}

describe('launcher fast path', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.resolve.mockReturnValue('/pkg/octocode');
    mocks.allowed.mockReturnValue(false);
  });

  it('resolves the installed binary without loading the fallback or config', async () => {
    const { resolveNativeBin } = await freshDelegate();
    await expect(resolveNativeBin({}, '1.2.3')).resolves.toEqual({
      bin: '/pkg/octocode',
    });
    expect(loaded).toEqual({ fetcher: 0, config: 0 });
  });

  it('loads config for OCTOCODE_NATIVE_BIN and applies its policy', async () => {
    const { resolveNativeBin } = await freshDelegate();
    const env = { OCTOCODE_NATIVE_BIN: '/no/such/octocode' };
    // Disallowed (production): the override is ignored, the package wins.
    await expect(resolveNativeBin(env, '1.2.3')).resolves.toEqual({
      bin: '/pkg/octocode',
    });
    mocks.allowed.mockReturnValue(true);
    await expect(resolveNativeBin(env, '1.2.3')).resolves.toEqual({
      error: expect.stringContaining('/no/such/octocode'),
    });
    expect(mocks.allowed).toHaveBeenCalledWith(env, { bundled: false });
    expect(loaded).toEqual({ fetcher: 0, config: 1 });
  });

  it('loads the fallback only when the platform package is missing', async () => {
    mocks.resolve.mockImplementation(() => {
      throw Object.assign(new Error("platform package 'x' is not installed."), {
        code: 'OCTOCODE_PLATFORM_PACKAGE_MISSING',
        suffix: 'linux-x64-gnu',
      });
    });
    mocks.fetch.mockResolvedValue('/home/.octocode/native/1.2.3/octocode');
    const { resolveNativeBin } = await freshDelegate();
    await expect(resolveNativeBin({}, '1.2.3')).resolves.toEqual({
      bin: '/home/.octocode/native/1.2.3/octocode',
    });
    expect(mocks.fetch).toHaveBeenCalledExactlyOnceWith({
      suffix: 'linux-x64-gnu',
      version: '1.2.3',
      home: '/home/.octocode',
      env: {},
    });
    expect(loaded).toEqual({ fetcher: 1, config: 1 });
  });
});
