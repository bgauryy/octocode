import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  loadCommand: vi.fn(),
  delegate: vi.fn(() => 0),
  resolve: vi.fn((): string | null => '/native/octocode'),
  showHelp: vi.fn(),
}));

vi.mock('../../src/cli/native-delegate.js', () => ({
  shouldDelegateToNative: (command: string | null | undefined) =>
    command !== 'skill',
  resolveNativeBin: mocks.resolve,
  delegateToNative: mocks.delegate,
}));
vi.mock('../../src/cli/commands/index.js', () => ({
  loadCommand: mocks.loadCommand,
}));
vi.mock('../../src/cli/help.js', () => ({ showCommandHelp: mocks.showHelp }));
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
    const argv = ['tools', 'localFetch', '--queries', '{"path":"/tmp/a"}'];
    await expect(runCLI(argv)).resolves.toBe(true);
    expect(mocks.delegate).toHaveBeenCalledWith('/native/octocode', argv);
    expect(mocks.loadCommand).not.toHaveBeenCalled();
  });

  it('delegates top-level help and unknown commands to native parsing', async () => {
    const { runCLI } = await import('../../src/cli/index.js');
    await runCLI(['--help']);
    expect(mocks.delegate).toHaveBeenLastCalledWith('/native/octocode', [
      '--help',
    ]);
    await runCLI(['unknown', '--flag']);
    expect(mocks.delegate).toHaveBeenLastCalledWith('/native/octocode', [
      'unknown',
      '--flag',
    ]);
  });

  it('runs skill materialization in Node', async () => {
    const handler = vi.fn();
    mocks.loadCommand.mockResolvedValue({
      name: 'skill',
      options: [],
      handler,
    });
    const { runCLI } = await import('../../src/cli/index.js');
    await runCLI(['skill', 'list']);
    expect(handler).toHaveBeenCalledWith(
      expect.objectContaining({ command: 'skill', args: ['list'] })
    );
    expect(mocks.delegate).not.toHaveBeenCalled();
  });

  it('renders Node-owned skill help from its local spec', async () => {
    const { runCLI } = await import('../../src/cli/index.js');
    await runCLI(['skill', '--help']);
    expect(mocks.showHelp).toHaveBeenCalledWith(
      expect.objectContaining({ name: 'skill' })
    );
    expect(mocks.loadCommand).not.toHaveBeenCalled();
  });

  it('fails closed when the native runtime is unavailable', async () => {
    mocks.resolve.mockReturnValue(null);
    const { runCLI } = await import('../../src/cli/index.js');
    await expect(runCLI(['tools'])).rejects.toThrow(
      'native Octocode runtime is unavailable'
    );
  });
});
