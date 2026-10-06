import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  delegate: vi.fn(() => 0),
  configView: vi.fn(async () => 0),
  resolve: vi.fn((): string | null => '/native/octocode'),
  skillHandler: vi.fn(),
  schemeHandler: vi.fn(),
  printInstructions: vi.fn(() => 0),
  setRuntimeSurface: vi.fn(),
}));

vi.mock('@octocodeai/config', async importOriginal => ({
  ...(await importOriginal<typeof import('@octocodeai/config')>()),
  setRuntimeSurface: mocks.setRuntimeSurface,
}));

vi.mock('../../src/cli/native-delegate.js', async importOriginal => ({
  ...(await importOriginal<
    typeof import('../../src/cli/native-delegate.js')
  >()),
  resolveNativeBin: mocks.resolve,
  delegateToNative: mocks.delegate,
}));
vi.mock('../../src/cli/commands/config-view.js', () => ({
  configViewCommand: mocks.configView,
}));
vi.mock('../../src/cli/commands/skill.js', () => ({
  skillCommand: { name: 'skill', options: [], handler: mocks.skillHandler },
}));
vi.mock('../../src/cli/commands/scheme.js', () => ({
  schemeCommand: { name: 'scheme', handler: mocks.schemeHandler },
  printAgentInstructions: mocks.printInstructions,
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

  it('starts the browser session only for config view and forwards its help to native', async () => {
    const { runCLI } = await import('../../src/cli/index.js');
    await runCLI(['config', 'view', '--no-open']);
    expect(mocks.configView).toHaveBeenCalledWith('/native/octocode', [
      'config',
      'view',
      '--no-open',
    ]);
    expect(mocks.delegate).not.toHaveBeenCalled();
    await runCLI(['config', 'view', '--help']);
    expect(mocks.delegate).toHaveBeenLastCalledWith('/native/octocode', [
      'config',
      'view',
      '--help',
    ]);
  });

  it('delegates public tool commands without interpreting their arguments', async () => {
    const { runCLI } = await import('../../src/cli/index.js');
    const argv = ['localFetch', '{"path":"/tmp/a","reasoning":"test"}'];
    await expect(runCLI(argv)).resolves.toBe(true);
    expect(mocks.delegate).toHaveBeenCalledWith('/native/octocode', argv);
    expect(mocks.skillHandler).not.toHaveBeenCalled();
  });

  it('loads the config surface only for commands Node handles in-process', async () => {
    const { runCLI } = await import('../../src/cli/index.js');
    await runCLI(['localSearch', '{"queries":[]}']);
    await runCLI(['localSearch', '--help']);
    expect(mocks.setRuntimeSurface).not.toHaveBeenCalled();
    await runCLI(['scheme']);
    expect(mocks.setRuntimeSurface).toHaveBeenLastCalledWith('cli');
    mocks.setRuntimeSurface.mockClear();
    await runCLI(['--help']);
    expect(mocks.setRuntimeSurface).toHaveBeenCalledTimes(1);
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

  it.each([['--help'], ['-h'], ['help']])(
    'appends canonical instructions to root help %j',
    async (...argv) => {
      const { runCLI } = await import('../../src/cli/index.js');
      await runCLI(argv);
      expect(mocks.delegate).toHaveBeenCalledWith('/native/octocode', argv);
      expect(mocks.printInstructions).toHaveBeenCalledTimes(1);
    }
  );

  it('does not append instructions to subcommand help or failed parsing', async () => {
    const { runCLI } = await import('../../src/cli/index.js');
    await runCLI(['help', 'localSearch']);
    await runCLI(['localSearch', '--help']);
    expect(mocks.printInstructions).not.toHaveBeenCalled();
    mocks.delegate.mockReturnValue(2);
    await runCLI(['--help']);
    expect(mocks.printInstructions).not.toHaveBeenCalled();
    expect(process.exitCode).toBe(2);
  });

  it('keeps root help exit 0 when the instructions hit contract drift', async () => {
    mocks.printInstructions.mockReturnValueOnce(5);
    const { runCLI } = await import('../../src/cli/index.js');
    await runCLI(['--help']);
    expect(mocks.printInstructions).toHaveBeenCalledTimes(1);
    expect(process.exitCode).toBe(0);
  });

  it('normalizes boolean flags given as --flag=value', async () => {
    const { isTrueFlag } = await import('../../src/cli/index.js');
    expect(isTrueFlag(true)).toBe(true);
    expect(isTrueFlag('true')).toBe(true);
    expect(isTrueFlag('TRUE')).toBe(true);
    expect(isTrueFlag('false')).toBe(false);
    expect(isTrueFlag(undefined)).toBe(false);
  });

  it('does not open the interactive picker for install --json=true', async () => {
    const interactive = vi.fn(async () => 0);
    vi.doMock('../../src/cli/interactive-install.js', () => ({
      runInteractiveInstall: interactive,
    }));
    const inTty = Object.getOwnPropertyDescriptor(process.stdin, 'isTTY');
    const outTty = Object.getOwnPropertyDescriptor(process.stdout, 'isTTY');
    Object.defineProperty(process.stdin, 'isTTY', {
      value: true,
      configurable: true,
    });
    Object.defineProperty(process.stdout, 'isTTY', {
      value: true,
      configurable: true,
    });
    try {
      const { runCLI } = await import('../../src/cli/index.js');
      await runCLI(['install', '--json=true']);
      expect(interactive).not.toHaveBeenCalled();
      expect(mocks.delegate).toHaveBeenCalledWith('/native/octocode', [
        'install',
        '--json=true',
      ]);
      await runCLI(['install']);
      expect(interactive).toHaveBeenCalledTimes(1);
    } finally {
      for (const [stream, d] of [
        [process.stdin, inTty],
        [process.stdout, outTty],
      ] as const) {
        if (d) Object.defineProperty(stream, 'isTTY', d);
        else delete (stream as { isTTY?: boolean }).isTTY;
      }
      vi.doUnmock('../../src/cli/interactive-install.js');
    }
  });

  it('prints the version line without delegating', async () => {
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
