import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const resolveNativeBin = vi.fn();
const delegateToNative = vi.fn();
const failStartup = vi.fn();

vi.mock(
  import('../../../octocode/src/cli/native-delegate.js'),
  async importOriginal => ({
    ...(await importOriginal()),
    resolveNativeBin,
    delegateToNative,
  })
);
vi.mock('../../src/startup-failure.js', () => ({ failStartup }));

async function launch(): Promise<void> {
  vi.resetModules();
  await import('../../src/index.js');
  await vi.waitFor(() => {
    expect(
      delegateToNative.mock.calls.length + failStartup.mock.calls.length
    ).toBeGreaterThan(0);
  });
}

describe('launcher', () => {
  const argv = process.argv;
  const exitCode = process.exitCode;

  beforeEach(() => {
    vi.clearAllMocks();
  });
  afterEach(() => {
    process.argv = argv;
    process.exitCode = exitCode;
  });

  it('starts the native binary as `octocode mcp` and exits with its code', async () => {
    process.argv = ['node', 'octocode-mcp', '--flag'];
    resolveNativeBin.mockReturnValue({ bin: '/native/octocode' });
    delegateToNative.mockResolvedValue(3);

    await launch();
    await vi.waitFor(() => expect(process.exitCode).toBe(3));

    expect(delegateToNative).toHaveBeenCalledWith('/native/octocode', [
      'mcp',
      '--flag',
    ]);
    expect(failStartup).not.toHaveBeenCalled();
  });

  it('reports an unresolvable binary as a startup failure', async () => {
    resolveNativeBin.mockReturnValue({ error: 'no binary for plan9' });

    await launch();

    expect(failStartup).toHaveBeenCalledWith(
      'Server initialization failed: no binary for plan9'
    );
    expect(delegateToNative).not.toHaveBeenCalled();
  });

  it('reports a binary that cannot start as a startup failure', async () => {
    resolveNativeBin.mockReturnValue({ bin: '/native/octocode' });
    delegateToNative.mockRejectedValue(new Error('spawn EACCES'));

    await launch();

    expect(failStartup).toHaveBeenCalledWith(
      'Server initialization failed: spawn EACCES'
    );
  });
});
