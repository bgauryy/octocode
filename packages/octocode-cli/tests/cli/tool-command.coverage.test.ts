import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  initialize: vi.fn().mockResolvedValue(undefined),
  initializeProviders: vi.fn().mockResolvedValue([]),
  loadToolContent: vi.fn().mockResolvedValue({
    instructions: 'Server instructions.',
    prompts: {},
    toolNames: {},
    baseSchema: {},
    tools: {
      githubSearchCode: {
        name: 'githubSearchCode',
        description: 'Search code.',
        schema: { keywordsToSearch: 'terms', owner: 'owner' },
        hints: { hasResults: [], empty: [] },
      },
      localSearchCode: {
        name: 'localSearchCode',
        description: 'Local search.',
        schema: { path: 'dir', pattern: 'regex' },
        hints: { hasResults: [], empty: [] },
      },
      githubCloneRepo: {
        name: 'githubCloneRepo',
        description: 'Clone a repo.',
        schema: { owner: 'owner', repo: 'repo' },
        hints: { hasResults: [], empty: [] },
      },

      legacyTool: {
        name: 'legacyTool',
        description: 'Legacy tool.',
        schema: { foo: 'Foo description', bar: 'Bar description' } as Record<
          string,
          string
        >,
        hints: { hasResults: [], empty: [] },
      },
    },
    baseHints: { hasResults: [], empty: [] },
    genericErrorHints: [],
  }),
  noop: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'ok' }],
  }),
  noopError: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'err' }],
    isError: true,
  }),
  localSearchCode: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'tool output' }],
  }),
  cloneRepo: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'cloned' }],
  }),
}));

vi.mock('octocode-mcp/public', async importOriginal => {
  const actual = await importOriginal<typeof import('octocode-mcp/public')>();
  const executeDirectTool = vi.fn(async (toolName: string, input: unknown) => {
    if (toolName.startsWith('github')) {
      await mocks.initialize();
      await mocks.initializeProviders();
    }

    if (toolName === 'localSearchCode') {
      return mocks.localSearchCode(input);
    }
    if (toolName === 'githubCloneRepo') {
      return mocks.cloneRepo(input);
    }
    return mocks.noop(input);
  });

  return {
    ...actual,
    initialize: mocks.initialize,
    initializeProviders: mocks.initializeProviders,
    loadToolContent: mocks.loadToolContent,
    executeDirectTool,
  };
});

describe('tool-command coverage', () => {
  let consoleSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    process.exitCode = undefined;
  });

  afterEach(() => {
    consoleSpy.mockRestore();
    process.exitCode = undefined;
  });

  it('showAvailableTools: lists tools grouped by category', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: [],
      options: {},
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('GitHub');
    expect(output).toContain('Local');
    expect(output).toContain('LSP');
    expect(output).toContain('localSearchCode');
    expect(output).toContain('githubSearchCode');
    expect(output).toContain('tools <name>');
    expect(process.exitCode).toBeUndefined();
  });

  it('showAvailableTools: --list flag triggers the tool list', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: [],
      options: { list: true },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('GitHub');
    expect(output).toContain('localSearchCode');
  });

  it('showAvailableTools: "list" as toolName triggers the tool list', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['list'],
      options: {},
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('GitHub');
  });

  it('printToolsContext: prints full context to stdout', async () => {
    const { printToolsContext } = await import('../../src/cli/tool-command.js');

    await printToolsContext();

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('CLI Usage:');
    expect(output).toContain('Server instructions.');
  });

  it('getToolsContextString: includes metadata-only tool via formatMetadataSchemaText', async () => {
    const { getToolsContextString } =
      await import('../../src/cli/tool-command.js');

    const context = await getToolsContextString();

    expect(context).toContain('legacyTool');
    expect(context).toContain('"foo": "Foo description"');
    expect(context).toContain('"bar": "Bar description"');
  });

  it('rejects an unknown tool name and sets exitCode 1', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['doesNotExist'],
      options: { tool: 'doesNotExist' },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Unknown tool: doesNotExist');
    expect(output).toContain('Available tools:');
    expect(process.exitCode).toBe(1);
  });

  it('showToolHelp: returns false for unknown tool', async () => {
    const { showToolHelp } = await import('../../src/cli/tool-command.js');
    const result = await showToolHelp('nonExistentTool');
    expect(result).toBe(false);
  });

  it('showToolHelp: GitHub tool shows mainResearchGoal in auto-filled hint', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['githubSearchCode'],
      options: { tool: 'githubSearchCode', schema: true },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('mainResearchGoal');
    expect(output).toContain('githubSearchCode');
    expect(output).toContain('keywordsToSearch');
  });

  it('showToolHelp: local tool does NOT show mainResearchGoal hint', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['localSearchCode'],
      options: { tool: 'localSearchCode', schema: true },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('localSearchCode');
    expect(output).toContain('Input Schema');

    expect(output).not.toContain('mainResearchGoal');
  });

  it('githubCloneRepo: executes with owner and repo fields', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['githubCloneRepo', '{"owner":"bgauryy","repo":"octocode-mcp"}'],
      options: { tool: 'githubCloneRepo' },
    });

    expect(mocks.initialize).toHaveBeenCalledTimes(1);
    expect(mocks.initializeProviders).toHaveBeenCalledTimes(1);
    expect(mocks.cloneRepo).toHaveBeenCalledWith(
      expect.objectContaining({
        queries: [
          expect.objectContaining({
            owner: 'bgauryy',
            repo: 'octocode-mcp',
          }),
        ],
      })
    );
    expect(consoleSpy).toHaveBeenCalledWith('cloned');
    expect(process.exitCode).toBeUndefined();
  });

  it('githubCloneRepo: branch is forwarded correctly', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: [
        'githubCloneRepo',
        '{"owner":"bgauryy","repo":"octocode-mcp","branch":"main"}',
      ],
      options: { tool: 'githubCloneRepo' },
    });

    expect(mocks.cloneRepo).toHaveBeenCalledWith(
      expect.objectContaining({
        queries: [
          expect.objectContaining({
            owner: 'bgauryy',
            repo: 'octocode-mcp',
            branch: 'main',
          }),
        ],
      })
    );
  });

  it('accepts an array of query objects directly', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: [
        'localSearchCode',
        '[{"path":".","pattern":"foo","matchContentLength":200,"filesPerPage":1,"filePageNumber":1,"matchesPerPage":1},{"path":"src","pattern":"bar","matchContentLength":200,"filesPerPage":1,"filePageNumber":1,"matchesPerPage":1}]',
      ],
      options: { tool: 'localSearchCode' },
    });

    expect(mocks.localSearchCode).toHaveBeenCalledWith(
      expect.objectContaining({
        queries: expect.arrayContaining([
          expect.objectContaining({ path: '.', pattern: 'foo' }),
          expect.objectContaining({ path: 'src', pattern: 'bar' }),
        ]),
      })
    );
  });

  it('passes responseCharOffset from { queries, responseCharOffset }', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: [
        'localSearchCode',
        '{"queries":[{"path":".","pattern":"foo","matchContentLength":200,"filesPerPage":1,"filePageNumber":1,"matchesPerPage":1}],"responseCharOffset":500}',
      ],
      options: { tool: 'localSearchCode' },
    });

    expect(mocks.localSearchCode).toHaveBeenCalledWith(
      expect.objectContaining({
        responseCharOffset: 500,
        queries: [expect.objectContaining({ path: '.', pattern: 'foo' })],
      })
    );
  });

  it('errors when more than two positional args are supplied', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: [
        'localSearchCode',
        '{"path":".","pattern":"x","matchContentLength":200,"filesPerPage":1,"filePageNumber":1,"matchesPerPage":1}',
        'extra',
      ],
      options: { tool: 'localSearchCode' },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Pass tool input as one quoted JSON string');
    expect(process.exitCode).toBe(1);
  });

  it('errors on non-string / non-object / non-array raw payload (e.g. number)', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['localSearchCode', '42'],
      options: { tool: 'localSearchCode' },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Tool input must be a JSON object');
    expect(process.exitCode).toBe(1);
  });

  it('errors when queries array is empty', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['localSearchCode', '{"queries":[]}'],
      options: { tool: 'localSearchCode' },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('At least one query is required');
    expect(process.exitCode).toBe(1);
  });

  it('normaliseKey: converts kebab-case query keys to camelCase', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: [
        'localSearchCode',
        '{"path":".","pattern":"x","fixed-string":true,"matchContentLength":200,"filesPerPage":1,"filePageNumber":1,"matchesPerPage":1}',
      ],
      options: { tool: 'localSearchCode' },
    });

    expect(mocks.localSearchCode).toHaveBeenCalledWith(
      expect.objectContaining({
        queries: [expect.objectContaining({ fixedString: true })],
      })
    );
  });

  it('printToolResult: falls back to structuredContent when content is empty', async () => {
    mocks.localSearchCode.mockResolvedValueOnce({
      content: [],
      structuredContent: { status: 'ok', count: 3 },
    });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: [
        'localSearchCode',
        '{"path":".","pattern":"x","matchContentLength":200,"filesPerPage":1,"filePageNumber":1,"matchesPerPage":1}',
      ],
      options: { tool: 'localSearchCode' },
    });

    const allArgs = consoleSpy.mock.calls.flat().join('\n');
    expect(allArgs).toContain('"status": "ok"');
  });

  it('printToolResult: falls back to JSON.stringify(result) when no content and no structuredContent', async () => {
    mocks.localSearchCode.mockResolvedValueOnce({
      content: [],
    });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: [
        'localSearchCode',
        '{"path":".","pattern":"x","matchContentLength":200,"filesPerPage":1,"filePageNumber":1,"matchesPerPage":1}',
      ],
      options: { tool: 'localSearchCode' },
    });

    const allArgs = consoleSpy.mock.calls.flat().join('\n');
    expect(allArgs).toContain('"content"');
  });

  it('printToolResult: --json mode prints the full MCP CallToolResult envelope', async () => {
    mocks.localSearchCode.mockResolvedValueOnce({
      content: [{ type: 'text', text: 'yaml output' }],
      structuredContent: { kind: 'results', items: ['a', 'b'] },
      isError: false,
    });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: [
        'localSearchCode',
        '{"path":".","pattern":"x","matchContentLength":200,"filesPerPage":1,"filePageNumber":1,"matchesPerPage":1}',
      ],
      options: { tool: 'localSearchCode', json: true },
    });

    const raw = consoleSpy.mock.calls.flat().join('\n');
    const parsed = JSON.parse(raw);
    expect(parsed.content).toEqual([{ type: 'text', text: 'yaml output' }]);
    expect(parsed.structuredContent).toEqual({
      kind: 'results',
      items: ['a', 'b'],
    });
    expect(parsed.isError).toBe(false);
  });

  it('printToolResult: --json mode preserves structuredContent results', async () => {
    mocks.localSearchCode.mockResolvedValueOnce({
      content: [{ type: 'text', text: 'yaml output' }],
      structuredContent: {
        base: '/repo/src',
        results: [{ id: 'q1', status: 'hasResults', data: {} }],
      },
    });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: [
        'localSearchCode',
        '{"path":".","pattern":"x","matchContentLength":200,"filesPerPage":1,"filePageNumber":1,"matchesPerPage":1}',
      ],
      options: { tool: 'localSearchCode', json: true },
    });

    const raw = consoleSpy.mock.calls.flat().join('\n');
    const parsed = JSON.parse(raw);
    expect(parsed.content).toEqual([{ type: 'text', text: 'yaml output' }]);
    expect(parsed.structuredContent.base).toBe('/repo/src');
    expect(parsed.structuredContent.results).toEqual([
      { id: 'q1', status: 'hasResults', data: {} },
    ]);
  });

  it('printToolResult: -o json flag also selects JSON mode', async () => {
    mocks.localSearchCode.mockResolvedValueOnce({
      content: [{ type: 'text', text: 'out' }],
      structuredContent: { answer: 42 },
    });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: [
        'localSearchCode',
        '{"path":".","pattern":"x","matchContentLength":200,"filesPerPage":1,"filePageNumber":1,"matchesPerPage":1}',
      ],
      options: { tool: 'localSearchCode', o: 'json' },
    });

    const raw = consoleSpy.mock.calls.flat().join('\n');
    const parsed = JSON.parse(raw);
    expect(parsed.content).toEqual([{ type: 'text', text: 'out' }]);
    expect(parsed.structuredContent).toEqual({ answer: 42 });
  });

  it('sets exitCode 1 when tool returns isError: true', async () => {
    mocks.localSearchCode.mockResolvedValueOnce({
      content: [{ type: 'text', text: 'failed' }],
      isError: true,
    });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: [
        'localSearchCode',
        '{"path":".","pattern":"x","matchContentLength":200,"filesPerPage":1,"filePageNumber":1,"matchesPerPage":1}',
      ],
      options: { tool: 'localSearchCode' },
    });

    expect(process.exitCode).toBe(1);
  });

  it('handles non-Error thrown value in tool execution', async () => {
    mocks.localSearchCode.mockRejectedValueOnce('string error');

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: [
        'localSearchCode',
        '{"path":".","pattern":"x","matchContentLength":200,"filesPerPage":1,"filePageNumber":1,"matchesPerPage":1}',
      ],
      options: { tool: 'localSearchCode' },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Tool execution failed.');
    expect(process.exitCode).toBe(1);
  });

  it('handles non-Error thrown by the execution function', async () => {
    mocks.localSearchCode.mockRejectedValueOnce(42);

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: [
        'localSearchCode',
        '{"path":".","pattern":"x","matchContentLength":200,"filesPerPage":1,"filePageNumber":1,"matchesPerPage":1}',
      ],
      options: { tool: 'localSearchCode' },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');

    expect(output).toContain('Tool execution failed.');
    expect(process.exitCode).toBe(1);
  });

  it('getDisplayFields: returns MCP display fields for canonical tools', async () => {
    const { getDisplayFields, TOOL_DEFINITIONS } =
      await import('../../src/cli/tool-command.js');

    const githubTool = TOOL_DEFINITIONS.find(
      tool => tool.name === 'githubSearchCode'
    );
    const packageTool = TOOL_DEFINITIONS.find(
      tool => tool.name === 'packageSearch'
    );

    expect(githubTool).toBeDefined();
    expect(packageTool).toBeDefined();

    const githubFields = getDisplayFields(githubTool!);
    const packageFields = getDisplayFields(packageTool!);
    const githubByName = Object.fromEntries(
      githubFields.map(field => [field.name, field])
    );
    const packageByName = Object.fromEntries(
      packageFields.map(field => [field.name, field])
    );

    expect(githubByName['keywordsToSearch']?.type).toBe('array<string>');
    expect(packageByName['name']?.type).toBe('string');
    expect(packageByName['itemsPerPage']?.type).toBe('integer');
    expect(githubByName['id']).toBeUndefined();
    expect(githubByName['researchGoal']).toBeUndefined();
    expect(githubByName['reasoning']).toBeUndefined();
  });

  it('packageSearch example includes the MCP-owned required fields', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['packageSearch'],
      options: { tool: 'packageSearch', schema: true },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('"name"');
    expect(output).toContain('react');
    // `itemsPerPage` is the single canonical result-count knob. The legacy
    // `limit` alias is no longer advertised (it was a duplicate field), though
    // it is still tolerated at runtime via the schema preprocess.
    expect(output).toContain('itemsPerPage');
    expect(output).not.toContain('"limit"');
  });

  it('githubSearchRepositories help includes MCP schema and required example fields', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['githubSearchRepositories'],
      options: { tool: 'githubSearchRepositories', schema: true },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('keywordsToSearch');
    expect(output).toContain('"page"');
    expect(output).toContain('itemsPerPage');
  });

  it('buildExampleValue: githubCloneRepo example includes owner=bgauryy', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['githubCloneRepo'],
      options: { tool: 'githubCloneRepo', schema: true },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('bgauryy');
    expect(output).toContain('octocode-mcp');
  });

  it('reports first failing query in a multi-query array', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',

      args: [
        'localSearchCode',
        '[{"path":".","pattern":"ok","matchContentLength":200,"filesPerPage":1,"filePageNumber":1,"matchesPerPage":1},{"path":".","pattern":999,"matchContentLength":200,"filesPerPage":1,"filePageNumber":1,"matchesPerPage":1}]',
      ],
      options: { tool: 'localSearchCode' },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Tool input does not match the expected schema.');
    expect(process.exitCode).toBe(1);
  });

  it('sortToolNames: tools in the same category maintain stable relative order', async () => {
    const { getToolsContextString } =
      await import('../../src/cli/tool-command.js');

    const context = await getToolsContextString();

    const ghIdx = context.indexOf('githubSearchCode');
    const cloneIdx = context.indexOf('githubCloneRepo');
    expect(ghIdx).toBeGreaterThan(-1);
    expect(cloneIdx).toBeGreaterThan(-1);

    expect(ghIdx).toBeLessThan(cloneIdx);
  });

  it('preserves user-supplied id, researchGoal, reasoning, mainResearchGoal', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: [
        'githubSearchCode',
        JSON.stringify({
          id: 'my-id',
          mainResearchGoal: 'my main goal',
          researchGoal: 'my goal',
          reasoning: 'my reasoning',
          keywordsToSearch: ['test'],
        }),
      ],
      options: { tool: 'githubSearchCode' },
    });

    expect(mocks.noop).toHaveBeenCalledWith(
      expect.objectContaining({
        queries: [
          expect.objectContaining({
            id: 'my-id',
            mainResearchGoal: 'my main goal',
            researchGoal: 'my goal',
            reasoning: 'my reasoning',
          }),
        ],
      })
    );
  });

  it('showAvailableTools: returns null metadata gracefully when loadToolContent fails', async () => {
    mocks.loadToolContent.mockRejectedValueOnce(
      new Error('metadata unavailable')
    );

    const { showAvailableTools } =
      await import('../../src/cli/tool-command.js');

    await expect(showAvailableTools()).resolves.toBeUndefined();

    const output = consoleSpy.mock.calls.flat().join('\n');

    expect(output).toContain('localSearchCode');
  });

  it('printToolResult: uses structuredContent when result.content is undefined', async () => {
    mocks.localSearchCode.mockResolvedValueOnce({
      structuredContent: { found: true },
    } as unknown as { content: []; structuredContent: { found: boolean } });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: [
        'localSearchCode',
        '{"path":".","pattern":"x","matchContentLength":200,"filesPerPage":1,"filePageNumber":1,"matchesPerPage":1}',
      ],
      options: { tool: 'localSearchCode' },
    });

    const out = consoleSpy.mock.calls.flat().join('\n');
    expect(out).toContain('"found": true');
  });

  it('printToolResult: content blocks with non-string text are filtered out', async () => {
    mocks.localSearchCode.mockResolvedValueOnce({
      content: [
        { type: 'image', data: 'base64...' },
        { type: 'text', text: '' },
        { type: 'text', text: 'real output' },
      ],
    } as unknown as { content: Array<{ type: string; text?: string }> });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: [
        'localSearchCode',
        '{"path":".","pattern":"x","matchContentLength":200,"filesPerPage":1,"filePageNumber":1,"matchesPerPage":1}',
      ],
      options: { tool: 'localSearchCode' },
    });

    const out = consoleSpy.mock.calls.flat().join('\n');
    expect(out).toContain('real output');
    expect(out).not.toContain('base64');
  });

  it('printToolResult: JSON mode preserves null structuredContent in the envelope', async () => {
    mocks.localSearchCode.mockResolvedValueOnce({
      content: [{ type: 'text', text: 'txt' }],
      structuredContent: null,
    } as unknown as { content: Array<{ type: string; text: string }> });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: [
        'localSearchCode',
        '{"path":".","pattern":"x","matchContentLength":200,"filesPerPage":1,"filePageNumber":1,"matchesPerPage":1}',
      ],
      options: { tool: 'localSearchCode', json: true },
    });

    const parsed = JSON.parse(consoleSpy.mock.calls.flat().join('\n'));
    expect(parsed.content).toEqual([{ type: 'text', text: 'txt' }]);
    expect(parsed.structuredContent).toBeNull();
  });

  it('printToolResult: JSON mode preserves primitive structuredContent in the envelope', async () => {
    mocks.localSearchCode.mockResolvedValueOnce({
      content: [{ type: 'text', text: 'txt' }],
      structuredContent: 'just a string',
    } as unknown as { content: Array<{ type: string; text: string }> });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: [
        'localSearchCode',
        '{"path":".","pattern":"x","matchContentLength":200,"filesPerPage":1,"filePageNumber":1,"matchesPerPage":1}',
      ],
      options: { tool: 'localSearchCode', json: true },
    });

    const parsed = JSON.parse(consoleSpy.mock.calls.flat().join('\n'));
    expect(parsed.content).toEqual([{ type: 'text', text: 'txt' }]);
    expect(parsed.structuredContent).toBe('just a string');
  });

  it('buildExampleValue: lspFindReferences example exercises boolean and unknown-name branches', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['lspFindReferences'],
      options: { tool: 'lspFindReferences', schema: true },
    });

    const out = consoleSpy.mock.calls.flat().join('\n');
    expect(out).toContain('lspFindReferences');
    expect(out).toContain('Input Schema');
  });

  it('buildExampleValue: lspCallHierarchy example exercises enum branch in schema', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['lspCallHierarchy'],
      options: { tool: 'lspCallHierarchy', schema: true },
    });

    const out = consoleSpy.mock.calls.flat().join('\n');
    expect(out).toContain('direction');

    expect(out).toContain('incoming');
  });

  it('resolves tool name from --tool option when no positional arg given', async () => {
    const { executeToolCommand } =
      await import('../../src/cli/tool-command.js');

    const ok = await executeToolCommand({
      command: 'tool',
      args: [],
      options: {
        tool: 'localSearchCode',
        queries:
          '{"path":".","pattern":"x","matchContentLength":200,"filesPerPage":1,"filePageNumber":1,"matchesPerPage":1}',
      },
    });

    expect(ok).toBe(true);
    expect(mocks.localSearchCode).toHaveBeenCalledTimes(1);
  });

  it('shows the tool list when neither positional arg nor --tool option present', async () => {
    const { executeToolCommand } =
      await import('../../src/cli/tool-command.js');

    const ok = await executeToolCommand({
      command: 'tool',
      args: [],
      options: {},
    });

    expect(ok).toBe(true);
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('localSearchCode');
  });
});
