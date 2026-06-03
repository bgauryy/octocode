import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  findCommand: vi.fn(),
  showHelp: vi.fn(),
  showCommandHelp: vi.fn(),
  showToolHelp: vi.fn(),
  findStaticCommandHelp: vi.fn(),
  executeToolCommand: vi.fn().mockResolvedValue(true),
  printToolsContext: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../src/cli/commands.js', () => ({
  findCommand: mocks.findCommand,
}));

vi.mock('../../src/cli/help.js', () => ({
  showCommandHelp: mocks.showCommandHelp,
}));

vi.mock('../../src/cli/main-help.js', () => ({
  showHelp: mocks.showHelp,
}));

vi.mock('../../src/cli/command-help-specs.js', () => ({
  findStaticCommandHelp: mocks.findStaticCommandHelp,
}));

vi.mock('../../src/cli/tool-command.js', () => ({
  showToolHelp: mocks.showToolHelp,
  executeToolCommand: mocks.executeToolCommand,
  printToolsContext: mocks.printToolsContext,
}));

describe('runCLI', () => {
  let originalExitCode: typeof process.exitCode;
  let consoleSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    originalExitCode = process.exitCode;
    process.exitCode = undefined;
    consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
  });

  afterEach(() => {
    process.exitCode = originalExitCode;
    consoleSpy.mockRestore();
  });

  it('handles --tools-context before command dispatch', async () => {
    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI(['--tools-context']);

    expect(handled).toBe(true);
    expect(mocks.printToolsContext).toHaveBeenCalledTimes(1);
    expect(mocks.findCommand).not.toHaveBeenCalled();
  });

  it('routes --tool usage through the unified tool executor', async () => {
    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI([
      '--tool',
      'localSearchCode',
      '--queries',
      '{"path":".","pattern":"runCLI"}',
    ]);

    expect(handled).toBe(true);
    expect(mocks.executeToolCommand).toHaveBeenCalledTimes(1);
    expect(mocks.executeToolCommand).toHaveBeenCalledWith({
      command: 'tool',
      args: ['localSearchCode'],
      options: {
        tool: 'localSearchCode',
        queries: '{"path":".","pattern":"runCLI"}',
      },
    });
    expect(mocks.findCommand).not.toHaveBeenCalled();
  });

  it('routes github --tool usage through the unified tool executor', async () => {
    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI([
      '--tool',
      'githubSearchCode',
      '--queries',
      '{"owner":"bgauryy","repo":"octocode-mcp","keywordsToSearch":["tool"]}',
      '--output',
      'json',
    ]);

    expect(handled).toBe(true);
    expect(mocks.executeToolCommand).toHaveBeenCalledTimes(1);
    expect(mocks.executeToolCommand).toHaveBeenCalledWith({
      command: 'tool',
      args: ['githubSearchCode'],
      options: {
        tool: 'githubSearchCode',
        queries:
          '{"owner":"bgauryy","repo":"octocode-mcp","keywordsToSearch":["tool"]}',
        output: 'json',
      },
    });
    expect(mocks.findCommand).not.toHaveBeenCalled();
  });

  it('shows dynamic tool help for --tool --help usage', async () => {
    mocks.showToolHelp.mockResolvedValue(true);

    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI(['--tool', 'localSearchCode', '--help']);

    expect(handled).toBe(true);
    expect(mocks.showToolHelp).toHaveBeenCalledTimes(1);
    expect(mocks.showToolHelp).toHaveBeenCalledWith('localSearchCode');
    expect(mocks.executeToolCommand).not.toHaveBeenCalled();
    expect(mocks.findCommand).not.toHaveBeenCalled();
  });

  it('shows dynamic tool help for any tool --help', async () => {
    mocks.showToolHelp.mockResolvedValue(true);

    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI(['--tool', 'lspGotoDefinition', '--help']);

    expect(handled).toBe(true);
    expect(mocks.showToolHelp).toHaveBeenCalledTimes(1);
    expect(mocks.showToolHelp).toHaveBeenCalledWith('lspGotoDefinition');
    expect(mocks.executeToolCommand).not.toHaveBeenCalled();
  });

  it('rejects the legacy tool command and points users to --tool', async () => {
    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI([
      'tool',
      'localSearchCode',
      '{"path":".","pattern":"runCLI"}',
    ]);

    expect(handled).toBe(true);
    expect(mocks.executeToolCommand).not.toHaveBeenCalled();
    expect(mocks.findCommand).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Use octocode --tool')
    );
    expect(process.exitCode).toBe(1);
  });

  it('shows main help when --help is passed without a command', async () => {
    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI(['--help']);

    expect(handled).toBe(true);
    expect(mocks.showHelp).toHaveBeenCalledTimes(1);
    expect(mocks.showCommandHelp).not.toHaveBeenCalled();
  });

  it('shows main help for "tool --help" (from main-help, not help module)', async () => {
    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI(['tool', '--help']);

    expect(handled).toBe(true);
    expect(mocks.showHelp).toHaveBeenCalledTimes(1);
    expect(mocks.showCommandHelp).not.toHaveBeenCalled();
  });

  it('falls through to main help for unknown command --help', async () => {
    mocks.findStaticCommandHelp.mockReturnValue(undefined);
    mocks.findCommand.mockReturnValue(undefined);

    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI(['nonexistent', '--help']);

    expect(handled).toBe(true);
    expect(mocks.showHelp).toHaveBeenCalledTimes(1);
  });

  it('shows static command help for "install --help" using shared renderer', async () => {
    const fakeCmd = { name: 'install', description: 'Configure octocode-mcp' };
    mocks.findStaticCommandHelp.mockReturnValue(fakeCmd);

    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI(['install', '--help']);

    expect(handled).toBe(true);
    expect(mocks.findStaticCommandHelp).toHaveBeenCalledWith('install');
    expect(mocks.showCommandHelp).toHaveBeenCalledWith(fakeCmd);
    expect(mocks.findCommand).not.toHaveBeenCalled();
  });

  it('shows dynamic command help when static lookup misses but findCommand hits', async () => {
    const fakeCmd = { name: 'cache', description: 'Manage cache' };
    mocks.findStaticCommandHelp.mockReturnValue(undefined);
    mocks.findCommand.mockReturnValue(fakeCmd);

    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI(['cache', '--help']);

    expect(handled).toBe(true);
    expect(mocks.showCommandHelp).toHaveBeenCalledWith(fakeCmd);
    expect(mocks.showHelp).not.toHaveBeenCalled();
  });

  it('prints version for --version flag', async () => {
    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI(['--version']);

    expect(handled).toBe(true);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('octocode v')
    );
    expect(mocks.findCommand).not.toHaveBeenCalled();
  });

  it('prints version for -v flag', async () => {
    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI(['-v']);

    expect(handled).toBe(true);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('octocode v')
    );
  });

  it('returns false when no command is given (triggers interactive mode)', async () => {
    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI([]);

    expect(handled).toBe(false);
    expect(mocks.findCommand).not.toHaveBeenCalled();
    expect(mocks.showHelp).not.toHaveBeenCalled();
  });

  it('prints error for unknown command and sets exitCode 1', async () => {
    mocks.findCommand.mockReturnValue(undefined);

    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI(['nonexistent']);

    expect(handled).toBe(true);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Unknown command: nonexistent')
    );
    expect(process.exitCode).toBe(1);
  });

  it('sets exitCode 1 when --tool execution fails', async () => {
    mocks.executeToolCommand.mockResolvedValueOnce(false);

    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI([
      '--tool',
      'localSearchCode',
      '--queries',
      '{"bad":"input"}',
    ]);

    expect(handled).toBe(true);
    expect(process.exitCode).toBe(1);
  });

  it('sets exitCode 1 when github --tool execution fails', async () => {
    mocks.executeToolCommand.mockResolvedValueOnce(false);

    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI([
      '--tool',
      'githubSearchCode',
      '--queries',
      '{"bad":"input"}',
    ]);

    expect(handled).toBe(true);
    expect(process.exitCode).toBe(1);
  });

  it('does not print deprecation warning when using --tool', async () => {
    const consoleErrorSpy = vi
      .spyOn(console, 'error')
      .mockImplementation(() => {});

    const { runCLI } = await import('../../src/cli/index.js');

    await runCLI([
      '--tool',
      'githubSearchCode',
      '--queries',
      '{"owner":"x","repo":"y","keywordsToSearch":["a"]}',
    ]);

    expect(consoleErrorSpy).not.toHaveBeenCalled();
    consoleErrorSpy.mockRestore();
  });
});
