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
    expect(output).toContain('ghSearchCode');
    expect(output).toContain('localSearchCode');
    expect(output).toContain('lspGetSemantics');
    expect(output).toContain('npmSearch');
    expect(output).toContain('install');
    // Smart commands temporarily unhooked — SMART COMMANDS section removed
    expect(output).toContain('MANAGEMENT');
    expect(output).toContain('TOOLS');
    expect(output).toContain('context');
    expect(output).toContain('tools');
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

  it('finds install command by name "install"', async () => {
    const { findStaticCommandHelp } =
      await import('../../src/cli/command-help-specs.js');
    const cmd = findStaticCommandHelp('install');
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
      'get',
      'tree',
      'files',
      'search',
      'pr',
      'repo',
      'pkg',
      'symbols',
      'lsp',
      'context',
    ];
    for (const name of names) {
      expect(findStaticCommandHelp(name)).toBeDefined();
    }
  });

  it('keeps static command help option lists documented and unique', async () => {
    const { COMMAND_SPECS } = await import('../../src/cli/commands/specs.js');

    const researchCommands = new Set([
      'get',
      'tree',
      'files',
      'search',
      'pr',
      'repo',
      'pkg',
      'symbols',
      'lsp',
    ]);

    for (const command of COMMAND_SPECS) {
      const seen = new Set<string>();
      expect(command.description.trim().length).toBeGreaterThan(0);
      expect(command.usage?.startsWith(`octocode ${command.name}`)).toBe(true);
      expect(command.scheme?.length).toBeGreaterThan(0);

      if (researchCommands.has(command.name)) {
        expect(command.whenToUse?.length).toBeGreaterThan(0);
        expect(command.examples?.length).toBeGreaterThan(0);
      }

      for (const option of command.options ?? []) {
        expect(option.name.trim().length).toBeGreaterThan(0);
        expect(option.description.trim().length).toBeGreaterThan(0);
        expect(seen.has(option.name)).toBe(false);
        seen.add(option.name);
      }
    }
  });

  it('documents full agent-critical usage flags', async () => {
    const { findStaticCommandHelp } =
      await import('../../src/cli/command-help-specs.js');

    expect(findStaticCommandHelp('get')!.usage).toContain('--full-content');
    expect(findStaticCommandHelp('search')!.usage).toContain('--branch <ref>');
    expect(findStaticCommandHelp('lsp')!.usage).toContain(
      '--workspace-root <path>'
    );
    expect(findStaticCommandHelp('lsp')!.usage).toContain(
      '--format structured|compact'
    );
    expect(findStaticCommandHelp('symbols')!.usage).toContain(
      '--page-size <n>'
    );
    expect(findStaticCommandHelp('install')!.usage).toContain(
      '--backup-path <path>'
    );
    expect(findStaticCommandHelp('auth')!.usage).toContain('--hostname <host>');
    expect(findStaticCommandHelp('token')!.usage).toContain('--reveal');
    expect(findStaticCommandHelp('context')!.usage).toContain('--context');
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
    expect(output).toContain('SCHEME');
    expect(output).toContain('OPTIONS');
    expect(output).toContain('required option: --ide supported client id');
    expect(output).toContain('--ide');
    expect(output).toContain('--method');
    expect(output).toContain('--force');

    stdoutSpy.mockRestore();
  });

  it('renders research command usage guidance and examples', async () => {
    const stdoutSpy = vi
      .spyOn(process.stdout, 'write')
      .mockImplementation(() => true);

    const { findStaticCommandHelp } =
      await import('../../src/cli/command-help-specs.js');
    const { showCommandHelp } = await import('../../src/cli/help.js');
    const cmd = findStaticCommandHelp('lsp')!;
    showCommandHelp(cmd);

    const output = stdoutSpy.mock.calls
      .map((c: unknown[]) => String(c[0]))
      .join('');
    expect(output).toContain('WHEN TO USE');
    expect(output).toContain('EXAMPLES');
    expect(output).toContain('SCHEME');
    expect(output).toContain(
      'required option: --type enum(definition|references'
    );
    expect(output).toContain('runtime: lspGetSemantics');
    expect(output).toContain('after search or symbols');
    expect(output).toContain('octocode lsp src/index.ts --type references');

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
    expect(output).toContain('env -> Octocode encrypted storage -> gh CLI');
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
    expect(output).toContain('--flag');
    expect(output).toContain('(default: yes)');
  });
});

describe('agent protocol help', () => {
  it('shows protocol with auth, tools, context, and skills steps', async () => {
    const stdoutSpy = vi
      .spyOn(console, 'log')
      .mockImplementation(() => undefined);

    const { printLightInstructions } =
      await import('../../src/cli/light-tool-help.js');
    printLightInstructions();

    const output = stdoutSpy.mock.calls
      .map((c: unknown[]) => c.map(String).join(' '))
      .join('\n');
    // Smart commands temporarily unhooked — fallback now shows protocol steps
    expect(output).toContain('octocode auth login');
    expect(output).toContain('octocode status');
    expect(output).toContain('octocode tools <name>');
    expect(output).toContain('octocode context');
    expect(output).toContain('octocode skills list');
    expect(output).toContain('octocode skills install --skill <name>');

    stdoutSpy.mockRestore();
  });
});
