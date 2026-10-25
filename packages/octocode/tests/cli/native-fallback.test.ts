import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  resolve: vi.fn<() => string>(),
  fetch: vi.fn<(options: Record<string, unknown>) => Promise<string>>(),
}));

vi.mock('../../../octocode-native/bin/resolve-binary.cjs', () => ({
  PLATFORM_PACKAGE_MISSING: 'OCTOCODE_PLATFORM_PACKAGE_MISSING',
  resolveNativeBinaryPath: mocks.resolve,
}));
vi.mock('../../../octocode-native/bin/fetch-binary.cjs', () => ({
  fetchNativeBinary: mocks.fetch,
}));

import { resolveNativeBin } from '../../src/cli/native-delegate.js';

const missing = () =>
  Object.assign(new Error("platform package 'x' is not installed."), {
    code: 'OCTOCODE_PLATFORM_PACKAGE_MISSING',
    suffix: 'linux-arm64-musl',
  });

describe('resolveNativeBin fallback for a skipped platform package', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.resolve.mockImplementation(() => {
      throw missing();
    });
  });

  it('fetches the pinned platform package into the Octocode home', async () => {
    mocks.fetch.mockResolvedValue(
      '/home/.octocode/native/1.2.3/linux-arm64-musl/octocode'
    );
    const env = { OCTOCODE_HOME: '/home/.octocode' };

    await expect(resolveNativeBin(env, '1.2.3')).resolves.toEqual({
      bin: '/home/.octocode/native/1.2.3/linux-arm64-musl/octocode',
    });
    expect(mocks.fetch).toHaveBeenCalledExactlyOnceWith({
      suffix: 'linux-arm64-musl',
      version: '1.2.3',
      home: expect.stringMatching(/[\\/]home[\\/]\.octocode$/),
      env,
    });
  });

  it('keeps both reasons when the download fails', async () => {
    mocks.fetch.mockRejectedValue(new Error('GET … answered HTTP 404'));
    const resolved = await resolveNativeBin({}, '1.2.3');
    expect(resolved).toEqual({
      error: expect.stringContaining('is not installed'),
    });
    expect('error' in resolved && resolved.error).toContain('HTTP 404');
  });

  it('never downloads without a pinned version (unbundled source runs)', async () => {
    await expect(resolveNativeBin({}, undefined)).resolves.toEqual({
      error: expect.stringContaining('is not installed'),
    });
    expect(mocks.fetch).not.toHaveBeenCalled();
  });

  it('never downloads for another resolver failure', async () => {
    mocks.resolve.mockImplementation(() => {
      throw new Error("unsupported platform 'plan9-x64'");
    });
    await expect(resolveNativeBin({}, '1.2.3')).resolves.toEqual({
      error: expect.stringContaining('unsupported platform'),
    });
    expect(mocks.fetch).not.toHaveBeenCalled();
  });
});
