import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  delegate: vi.fn(async () => 0),
  resolve: vi.fn(async (): Promise<{ bin: string } | { error: string }> => ({
    bin: '/native/octocode',
  })),
}));

vi.mock('../../src/cli/native-delegate.js', async importOriginal => ({
  ...(await importOriginal<
    typeof import('../../src/cli/native-delegate.js')
  >()),
  resolveNativeBin: mocks.resolve,
  delegateToNative: mocks.delegate,
}));

import { runCLI } from '../../src/cli/index.js';

describe('runCLI native boundary', () => {
  const originalExitCode = process.exitCode;

  beforeEach(() => {
    vi.clearAllMocks();
    process.exitCode = undefined;
    mocks.resolve.mockResolvedValue({ bin: '/native/octocode' });
    mocks.delegate.mockResolvedValue(0);
  });

  afterEach(() => {
    process.exitCode = originalExitCode;
  });

  it.each([
    [],
    ['--help'],
    ['--version'],
    ['--version', '--bogus'],
    ['--', '--help'],
    ['install'],
    ['install', '--json=true'],
    ['config', 'view', '--no-open'],
    ['config', 'view', '--json', '--idle-timeout=abc'],
    ['skill', 'list', '--json'],
    ['skill', 'install', '--all', '--platform', 'codex', '--global'],
    ['schema', 'localFetch', '--view', 'query'],
    ['localFetch', '{"queries":[{"path":"a b","ranges":["1-2"]}]}'],
    ['unknown', '--flag=value=with=equals'],
  ])('forwards arguments unchanged: %j', async (...argv) => {
    const original = [...argv];
    await runCLI(Object.freeze(argv));
    expect(argv).toEqual(original);
    expect(mocks.delegate).toHaveBeenCalledExactlyOnceWith(
      '/native/octocode',
      original
    );
  });

  it('leaves terminal behavior to native', async () => {
    const saved = Object.getOwnPropertyDescriptor(process.stdout, 'isTTY');
    Object.defineProperty(process.stdout, 'isTTY', {
      value: true,
      configurable: true,
    });
    try {
      await runCLI([]);
      expect(mocks.delegate).toHaveBeenCalledExactlyOnceWith(
        '/native/octocode',
        []
      );
    } finally {
      if (saved) Object.defineProperty(process.stdout, 'isTTY', saved);
      else delete (process.stdout as { isTTY?: boolean }).isTTY;
    }
  });

  it.each([0, 2, 5, 130, 143])('preserves native exit code %i', async code => {
    mocks.delegate.mockResolvedValue(code);
    await runCLI(['config']);
    expect(process.exitCode).toBe(code);
  });

  it('reports resolution failure without interpreting CLI flags', async () => {
    mocks.resolve.mockResolvedValue({
      error: 'Missing platform package; npm install it.',
    });
    await expect(runCLI(['--json'])).rejects.toThrow(
      'Missing platform package; npm install it.'
    );
    expect(mocks.delegate).not.toHaveBeenCalled();
  });

  it('propagates spawn failure to the launcher boundary', async () => {
    mocks.delegate.mockRejectedValue(new Error('EACCES'));
    await expect(runCLI([])).rejects.toThrow('EACCES');
  });
});
