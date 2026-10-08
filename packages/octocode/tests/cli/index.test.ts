import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  delegate: vi.fn(() => 0),
  configView: vi.fn(async () => 0),
  resolve: vi.fn((): string | null => '/native/octocode'),
  skillHandler: vi.fn(),
  schemaHandler: vi.fn(),
  toolHelp: vi.fn(async (_bin: string, name: string) =>
    name === 'clasify' ? 0 : undefined
  ),
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
vi.mock('../../src/cli/commands/tool-help.js', () => ({
  runToolHelp: mocks.toolHelp,
}));
vi.mock('../../src/cli/commands/schema.js', () => ({
  schemaCommand: { name: 'schema', handler: mocks.schemaHandler },
}));

/** Run `body` with `process.stdout.isTTY` set to `tty`. */
async function withStdoutTty(tty: boolean, body: () => Promise<void>) {
  const saved = Object.getOwnPropertyDescriptor(process.stdout, 'isTTY');
  Object.defineProperty(process.stdout, 'isTTY', {
    value: tty,
    configurable: true,
  });
  try {
    await body();
  } finally {
    if (saved) Object.defineProperty(process.stdout, 'isTTY', saved);
    else delete (process.stdout as { isTTY?: boolean }).isTTY;
  }
}
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

  it('keeps clasify execution and help native while schema discovery stays Node-owned', async () => {
    const { runCLI } = await import('../../src/cli/index.js');
    const invocation = ['clasify', '{"resources":[],"questions":[]}'];
    await runCLI(invocation);
    expect(mocks.delegate).toHaveBeenLastCalledWith(
      '/native/octocode',
      invocation
    );

    await runCLI(['schema', 'clasify', '--view', 'query']);
    expect(mocks.schemaHandler).toHaveBeenLastCalledWith(
      expect.objectContaining({
        command: 'schema',
        args: ['clasify'],
        options: expect.objectContaining({ view: 'query' }),
      })
    );
    expect(mocks.delegate).toHaveBeenCalledTimes(1);

    // The binary owns `schema --help` and `help schema`.
    await runCLI(['schema', '--help']);
    expect(mocks.delegate).toHaveBeenLastCalledWith('/native/octocode', [
      'schema',
      '--help',
    ]);
    await runCLI(['help', 'schema']);
    expect(mocks.delegate).toHaveBeenLastCalledWith('/native/octocode', [
      'help',
      'schema',
    ]);
    expect(mocks.schemaHandler).toHaveBeenCalledTimes(1);

    // Tool help is the binary's help plus the catalog's input section.
    await runCLI(['clasify', '--help']);
    await runCLI(['help', 'clasify']);
    expect(mocks.toolHelp.mock.calls).toEqual([
      ['/native/octocode', 'schema'],
      ['/native/octocode', 'schema'],
      ['/native/octocode', 'clasify'],
      ['/native/octocode', 'clasify'],
    ]);
    expect(mocks.delegate).toHaveBeenCalledTimes(3);
  });

  it('renders the schema catalog for a bare invocation on a pipe', async () => {
    const { runCLI } = await import('../../src/cli/index.js');
    await withStdoutTty(false, async () => {
      await runCLI([]);
    });
    expect(mocks.schemaHandler).toHaveBeenCalledWith(
      expect.objectContaining({ command: 'schema', args: [] })
    );
    expect(mocks.delegate).not.toHaveBeenCalled();
  });

  it('prints native root help for a bare invocation on a terminal', async () => {
    const { runCLI } = await import('../../src/cli/index.js');
    await withStdoutTty(true, async () => {
      await runCLI([]);
    });
    expect(mocks.delegate).toHaveBeenCalledWith('/native/octocode', ['--help']);
    expect(mocks.schemaHandler).not.toHaveBeenCalled();
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
    'delegates root help %j to native without appending instructions',
    async (...argv) => {
      const { runCLI } = await import('../../src/cli/index.js');
      await runCLI(argv);
      expect(mocks.delegate).toHaveBeenCalledWith('/native/octocode', argv);
      expect(mocks.delegate).toHaveBeenCalledTimes(1);
      expect(mocks.schemaHandler).not.toHaveBeenCalled();
    }
  );

  it('passes the native help exit code through', async () => {
    mocks.delegate.mockReturnValue(2);
    const { runCLI } = await import('../../src/cli/index.js');
    await runCLI(['--help']);
    expect(process.exitCode).toBe(2);
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
      // `schema` path — not a thrown generic exit 1.
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
