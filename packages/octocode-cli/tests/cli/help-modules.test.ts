import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

describe('main-help', () => {
  let stdoutSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    stdoutSpy = vi
      .spyOn(process.stdout, 'write')
      .mockImplementation(() => true);
  });

  afterEach(() => {
    stdoutSpy.mockRestore();
  });

  it('renders top-level help with commands and tools sections', async () => {
    const { showHelp } = await import('../../src/cli/main-help.js');
    showHelp();

    const output = stdoutSpy.mock.calls
      .map((c: unknown[]) => String(c[0]))
      .join('');
    expect(output).toContain('githubSearchCode');
    expect(output).toContain('localSearchCode');
    expect(output).toContain('lspGotoDefinition');
    expect(output).toContain('packageSearch');
    expect(output).toContain('install');
    expect(output).toContain('COMMANDS');
    expect(output).toContain('TOOLS');
    expect(output).toContain('OPTIONS');
    expect(output).toContain('EXAMPLES');
    expect(output).toContain('instructions');
    expect(output).toContain('tools');
    expect(output).toContain('--queries');
  });
});

describe('command-help-specs', () => {
  it('finds install command by name', async () => {
    const { findStaticCommandHelp } =
      await import('../../src/cli/command-help-specs.js');
    const cmd = findStaticCommandHelp('install');
    expect(cmd).toBeDefined();
    expect(cmd!.name).toBe('install');
  });

  it('finds install command by alias "setup"', async () => {
    const { findStaticCommandHelp } =
      await import('../../src/cli/command-help-specs.js');
    const cmd = findStaticCommandHelp('setup');
    expect(cmd).toBeDefined();
    expect(cmd!.name).toBe('install');
  });

  it('finds all expected static commands', async () => {
    const { findStaticCommandHelp } =
      await import('../../src/cli/command-help-specs.js');
    const names = [
      'install',
      'auth',
      'login',
      'logout',
      'skills',
      'token',
      'status',
      'sync',
      'mcp',
      'cache',
    ];
    for (const name of names) {
      expect(findStaticCommandHelp(name)).toBeDefined();
    }
  });

  it('returns undefined for unknown commands', async () => {
    const { findStaticCommandHelp } =
      await import('../../src/cli/command-help-specs.js');
    expect(findStaticCommandHelp('nonexistent')).toBeUndefined();
  });

  it('renders static command help via shared showCommandHelp', async () => {
    const stdoutSpy = vi
      .spyOn(process.stdout, 'write')
      .mockImplementation(() => true);

    const { findStaticCommandHelp } =
      await import('../../src/cli/command-help-specs.js');
    const { showCommandHelp } = await import('../../src/cli/help.js');
    const cmd = findStaticCommandHelp('install')!;
    showCommandHelp(cmd);

    const output = stdoutSpy.mock.calls
      .map((c: unknown[]) => String(c[0]))
      .join('');
    expect(output).toContain('install');
    expect(output).toContain('USAGE');
    expect(output).toContain('OPTIONS');
    expect(output).toContain('--ide');
    expect(output).toContain('--method');
    expect(output).toContain('--force');

    stdoutSpy.mockRestore();
  });

  it('does not rewrite token source names inside usage', async () => {
    const stdoutSpy = vi
      .spyOn(process.stdout, 'write')
      .mockImplementation(() => true);

    const { findStaticCommandHelp } =
      await import('../../src/cli/command-help-specs.js');
    const { showCommandHelp } = await import('../../src/cli/help.js');
    const cmd = findStaticCommandHelp('token')!;
    showCommandHelp(cmd);

    const output = stdoutSpy.mock.calls
      .map((c: unknown[]) => String(c[0]))
      .join('');
    expect(output).toContain('--type <auto|octocode|gh>');
    expect(output).toContain('env→octocode→gh');
    expect(output).not.toContain('auto|octocode-cli|gh');

    stdoutSpy.mockRestore();
  });
});

describe('help (dynamic fallback)', () => {
  let stdoutSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    stdoutSpy = vi
      .spyOn(process.stdout, 'write')
      .mockImplementation(() => true);
  });

  afterEach(() => {
    stdoutSpy.mockRestore();
  });

  it('renders dynamic command help with usage and options', async () => {
    const { showCommandHelp } = await import('../../src/cli/help.js');
    showCommandHelp({
      name: 'test-cmd',
      description: 'A test command',
      usage: 'octocode test-cmd --flag',
      options: [
        {
          name: 'flag',
          short: 'f',
          description: 'A flag',
          hasValue: true,
          default: 'yes',
        },
        { name: 'bool', description: 'Boolean flag' },
      ],
    });

    const output = stdoutSpy.mock.calls
      .map((c: unknown[]) => String(c[0]))
      .join('');
    expect(output).toContain('test-cmd');
    expect(output).toContain('A test command');
    expect(output).toContain('USAGE');
    expect(output).toContain('octocode test-cmd --flag');
    expect(output).toContain('OPTIONS');
    expect(output).toContain('-f, --flag');
    expect(output).toContain('(default: yes)');
  });
});
