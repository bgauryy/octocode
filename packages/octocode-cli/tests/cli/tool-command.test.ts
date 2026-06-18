import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const publicMocks = vi.hoisted(() => ({
  initialize: vi.fn().mockResolvedValue(undefined),
  initializeProviders: vi.fn().mockResolvedValue([]),
  loadToolContent: vi.fn().mockResolvedValue({
    instructions: 'Use Octocode tools carefully.',
    prompts: {},
    toolNames: {},
    baseSchema: {
      mainResearchGoal: 'main goal',
      researchGoal: 'goal',
      reasoning: 'reasoning',
      bulkQuery: (toolName: string) => `queries for ${toolName}`,
    },
    tools: {
      ghSearchCode: {
        name: 'ghSearchCode',
        description: 'Search code in GitHub repositories.',
        schema: {
          keywords: 'Search terms',
          owner: 'Repository owner',
        },
        hints: { hasResults: [], empty: [] },
      },
      localSearchCode: {
        name: 'localSearchCode',
        description: 'Search local code with ripgrep.',
        schema: {
          path: 'Path to search',
          keywords: 'Pattern to find',
        },
        hints: { hasResults: [], empty: [] },
      },
      ghCloneRepo: {
        name: 'ghCloneRepo',
        description: 'Clone a repository locally.',
        schema: {
          owner: 'Repository owner',
          repo: 'Repository name',
        },
        hints: { hasResults: [], empty: [] },
      },
    },
    baseHints: { hasResults: [], empty: [] },
    genericErrorHints: [],
  }),
  localSearchCode: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'tool output' }],
  }),
  ghSearchCode: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'github output' }],
  }),
  noop: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'ok' }],
  }),
}));

vi.mock('@octocodeai/octocode-tools-core/direct', async importOriginal => {
  const actual =
    await importOriginal<
      typeof import('@octocodeai/octocode-tools-core/direct')
    >();
  const executeDirectTool = vi.fn(async (toolName: string, input: unknown) => {
    if (toolName.startsWith('gh')) {
      await publicMocks.initialize();
      await publicMocks.initializeProviders();
    }

    if (toolName === 'localSearchCode') {
      return publicMocks.localSearchCode(input);
    }
    if (toolName === 'ghSearchCode') {
      return publicMocks.ghSearchCode(input);
    }
    return publicMocks.noop(input);
  });

  return {
    ...actual,
    loadToolContent: publicMocks.loadToolContent,
    executeDirectTool,
  };
});

describe('toolCommand', () => {
  let consoleSpy: ReturnType<typeof vi.spyOn>;
  let originalExitCode: typeof process.exitCode;

  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    originalExitCode = process.exitCode;
    process.exitCode = undefined;
  });

  afterEach(() => {
    consoleSpy.mockRestore();
    process.exitCode = originalExitCode;
  });

  it('executes a tool from a positional JSON payload', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: [
        'localSearchCode',
        '{"path":".","keywords":"runCLI","fixedString":true,"include":["ts","tsx"],"maxFiles":5,"matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}',
      ],
      options: {},
    });

    expect(publicMocks.initialize).not.toHaveBeenCalled();
    expect(publicMocks.initializeProviders).not.toHaveBeenCalled();
    expect(publicMocks.localSearchCode).toHaveBeenCalledWith(
      expect.objectContaining({
        queries: [
          expect.objectContaining({
            path: '.',
            keywords: 'runCLI',
            fixedString: true,
            include: ['ts', 'tsx'],
            maxFiles: 5,
            researchGoal: 'Execute localSearchCode via octocode-cli',
            reasoning: 'Executed via octocode-cli tool command',
          }),
        ],
      })
    );
    expect(consoleSpy).toHaveBeenCalledWith('tool output');
    expect(process.exitCode).toBeUndefined();
  });

  it('accepts JSON bulk payloads from the positional input string', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: [
        'ghSearchCode',
        '{"queries":[{"keywords":["tool"],"owner":"bgauryy","repo":"octocode-mcp"}],"responseCharLength":1200}',
      ],
      options: {},
    });

    expect(publicMocks.initialize).toHaveBeenCalledTimes(1);
    expect(publicMocks.initializeProviders).toHaveBeenCalledTimes(1);
    expect(publicMocks.ghSearchCode).toHaveBeenCalledWith(
      expect.objectContaining({
        queries: [
          expect.objectContaining({
            keywords: ['tool'],
            owner: 'bgauryy',
            repo: 'octocode-mcp',
            mainResearchGoal: 'Execute ghSearchCode via octocode-cli',
            researchGoal: 'Execute ghSearchCode via octocode-cli',
            reasoning: 'Executed via octocode-cli tool command',
          }),
        ],
      })
    );
    expect(consoleSpy).toHaveBeenCalledWith('github output');
  });

  it('supports JSON output mode for canonical tool execution', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: [
        'localSearchCode',
        '{"path":".","keywords":"runCLI","matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}',
      ],
      options: {
        json: true,
      },
    });

    expect(publicMocks.localSearchCode).toHaveBeenCalledTimes(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('"content"')
    );
  });

  it('shows schema help when a tool is selected without input', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearchCode'],
      options: {},
    });

    expect(publicMocks.localSearchCode).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('localSearchCode')
    );
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Input Schema')
    );
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('same Octocode MCP tool implementation')
    );
  });

  it('shows schema help when --scheme is provided', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearchCode'],
      options: { scheme: true },
    });

    expect(publicMocks.localSearchCode).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('Example'));
  });

  it('rejects legacy --input usage and points to the canonical contract', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearchCode'],
      options: {
        input:
          '{"path":".","keywords":"runCLI","matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}',
      },
    });

    expect(publicMocks.localSearchCode).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Use octocode tools')
    );
    expect(process.exitCode).toBe(2);
  });

  it('rejects legacy tool-specific flags and requires one JSON payload', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearchCode'],
      options: {
        path: '.',
        keywords: 'runCLI',
      },
    });

    expect(publicMocks.localSearchCode).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Unsupported tool flags')
    );
    expect(process.exitCode).toBe(2);
  });

  it('rejects invalid JSON payloads for canonical tool usage', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearchCode', '{"path":".","keywords":"runCLI"'],
      options: {},
    });

    expect(publicMocks.localSearchCode).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Tool input must be valid JSON')
    );
    expect(process.exitCode).toBe(2);
  });

  it('schema validation failure should show error', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: [
        'localSearchCode',
        '{"path":".","keywords":999,"matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}',
      ],
      options: {},
    });

    expect(publicMocks.localSearchCode).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Tool input does not match the expected schema.')
    );
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('keywords:')
    );
    expect(process.exitCode).toBe(2);
  });

  it('tool execution throwing should show error and return false', async () => {
    const err = new Error('Ripgrep launcher failed.');
    publicMocks.localSearchCode.mockRejectedValueOnce(err);

    const { executeToolCommand, toolCommand } =
      await import('../../src/cli/tool-command.js');

    const ok = await executeToolCommand({
      command: 'tools',
      args: [
        'localSearchCode',
        '{"path":".","keywords":"runCLI","matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}',
      ],
      options: {},
    });

    expect(ok).toBe(false);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Ripgrep launcher failed.')
    );

    process.exitCode = undefined;
    consoleSpy.mockClear();

    publicMocks.localSearchCode.mockRejectedValueOnce(err);

    await toolCommand.handler!({
      command: 'tools',
      args: [
        'localSearchCode',
        '{"path":".","keywords":"runCLI","matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}',
      ],
      options: {},
    });

    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Ripgrep launcher failed.')
    );
    expect(process.exitCode).toBe(5);

    vi.mocked(publicMocks.localSearchCode).mockResolvedValue({
      content: [{ type: 'text', text: 'tool output' }],
    });
  });

  it('shows multiple tool schemas when given multiple tool-name args', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearchCode', 'localFindFiles'],
      options: {},
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('localSearchCode');
    expect(output).toContain('localFindFiles');
  });

  it('shows error and tool help when --queries input cannot be parsed into a valid tool input', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearchCode'],
      options: { queries: 'null' },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Tool input must be a JSON object');
  });

  it('builds tools context from MCP instructions and tool schemas (--full)', async () => {
    const { getToolsContextString } =
      await import('../../src/cli/tool-command.js');

    const context = await getToolsContextString({ full: true });

    expect(publicMocks.loadToolContent).toHaveBeenCalledTimes(1);
    expect(context).toContain('TOOL CALLS');
    expect(context).toContain('octocode tools');
    expect(context).toContain('Use Octocode tools carefully.');
    expect(context).toContain('1. ghSearchCode');
    expect(context).toContain('2. ghCloneRepo');
    expect(context).toContain('3. localSearchCode');
    // full mode includes complete tool descriptions
    expect(context).toContain('Search code in GitHub repositories.');
    expect(context).toContain('Clone a repository locally.');
  });

  it('builds a lean default tools context (compact field lists)', async () => {
    const { getToolsContextString } =
      await import('../../src/cli/tool-command.js');

    const context = await getToolsContextString();

    // lean mode includes short tool descriptions inline
    expect(context).toContain(
      '1. ghSearchCode — Search code in GitHub repositories.'
    );
    expect(context).not.toContain('"$schema"');
    expect(context).toContain('RESEARCH LOOP');
  });
});
