import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  delegate: vi.fn(() => 0),
  resolve: vi.fn((): string | null => '/native/octocode'),
  skillHandler: vi.fn(),
  schemeHandler: vi.fn(),
}));

vi.mock('../../src/cli/native-delegate.js', () => ({
  shouldDelegateToNative: (command: string | null | undefined) =>
    command !== 'skill' && command !== 'scheme',
  resolveNativeBin: mocks.resolve,
  delegateToNative: mocks.delegate,
}));
vi.mock('../../src/cli/commands/skill.js', () => ({
  skillCommand: { name: 'skill', options: [], handler: mocks.skillHandler },
}));
vi.mock('../../src/cli/commands/scheme.js', () => ({
  schemeCommand: { name: 'scheme', handler: mocks.schemeHandler },
}));
vi.mock('../../src/cli/stale-build.js', () => ({
  maybeWarnAboutStaleBuild: vi.fn(),
}));

describe('runCLI native boundary', () => {
  const originalExitCode = process.exitCode;

  beforeEach(() => {
    vi.clearAllMocks();
    process.exitCode = undefined;
    mocks.resolve.mockReturnValue('/native/octocode');
    mocks.delegate.mockReturnValue(0);
  });

  afterEach(() => {
    process.exitCode = originalExitCode;
  });

  it('delegates public tool commands without interpreting their arguments', async () => {
    const { runCLI } = await import('../../src/cli/index.js');
    const argv = ['localFetch', '{"path":"/tmp/a","reasoning":"test"}'];
    await expect(runCLI(argv)).resolves.toBe(true);
    expect(mocks.delegate).toHaveBeenCalledWith('/native/octocode', argv);
    expect(mocks.skillHandler).not.toHaveBeenCalled();
  });

  it('keeps clasify execution and help native while scheme discovery stays Node-owned', async () => {
    const { runCLI } = await import('../../src/cli/index.js');
    const invocation = ['clasify', '{"resources":[],"questions":[]}'];
    await runCLI(invocation);
    expect(mocks.delegate).toHaveBeenLastCalledWith(
      '/native/octocode',
      invocation
    );

    await runCLI(['scheme', 'clasify', '--compact']);
    expect(mocks.schemeHandler).toHaveBeenLastCalledWith(
      expect.objectContaining({
        command: 'scheme',
        args: ['clasify'],
        options: expect.objectContaining({ compact: true }),
      })
    );
    expect(mocks.delegate).toHaveBeenCalledTimes(1);

    await runCLI(['clasify', '--help']);
    expect(mocks.delegate).toHaveBeenLastCalledWith('/native/octocode', [
      'clasify',
      '--help',
    ]);
  });

  it('renders the agent overview (scheme catalog) for a bare invocation', async () => {
    const { runCLI } = await import('../../src/cli/index.js');
    await runCLI([]);
    expect(mocks.schemeHandler).toHaveBeenCalledWith(
      expect.objectContaining({ command: 'scheme', args: [] })
    );
    expect(mocks.delegate).not.toHaveBeenCalled();
  });

  it('delegates top-level help, version, and unknown commands to native parsing', async () => {
    const { runCLI } = await import('../../src/cli/index.js');
    await runCLI(['--help']);
    expect(mocks.delegate).toHaveBeenLastCalledWith('/native/octocode', [
      '--help',
    ]);
    await runCLI(['--version']);
    expect(mocks.delegate).toHaveBeenLastCalledWith('/native/octocode', [
      '--version',
    ]);
    await runCLI(['unknown', '--flag']);
    expect(mocks.delegate).toHaveBeenLastCalledWith('/native/octocode', [
      'unknown',
      '--flag',
    ]);
  });

  it('prints launcher and native versions without delegating', async () => {
    vi.doMock('../../src/cli/version.js', () => ({
      versionLine: () => 'octocode 1.2.3 (native 4.5.6)',
    }));
    const write = vi
      .spyOn(process.stdout, 'write')
      .mockImplementation(() => true);
    try {
      const { runCLI } = await import('../../src/cli/index.js');
      mocks.delegate.mockClear();
      await runCLI(['--version']);
      expect(write).toHaveBeenCalledWith('octocode 1.2.3 (native 4.5.6)\n');
      expect(mocks.delegate).not.toHaveBeenCalled();
    } finally {
      write.mockRestore();
      vi.doUnmock('../../src/cli/version.js');
    }
  });

  it('runs skill materialization in Node', async () => {
    const { runCLI } = await import('../../src/cli/index.js');
    await runCLI(['skill', 'list']);
    expect(mocks.skillHandler).toHaveBeenCalledWith(
      expect.objectContaining({ command: 'skill', args: ['list'] })
    );
    expect(mocks.delegate).not.toHaveBeenCalled();
  });

  it('routes skill --help into the Node skill command', async () => {
    const { runCLI } = await import('../../src/cli/index.js');
    await runCLI(['skill', '--help']);
    expect(mocks.skillHandler).toHaveBeenCalledWith(
      expect.objectContaining({
        command: 'skill',
        options: expect.objectContaining({ help: true }),
      })
    );
    expect(mocks.delegate).not.toHaveBeenCalled();
  });

  it('fails closed with exit 5 when the native runtime is unavailable', async () => {
    mocks.resolve.mockReturnValue(null);
    const { runCLI } = await import('../../src/cli/index.js');
    const stderr = vi.spyOn(process.stderr, 'write').mockReturnValue(true);
    const previousExit = process.exitCode;
    try {
      const result = await runCLI(['tools']);
      // Execution failure (exit 5), matching the native exit-code table and the
      // `scheme` path — not a thrown generic exit 1.
      expect(result).toBe(false);
      expect(process.exitCode).toBe(5);
      expect(stderr).toHaveBeenCalledWith(
        expect.stringContaining('native Octocode runtime is unavailable')
      );
    } finally {
      process.exitCode = previousExit;
      stderr.mockRestore();
    }
  });
});
