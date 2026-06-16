import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  loadCommand: vi.fn(),
  showHelp: vi.fn(),
  showCommandHelp: vi.fn(),
  showToolHelp: vi.fn(),
  showAvailableTools: vi.fn().mockResolvedValue(undefined),
  showMultipleToolSchemas: vi.fn().mockResolvedValue(undefined),
  findStaticCommandHelp: vi.fn(),
  executeToolCommand: vi.fn().mockResolvedValue(true),
  printToolsContext: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../src/cli/commands/index.js', () => ({
  loadCommand: mocks.loadCommand,
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
  showAvailableTools: mocks.showAvailableTools,
  showMultipleToolSchemas: mocks.showMultipleToolSchemas,
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

  it('routes context to the tools context', async () => {
    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI(['context']);

    expect(handled).toBe(true);
    expect(mocks.printToolsContext).toHaveBeenCalledWith({ full: false });
    expect(mocks.loadCommand).not.toHaveBeenCalled();
  });

  it('passes --full to context', async () => {
    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI(['context', '--full']);

    expect(handled).toBe(true);
    expect(mocks.printToolsContext).toHaveBeenCalledWith({ full: true });
  });

  it('routes --context as a top-level agent-context shortcut', async () => {
    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI(['--no-color', '--context', '--full']);

    expect(handled).toBe(true);
    expect(process.env.NO_COLOR).toBe('1');
    expect(mocks.printToolsContext).toHaveBeenCalledWith({ full: true });
    expect(mocks.loadCommand).not.toHaveBeenCalled();
  });

  it('routes tools usage through the unified tool executor', async () => {
    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI([
      'tools',
      'localSearchCode',
      '--queries',
      '{"path":".","keywords":"runCLI"}',
    ]);

    expect(handled).toBe(true);
    expect(mocks.executeToolCommand).toHaveBeenCalledTimes(1);
    expect(mocks.executeToolCommand).toHaveBeenCalledWith({
      command: 'tools',
      args: ['localSearchCode'],
      options: {
        queries: '{"path":".","keywords":"runCLI"}',
      },
    });
    expect(mocks.loadCommand).not.toHaveBeenCalled();
  });

  it('routes GitHub tools through the unified tool executor', async () => {
    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI([
      'tools',
      'ghSearchCode',
      '--queries',
      '{"owner":"bgauryy","repo":"octocode-mcp","keywordsToSearch":["tool"]}',
      '--output',
      'json',
    ]);

    expect(handled).toBe(true);
    expect(mocks.executeToolCommand).toHaveBeenCalledTimes(1);
    expect(mocks.executeToolCommand).toHaveBeenCalledWith({
      command: 'tools',
      args: ['ghSearchCode'],
      options: {
        queries:
          '{"owner":"bgauryy","repo":"octocode-mcp","keywordsToSearch":["tool"]}',
        output: 'json',
      },
    });
  });

  it('shows dynamic tool help for tools <name> --help', async () => {
    mocks.showToolHelp.mockResolvedValue(true);

    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI(['tools', 'localSearchCode', '--help']);

    expect(handled).toBe(true);
    expect(mocks.showToolHelp).toHaveBeenCalledTimes(1);
    expect(mocks.showToolHelp).toHaveBeenCalledWith('localSearchCode');
    expect(mocks.executeToolCommand).not.toHaveBeenCalled();
    expect(mocks.loadCommand).not.toHaveBeenCalled();
  });

  it('rejects the removed singular tool command', async () => {
    mocks.loadCommand.mockResolvedValue(undefined);

    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI([
      'tool',
      'localSearchCode',
      '{"path":".","keywords":"runCLI"}',
    ]);

    expect(handled).toBe(true);
    expect(mocks.executeToolCommand).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Unknown command: tool')
    );
    expect(process.exitCode).toBe(3);
  });

  it('shows main help when --help is passed without a command', async () => {
    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI(['--help']);

    expect(handled).toBe(true);
    expect(mocks.showHelp).toHaveBeenCalledTimes(1);
    expect(mocks.showCommandHelp).not.toHaveBeenCalled();
  });

  it('shows main help for unknown command --help', async () => {
    mocks.findStaticCommandHelp.mockReturnValue(undefined);

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
    expect(mocks.loadCommand).not.toHaveBeenCalled();
  });

  it('prints version for --version flag', async () => {
    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI(['--version']);

    expect(handled).toBe(true);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('octocode v')
    );
    expect(mocks.loadCommand).not.toHaveBeenCalled();
  });

  it('treats single-dash version spelling as an unknown command', async () => {
    mocks.loadCommand.mockResolvedValue(undefined);
    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI(['-v']);

    expect(handled).toBe(true);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Unknown command: -v')
    );
    expect(process.exitCode).toBe(3);
  });

  it('returns false when no command is given', async () => {
    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI([]);

    expect(handled).toBe(false);
    expect(mocks.loadCommand).not.toHaveBeenCalled();
    expect(mocks.showHelp).not.toHaveBeenCalled();
  });

  it('prints error for unknown top-level options', async () => {
    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI(['--not-real']);

    expect(handled).toBe(true);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Unknown option: --not-real')
    );
    expect(process.exitCode).toBe(3);
    expect(mocks.loadCommand).not.toHaveBeenCalled();
  });

  it('reports the actual unknown option after known global flags', async () => {
    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI(['--no-color', '--contecxt']);

    expect(handled).toBe(true);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining(
        'Unknown option: --contecxt (did you mean --context?)'
      )
    );
    expect(process.exitCode).toBe(3);
    expect(mocks.loadCommand).not.toHaveBeenCalled();
  });

  it('prints error for unknown command and sets exitCode 3', async () => {
    mocks.loadCommand.mockResolvedValue(undefined);

    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI(['nonexistent']);

    expect(handled).toBe(true);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Unknown command: nonexistent')
    );
    expect(process.exitCode).toBe(3);
  });

  it('sets exitCode 1 when tool execution fails without a specific code', async () => {
    mocks.executeToolCommand.mockResolvedValueOnce(false);

    const { runCLI } = await import('../../src/cli/index.js');

    const handled = await runCLI([
      'tools',
      'localSearchCode',
      '--queries',
      '{"bad":"input"}',
    ]);

    expect(handled).toBe(true);
    expect(process.exitCode).toBe(1);
  });

  it('does not print warnings for canonical tool usage', async () => {
    const consoleErrorSpy = vi
      .spyOn(console, 'error')
      .mockImplementation(() => {});

    const { runCLI } = await import('../../src/cli/index.js');

    await runCLI([
      'tools',
      'ghSearchCode',
      '--queries',
      '{"owner":"x","repo":"y","keywordsToSearch":["a"]}',
    ]);

    expect(consoleErrorSpy).not.toHaveBeenCalled();
    consoleErrorSpy.mockRestore();
  });
});
