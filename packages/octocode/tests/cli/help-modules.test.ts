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
    await showHelp();

    const output = stdoutSpy.mock.calls
      .map((c: unknown[]) => String(c[0]))
      .join('');
    expect(output).toContain('ghSearchCode');
    expect(output).toContain('Search code contents');
    expect(output).toContain('<AGENT_INSTRUCTIONS>');
    expect(output).toContain('localSearchCode');
    expect(output).toContain('Search local files');
    expect(output).toContain('lspGetSemantics');
    expect(output).toContain('npmSearch');
    expect(output).not.toContain('[path*');
    expect(output).toContain('install');
    // Command list is derived from core specs, so every command appears —
    // including lsp-server, which the old hardcoded MANAGEMENT block omitted.
    expect(output).toContain('MORE COMMANDS');
    expect(output).toContain('lsp-server');
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
      'status',
      'search',
      'skill',
      'clone',
      'unzip',
      'cache',
      'context',
    ];
    for (const name of names) {
      expect(findStaticCommandHelp(name)).toBeDefined();
    }
  });

  it('no longer exposes removed read-only shortcut command help', async () => {
    const { findStaticCommandHelp } =
      await import('../../src/cli/command-help-specs.js');
    for (const name of [
      'ast',
      'symbols',
      'cat',
      'ls',
      'find',
      'diff',
      'history',
      'repo',
      'pkg',
      'binary',
      'grep',
      'lsp',
    ]) {
      expect(findStaticCommandHelp(name)).toBeUndefined();
    }
  });

  it('search help teaches the two scheme views and the Haiku-gap recipes', async () => {
    const { findStaticCommandHelp } =
      await import('../../src/cli/command-help-specs.js');
    const search = findStaticCommandHelp('search');
    expect(search).toBeDefined();
    const blob = [
      ...(search!.scheme ?? []),
      ...(search!.examples ?? []),
      ...(search!.options ?? []).map(o => o.description),
    ].join('\n');

    // both schema entry points
    expect(blob).toContain('search --scheme --compact');
    expect(blob).toContain('search --scheme');
    // Haiku gaps: npm, remote file read, references-vs-callers
    expect(blob).toContain('--target packages');
    expect(blob).toContain(
      'search facebook/react/README.md --content-view exact'
    );
    expect(blob).toContain('callers = incoming calls only');
  });

  it('keeps static command help option lists documented and unique', async () => {
    const { COMMAND_SPECS } = await import('../../src/cli/commands/specs.js');

    const researchCommands = new Set([
      'search',
      'clone',
      'cache',
      'unzip',
      // management commands now carry agent guidance too
      'install',
      'auth',
      'status',
    ]);

    for (const command of COMMAND_SPECS) {
      const seen = new Set<string>();
      expect(command.description.trim().length).toBeGreaterThan(0);
      expect(command.usage?.startsWith(command.name)).toBe(true);
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

    expect(findStaticCommandHelp('search')!.usage).toContain('--lang');
    expect(findStaticCommandHelp('search')!.usage).toContain('--op');
    expect(findStaticCommandHelp('search')!.usage).toContain(
      '--target packages'
    );
    expect(findStaticCommandHelp('install')!.usage).toContain(
      '--backup-path <path>'
    );
    expect(findStaticCommandHelp('skill')!.usage).toContain(
      '--add <github-folder>'
    );
    expect(findStaticCommandHelp('auth')!.usage).toContain('--hostname <host>');
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
    const cmd = findStaticCommandHelp('search')!;
    showCommandHelp(cmd);

    const output = stdoutSpy.mock.calls
      .map((c: unknown[]) => String(c[0]))
      .join('');
    expect(output).toContain('WHEN TO USE');
    expect(output).toContain('EXAMPLES');
    expect(output).toContain('SCHEME');
    expect(output).toContain('Sources: Local path, GitHub owner/repo');
    expect(output).toContain(
      'Answer types / targets: code, content, structure'
    );
    // semantics (formerly the lsp command) is now reachable via search --op
    expect(output).toContain('--op');
    expect(output).toContain('documentSymbols');
    expect(output).toContain('search --scheme');

    stdoutSpy.mockRestore();
  });

  it('no longer exposes removed token and skills command help', async () => {
    const { findStaticCommandHelp } =
      await import('../../src/cli/command-help-specs.js');

    expect(findStaticCommandHelp('token')).toBeUndefined();
    expect(findStaticCommandHelp('skills')).toBeUndefined();
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
  it('shows protocol with login, auth status, tools, and context steps', async () => {
    const stdoutSpy = vi
      .spyOn(console, 'log')
      .mockImplementation(() => undefined);

    const { printLightInstructions } =
      await import('../../src/cli/light-tool-help.js');
    printLightInstructions();

    const output = stdoutSpy.mock.calls
      .map((c: unknown[]) => c.map(String).join(' '))
      .join('\n');
    // Smart commands temporarily unhooked — fallback now shows protocol steps.
    // Command examples omit the `octocode` prefix — agents know how to invoke the CLI.
    expect(output).toContain('login');
    expect(output).toContain('auth status');
    expect(output).toContain('tools <name>');
    expect(output).toContain('context');
    expect(output).toContain('auth status --json');

    stdoutSpy.mockRestore();
  });
});
