import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  delegate: vi.fn(() => 0),
  resolve: vi.fn((): string | null => '/native/octocode'),
  skillHandler: vi.fn(),
}));

vi.mock('../../src/cli/native-delegate.js', () => ({
  shouldDelegateToNative: (command: string | null | undefined) =>
    command !== 'skill',
  resolveNativeBin: mocks.resolve,
  delegateToNative: mocks.delegate,
}));
vi.mock('../../src/cli/commands/skill.js', () => ({
  skillCommand: { name: 'skill', options: [], handler: mocks.skillHandler },
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

  it('keeps semanticAssess and its help/schema discovery on the native CLI', async () => {
    const { runCLI } = await import('../../src/cli/index.js');
    const invocation = ['semanticAssess', '{"resources":[],"questions":[]}'];
    await runCLI(invocation);
    expect(mocks.delegate).toHaveBeenLastCalledWith(
      '/native/octocode',
      invocation
    );

    await runCLI(['scheme', 'semanticAssess', '--compact']);
    expect(mocks.delegate).toHaveBeenLastCalledWith('/native/octocode', [
      'scheme',
      'semanticAssess',
      '--compact',
    ]);

    await runCLI(['semanticAssess', '--help']);
    expect(mocks.delegate).toHaveBeenLastCalledWith('/native/octocode', [
      'semanticAssess',
      '--help',
    ]);
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

  it('fails closed when the native runtime is unavailable', async () => {
    mocks.resolve.mockReturnValue(null);
    const { runCLI } = await import('../../src/cli/index.js');
    await expect(runCLI(['tools'])).rejects.toThrow(
      'native Octocode runtime is unavailable'
    );
  });
});
