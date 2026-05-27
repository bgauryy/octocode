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
      githubSearchCode: {
        name: 'githubSearchCode',
        description: 'Search code in GitHub repositories.',
        schema: {
          keywordsToSearch: 'Search terms',
          owner: 'Repository owner',
        },
        hints: { hasResults: [], empty: [] },
      },
      localSearchCode: {
        name: 'localSearchCode',
        description: 'Search local code with ripgrep.',
        schema: {
          path: 'Path to search',
          pattern: 'Pattern to find',
        },
        hints: { hasResults: [], empty: [] },
      },
      githubCloneRepo: {
        name: 'githubCloneRepo',
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
  githubSearchCode: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'github output' }],
  }),
  noop: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'ok' }],
  }),
}));

vi.mock('octocode-mcp/public', async () => {
  const { z } = await import('zod/v4');

  const localBase = z.object({
    id: z.string(),
    researchGoal: z.string(),
    reasoning: z.string(),
  });

  const githubBase = z.object({
    id: z.string(),
    mainResearchGoal: z.string(),
    researchGoal: z.string(),
    reasoning: z.string(),
  });

  return {
    initialize: publicMocks.initialize,
    initializeProviders: publicMocks.initializeProviders,
    loadToolContent: publicMocks.loadToolContent,
    executeRipgrepSearch: publicMocks.localSearchCode,
    executeFetchContent: publicMocks.noop,
    executeFindFiles: publicMocks.noop,
    executeViewStructure: publicMocks.noop,
    executeGotoDefinition: publicMocks.noop,
    executeFindReferences: publicMocks.noop,
    executeCallHierarchy: publicMocks.noop,
    fetchMultipleGitHubFileContents: publicMocks.noop,
    searchMultipleGitHubCode: publicMocks.githubSearchCode,
    searchMultipleGitHubPullRequests: publicMocks.noop,
    searchMultipleGitHubRepos: publicMocks.noop,
    exploreMultipleRepositoryStructures: publicMocks.noop,
    executeCloneRepo: publicMocks.noop,
    searchPackages: publicMocks.noop,
    DEFAULT_TOOL_RESPONSE_FORMAT: 'tsv',
    formatCallToolResultForOutput: (
      result: unknown,
      outputMode: 'text' | 'json'
    ) => {
      if (outputMode === 'json') return JSON.stringify(result);
      const typedResult = result as {
        content?: Array<{ text?: string }>;
        structuredContent?: unknown;
      };
      const textBlocks = Array.isArray(typedResult.content)
        ? typedResult.content
            .map(block => (typeof block.text === 'string' ? block.text : ''))
            .filter(Boolean)
        : [];
      if (textBlocks.length > 0) return textBlocks.join('\n\n');
      if (typedResult.structuredContent !== undefined) {
        return JSON.stringify(typedResult.structuredContent, null, 2);
      }
      return JSON.stringify(result, null, 2);
    },
    RipgrepQuerySchema: localBase.extend({
      path: z.string(),
      pattern: z.string(),
      fixedString: z.boolean().optional(),
      include: z.array(z.string()).optional(),
      limit: z.number().optional(),
    }),
    FetchContentQuerySchema: localBase.extend({
      path: z.string(),
    }),
    FindFilesQuerySchema: localBase.extend({
      path: z.string(),
    }),
    ViewStructureQuerySchema: localBase.extend({
      path: z.string(),
    }),
    LSPGotoDefinitionQuerySchema: localBase.extend({
      path: z.string(),
    }),
    LSPFindReferencesQuerySchema: localBase.extend({
      path: z.string(),
    }),
    LSPCallHierarchyQuerySchema: localBase.extend({
      path: z.string(),
    }),
    FileContentQuerySchema: githubBase.extend({
      owner: z.string(),
      repo: z.string(),
      path: z.string(),
    }),
    GitHubCodeSearchQuerySchema: githubBase.extend({
      keywordsToSearch: z.array(z.string()),
      owner: z.string().optional(),
      repo: z.string().optional(),
      limit: z.number().optional(),
    }),
    GitHubPullRequestSearchQuerySchema: githubBase.extend({
      owner: z.string(),
      repo: z.string(),
    }),
    GitHubReposSearchSingleQuerySchema: githubBase.extend({
      keywordsToSearch: z.array(z.string()),
    }),
    GitHubViewRepoStructureQuerySchema: githubBase.extend({
      owner: z.string(),
      repo: z.string(),
    }),
    PackageSearchQuerySchema: githubBase.extend({
      ecosystem: z.enum(['npm', 'python']),
      name: z.string(),
    }),
    CloneRepoQuerySchema: githubBase.extend({
      owner: z.string().describe('Repository owner'),
      repo: z.string().describe('Repository name'),
      branch: z.string().optional(),
      sparse_path: z.string().optional(),
      forceRefresh: z.boolean().optional().default(false),
    }),
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
      command: 'tool',
      args: [
        'localSearchCode',
        '{"path":".","pattern":"runCLI","fixedString":true,"include":["ts","tsx"],"limit":5}',
      ],
      options: {
        tool: 'localSearchCode',
      },
    });

    expect(publicMocks.initialize).not.toHaveBeenCalled();
    expect(publicMocks.initializeProviders).not.toHaveBeenCalled();
    expect(publicMocks.localSearchCode).toHaveBeenCalledWith(
      expect.objectContaining({
        queries: [
          expect.objectContaining({
            path: '.',
            pattern: 'runCLI',
            fixedString: true,
            include: ['ts', 'tsx'],
            limit: 5,
            researchGoal: 'Execute localSearchCode via octocode-cli',
            reasoning: 'Executed via octocode-cli tool command',
          }),
        ],
        format: 'tsv',
      })
    );
    expect(consoleSpy).toHaveBeenCalledWith('tool output');
    expect(process.exitCode).toBeUndefined();
  });

  it('accepts JSON bulk payloads from the positional input string', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: [
        'githubSearchCode',
        '{"queries":[{"keywordsToSearch":["tool"],"owner":"bgauryy","repo":"octocode-mcp"}],"responseCharLength":1200}',
      ],
      options: { tool: 'githubSearchCode' },
    });

    expect(publicMocks.initialize).toHaveBeenCalledTimes(1);
    expect(publicMocks.initializeProviders).toHaveBeenCalledTimes(1);
    expect(publicMocks.githubSearchCode).toHaveBeenCalledWith(
      expect.objectContaining({
        queries: [
          expect.objectContaining({
            keywordsToSearch: ['tool'],
            owner: 'bgauryy',
            repo: 'octocode-mcp',
            mainResearchGoal: 'Execute githubSearchCode via octocode-cli',
            researchGoal: 'Execute githubSearchCode via octocode-cli',
            reasoning: 'Executed via octocode-cli tool command',
          }),
        ],
        responseCharLength: 1200,
        format: 'tsv',
      })
    );
    expect(consoleSpy).toHaveBeenCalledWith('github output');
  });

  it('supports JSON output mode for canonical tool execution', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['localSearchCode', '{"path":".","pattern":"runCLI"}'],
      options: {
        tool: 'localSearchCode',
        output: 'json',
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
      command: 'tool',
      args: ['localSearchCode'],
      options: { tool: 'localSearchCode' },
    });

    expect(publicMocks.localSearchCode).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('localSearchCode')
    );
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Input Schema')
    );
  });

  it('shows schema help when --schema is provided', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['localSearchCode'],
      options: { tool: 'localSearchCode', schema: true },
    });

    expect(publicMocks.localSearchCode).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('Example'));
  });

  it('rejects legacy --input usage and points to the canonical contract', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['localSearchCode'],
      options: {
        tool: 'localSearchCode',
        input: '{"path":".","pattern":"runCLI"}',
      },
    });

    expect(publicMocks.localSearchCode).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Use octocode-cli --tool')
    );
    expect(process.exitCode).toBe(1);
  });

  it('rejects legacy tool-specific flags and requires one JSON payload', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['localSearchCode'],
      options: {
        tool: 'localSearchCode',
        path: '.',
        pattern: 'runCLI',
      },
    });

    expect(publicMocks.localSearchCode).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Unsupported tool flags')
    );
    expect(process.exitCode).toBe(1);
  });

  it('rejects invalid JSON payloads for canonical tool usage', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['localSearchCode', '{"path":".","pattern":"runCLI"'],
      options: {
        tool: 'localSearchCode',
      },
    });

    expect(publicMocks.localSearchCode).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Tool input must be valid JSON')
    );
    expect(process.exitCode).toBe(1);
  });

  it('schema validation failure should show error', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['localSearchCode', '{"path":".","pattern":999}'],
      options: {
        tool: 'localSearchCode',
      },
    });

    expect(publicMocks.localSearchCode).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Tool input does not match the expected schema.')
    );
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('pattern:')
    );
    expect(process.exitCode).toBe(1);
  });

  it('tool execution throwing should show error and return false', async () => {
    const err = new Error('Ripgrep launcher failed.');
    publicMocks.localSearchCode.mockRejectedValueOnce(err);

    const { executeToolCommand, toolCommand } =
      await import('../../src/cli/tool-command.js');

    const ok = await executeToolCommand({
      command: 'tool',
      args: ['localSearchCode', '{"path":".","pattern":"runCLI"}'],
      options: {
        tool: 'localSearchCode',
      },
    });

    expect(ok).toBe(false);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Ripgrep launcher failed.')
    );

    process.exitCode = undefined;
    consoleSpy.mockClear();

    publicMocks.localSearchCode.mockRejectedValueOnce(err);

    await toolCommand.handler!({
      command: 'tool',
      args: ['localSearchCode', '{"path":".","pattern":"runCLI"}'],
      options: {
        tool: 'localSearchCode',
      },
    });

    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Ripgrep launcher failed.')
    );
    expect(process.exitCode).toBe(1);

    vi.mocked(publicMocks.localSearchCode).mockResolvedValue({
      content: [{ type: 'text', text: 'tool output' }],
    });
  });

  it('builds tools context from MCP instructions and tool schemas', async () => {
    const { getToolsContextString } =
      await import('../../src/cli/tool-command.js');

    const context = await getToolsContextString();

    expect(publicMocks.loadToolContent).toHaveBeenCalledTimes(1);
    expect(context).toContain('CLI Contract:');
    expect(context).toContain(
      "octocode-cli --tool <toolName> --queries '<json-stringified-input>'"
    );
    expect(context).toContain('Use Octocode tools carefully.');
    expect(context).toContain('1. githubSearchCode');
    expect(context).toContain('2. githubCloneRepo');
    expect(context).toContain('3. localSearchCode');
    expect(context).toContain('Input schema:');
    expect(context).toContain('"keywordsToSearch"');
    expect(context).toContain('"description": "Repository owner"');
  });
});
