import {
  afterAll,
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from 'vitest';

const oqlEnv = vi.hoisted(() => {
  const previous = process.env.ENABLE_OQL;
  process.env.ENABLE_OQL = '1';
  return { previous };
});

const mocks = vi.hoisted(() => ({
  initialize: vi.fn().mockResolvedValue(undefined),
  initializeProviders: vi.fn().mockResolvedValue([]),
  loadToolContent: vi.fn().mockResolvedValue({
    systemPrompt: 'Server instructions.',
    prompts: {},
    toolNames: {},
    baseSchema: {},
    tools: {
      ghSearchCode: {
        name: 'ghSearchCode',
        description: 'Search code.',
        schema: { keywords: 'terms', owner: 'owner' },
        hints: { hasResults: [], empty: [] },
      },
      localSearchCode: {
        name: 'localSearchCode',
        description: 'Local search.',
        schema: { path: 'dir', keywords: 'regex' },
        hints: { hasResults: [], empty: [] },
      },
      ghCloneRepo: {
        name: 'ghCloneRepo',
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

// Schema/help path imports the engine-free `/schema` subpath (P3).
vi.mock('@octocodeai/octocode-tools-core/schema', async importOriginal => {
  const actual =
    await importOriginal<
      typeof import('@octocodeai/octocode-tools-core/schema')
    >();
  return {
    ...actual,
    loadToolContent: mocks.loadToolContent,
  };
});

vi.mock('@octocodeai/octocode-tools-core/direct', async importOriginal => {
  const actual =
    await importOriginal<
      typeof import('@octocodeai/octocode-tools-core/direct')
    >();
  const executeDirectTool = vi.fn(async (toolName: string, input: unknown) => {
    if (toolName.startsWith('gh')) {
      await mocks.initialize();
      await mocks.initializeProviders();
    }

    if (toolName === 'localSearchCode') {
      return mocks.localSearchCode(input);
    }
    if (toolName === 'ghCloneRepo') {
      return mocks.cloneRepo(input);
    }
    return mocks.noop(input);
  });

  return {
    ...actual,
    loadToolContent: mocks.loadToolContent,
    executeDirectTool,
  };
});

describe('tool-command coverage', () => {
  afterAll(() => {
    if (oqlEnv.previous === undefined) delete process.env.ENABLE_OQL;
    else process.env.ENABLE_OQL = oqlEnv.previous;
  });

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
      command: 'tools',
      args: [],
      options: {},
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('GitHub');
    expect(output).toContain('Local Code');
    expect(output).not.toContain('\n  LSP\n');
    expect(output).toContain('localSearchCode');
    expect(output).toContain('ghSearchCode');
    expect(output).toContain('Search code.');
    expect(output).toContain('Local search.');
    expect(output).not.toContain('[path*');
    expect(output).not.toContain('workspaceSymbol');
    expect(output).not.toContain('diagnostic');
    expect(output).toContain('tools <name>');
    expect(process.exitCode).toBeUndefined();
  });

  it('showAvailableTools: --list flag triggers the tool list', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
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
      command: 'tools',
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
    expect(output).toContain('Octocode CLI — Agent Context');
    expect(output).toContain('tools <name>');
    // Smart commands section removed; verify RESEARCH LOOP and TOOL CALLS are present
    expect(output).toContain('RESEARCH LOOP');
    expect(output).toContain('TOOL CALLS');
    expect(output).toContain('Server instructions.');
    expect(output).toContain('Exit codes:');
    expect(output).toContain('structuredContent.results[]');
    expect(output).toContain('Output contract');
  });

  it('A2: default context uses compact field lists, not full JSON schemas', async () => {
    const { getToolsContextString } =
      await import('../../src/cli/tool-command.js');

    const compact = await getToolsContextString();
    const full = await getToolsContextString({ full: true });

    // Schemas are no longer embedded in context — read them on demand via octocode tools <name>
    expect(compact).not.toContain('"$schema"');
    expect(compact).toContain('RESEARCH LOOP');
    expect(full).toContain('RESEARCH LOOP');
    // full mode includes the complete description text on a separate line
    expect(full).toContain('Search code.');
    expect(full).toContain('Clone a repo.');
  });

  it('A1: --compact emits minified structuredContent only', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');
    mocks.localSearchCode.mockResolvedValueOnce({
      content: [{ type: 'text', text: 'results:\n  - id: x' }],
      structuredContent: {
        results: [{ id: 'x' }],
        evidence: { answerReady: true },
      },
      isError: false,
    });

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearchCode'],
      options: {
        queries: '{"path":".","keywords":"x"}',
        compact: true,
      },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    const parsed = JSON.parse(output.trim());
    expect(parsed).toEqual({
      results: [{ id: 'x' }],
      evidence: { answerReady: true },
    });
    expect(output).not.toContain('"content"');
    expect(output).not.toContain('"isError"');
  });

  it('A4: --format=tool emits a register-ready tool definition', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearchCode'],
      options: { format: 'tool' },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    const def = JSON.parse(output.trim());
    expect(def.name).toBe('localSearchCode');
    expect(typeof def.description).toBe('string');
    expect(def.inputSchema.type).toBe('object');
    expect(process.exitCode).toBeUndefined();
  });

  it('A3: unknown tool sets exit code NOT_FOUND (3)', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['doesNotExist'],
      options: {},
    });

    expect(process.exitCode).toBe(3);
  });

  it('getToolsContextString: includes metadata-only tool via formatMetadataSchemaText', async () => {
    const { getToolsContextString } =
      await import('../../src/cli/tool-command.js');

    const context = await getToolsContextString();

    expect(context).toContain('legacyTool');
    // Schema is no longer embedded in context — tool description is shown instead
    expect(context).toContain('Legacy tool.');
  });

  it('rejects an unknown tool name and sets exitCode NOT_FOUND (3)', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['doesNotExist'],
      options: {},
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Unknown tool: doesNotExist');
    expect(output).toContain('Available tools:');
    expect(process.exitCode).toBe(3);
  });

  it('showToolHelp: returns false for unknown tool', async () => {
    const { showToolHelp } = await import('../../src/cli/tool-command.js');
    const result = await showToolHelp('nonExistentTool');
    expect(result).toBe(false);
  });

  it('showToolHelp: GitHub tool shows mainResearchGoal in auto-filled hint', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['ghSearchCode'],
      options: { scheme: true },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('mainResearchGoal');
    expect(output).toContain('ghSearchCode');
    expect(output).toContain('keywords');
  });

  it('showToolHelp: local tool does NOT show mainResearchGoal hint', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearchCode'],
      options: { scheme: true },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('localSearchCode');
    expect(output).toContain('Input Schema');

    expect(output).not.toContain('mainResearchGoal');
  });

  it('ghCloneRepo: executes with owner and repo fields', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['ghCloneRepo', '{"owner":"bgauryy","repo":"octocode-mcp"}'],
      options: {},
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

  it('ghCloneRepo: branch is forwarded correctly', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: [
        'ghCloneRepo',
        '{"owner":"bgauryy","repo":"octocode-mcp","branch":"main"}',
      ],
      options: {},
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
      command: 'tools',
      args: [
        'localSearchCode',
        '[{"path":".","keywords":"foo","matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1},{"path":"src","keywords":"bar","matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}]',
      ],
      options: {},
    });

    expect(mocks.localSearchCode).toHaveBeenCalledWith(
      expect.objectContaining({
        queries: expect.arrayContaining([
          expect.objectContaining({ path: '.', keywords: 'foo' }),
          expect.objectContaining({ path: 'src', keywords: 'bar' }),
        ]),
      })
    );
  });

  it('forwards envelope-level fields like responseCharOffset to the tool', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: [
        'localSearchCode',
        '{"queries":[{"path":".","keywords":"foo","matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}],"responseCharOffset":500}',
      ],
      options: {},
    });

    const callArg = mocks.localSearchCode.mock.calls[0]?.[0];
    expect(callArg).toEqual(
      expect.objectContaining({
        queries: [expect.objectContaining({ path: '.', keywords: 'foo' })],
        responseCharOffset: 500,
      })
    );
  });

  it('errors when more than two positional args are supplied', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: [
        'localSearchCode',
        '{"path":".","keywords":"x","matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}',
        'extra',
      ],
      options: {},
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Pass tool input as one quoted JSON string');
    expect(process.exitCode).toBe(2);
  });

  it('errors on non-string / non-object / non-array raw payload (e.g. number)', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearchCode', '42'],
      options: {},
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Tool input must be a JSON object');
    expect(process.exitCode).toBe(2);
  });

  it('errors when queries array is empty', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearchCode', '{"queries":[]}'],
      options: {},
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('At least one query is required');
    expect(process.exitCode).toBe(2);
  });

  it('uses canonical query keys for localSearchCode pagination', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');
    const consoleErrorSpy = vi
      .spyOn(console, 'error')
      .mockImplementation(() => {});

    try {
      await toolCommand.handler!({
        command: 'tools',
        args: [
          'localSearchCode',
          '{"path":".","keywords":"x","fixedString":true,"matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}',
        ],
        options: {},
      });

      expect(mocks.localSearchCode).toHaveBeenCalledWith(
        expect.objectContaining({
          queries: [
            expect.objectContaining({
              fixedString: true,
              itemsPerPage: 1,
              page: 1,
              maxMatchesPerFile: 1,
            }),
          ],
        })
      );
      expect(consoleErrorSpy).not.toHaveBeenCalled();
    } finally {
      consoleErrorSpy.mockRestore();
    }
  });

  it('printToolResult: falls back to structuredContent when content is empty', async () => {
    mocks.localSearchCode.mockResolvedValueOnce({
      content: [],
      structuredContent: { status: 'ok', count: 3 },
    });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: [
        'localSearchCode',
        '{"path":".","keywords":"x","matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}',
      ],
      options: {},
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
      command: 'tools',
      args: [
        'localSearchCode',
        '{"path":".","keywords":"x","matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}',
      ],
      options: {},
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
      command: 'tools',
      args: [
        'localSearchCode',
        '{"path":".","keywords":"x","matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}',
      ],
      options: { json: true },
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
      command: 'tools',
      args: [
        'localSearchCode',
        '{"path":".","keywords":"x","matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}',
      ],
      options: { json: true },
    });

    const raw = consoleSpy.mock.calls.flat().join('\n');
    const parsed = JSON.parse(raw);
    expect(parsed.content).toEqual([{ type: 'text', text: 'yaml output' }]);
    expect(parsed.structuredContent.base).toBe('/repo/src');
    expect(parsed.structuredContent.results).toEqual([
      { id: 'q1', status: 'hasResults', data: {} },
    ]);
  });

  it('printToolResult: --json selects JSON mode for structured output', async () => {
    mocks.localSearchCode.mockResolvedValueOnce({
      content: [{ type: 'text', text: 'out' }],
      structuredContent: { answer: 42 },
    });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: [
        'localSearchCode',
        '{"path":".","keywords":"x","matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}',
      ],
      options: { json: true },
    });

    const raw = consoleSpy.mock.calls.flat().join('\n');
    const parsed = JSON.parse(raw);
    expect(parsed.content).toEqual([{ type: 'text', text: 'out' }]);
    expect(parsed.structuredContent).toEqual({ answer: 42 });
  });

  it('sets exitCode TOOL (5) when tool returns isError: true', async () => {
    mocks.localSearchCode.mockResolvedValueOnce({
      content: [{ type: 'text', text: 'failed' }],
      isError: true,
    });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: [
        'localSearchCode',
        '{"path":".","keywords":"x","matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}',
      ],
      options: {},
    });

    expect(process.exitCode).toBe(5);
  });

  it('handles non-Error thrown value in tool execution', async () => {
    mocks.localSearchCode.mockRejectedValueOnce('string error');

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: [
        'localSearchCode',
        '{"path":".","keywords":"x","matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}',
      ],
      options: {},
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Tool execution failed.');
    expect(process.exitCode).toBe(5);
  });

  it('handles non-Error thrown by the execution function', async () => {
    mocks.localSearchCode.mockRejectedValueOnce(42);

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: [
        'localSearchCode',
        '{"path":".","keywords":"x","matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}',
      ],
      options: {},
    });

    const output = consoleSpy.mock.calls.flat().join('\n');

    expect(output).toContain('Tool execution failed.');
    expect(process.exitCode).toBe(5);
  });

  it('getDisplayFields: returns MCP display fields for canonical tools', async () => {
    const { getDisplayFields, TOOL_DEFINITIONS } =
      await import('../../src/cli/tool-command.js');

    const githubTool = TOOL_DEFINITIONS.find(
      tool => tool.name === 'ghSearchCode'
    );
    const packageTool = TOOL_DEFINITIONS.find(
      tool => tool.name === 'npmSearch'
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

    expect(githubByName['keywords']?.type).toBe('array<string>');
    expect(packageByName['packageName']?.type).toBe('string');
    expect(packageByName['page']?.type).toBe('integer');
    expect(githubByName['id']).toBeUndefined();
    expect(githubByName['researchGoal']).toBeUndefined();
    expect(githubByName['reasoning']).toBeUndefined();
  });

  it('npmSearch example includes the MCP-owned required fields', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['npmSearch'],
      options: { scheme: true },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('"packageName"');
    expect(output).toContain('zod');
    expect(output).not.toContain('"page":');
    expect(output).not.toContain('"limit"');
  });

  it('ghSearchRepos help includes MCP schema and required example fields', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['ghSearchRepos'],
      options: { scheme: true },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('keywords');
    // page field format includes constraints: 'page (integer, 1-1000, default 1)'
    expect(output).toContain('page');
    expect(output).toContain('integer');
    expect(output).toContain('sort');
  });

  it('buildExampleValue: ghCloneRepo example includes a concrete repo', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['ghCloneRepo'],
      options: { scheme: true },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('"owner":"bgauryy"');
    expect(output).toContain('"repo":"octocode"');
  });

  it('reports first failing query in a multi-query array', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',

      args: [
        'localSearchCode',
        '[{"path":".","keywords":"ok","matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1},{"path":".","keywords":999,"matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}]',
      ],
      options: {},
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Tool input does not match the expected schema.');
    expect(process.exitCode).toBe(2);
  });

  it('sortToolNames: tools in the same category maintain stable relative order', async () => {
    const { getToolsContextString } =
      await import('../../src/cli/tool-command.js');

    const context = await getToolsContextString();

    const ghIdx = context.indexOf('ghSearchCode');
    const cloneIdx = context.indexOf('ghCloneRepo');
    expect(ghIdx).toBeGreaterThan(-1);
    expect(cloneIdx).toBeGreaterThan(-1);

    expect(ghIdx).toBeLessThan(cloneIdx);
  });

  it('preserves user-supplied id, researchGoal, reasoning, mainResearchGoal', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: [
        'ghSearchCode',
        JSON.stringify({
          id: 'my-id',
          mainResearchGoal: 'my main goal',
          researchGoal: 'my goal',
          reasoning: 'my reasoning',
          keywords: ['test'],
        }),
      ],
      options: {},
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
      command: 'tools',
      args: [
        'localSearchCode',
        '{"path":".","keywords":"x","matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}',
      ],
      options: {},
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
      command: 'tools',
      args: [
        'localSearchCode',
        '{"path":".","keywords":"x","matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}',
      ],
      options: {},
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
      command: 'tools',
      args: [
        'localSearchCode',
        '{"path":".","keywords":"x","matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}',
      ],
      options: { json: true },
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
      command: 'tools',
      args: [
        'localSearchCode',
        '{"path":".","keywords":"x","matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}',
      ],
      options: { json: true },
    });

    const parsed = JSON.parse(consoleSpy.mock.calls.flat().join('\n'));
    expect(parsed.content).toEqual([{ type: 'text', text: 'txt' }]);
    expect(parsed.structuredContent).toBe('just a string');
  });

  it('buildExampleValue: lspGetSemantics example exercises semantic enum branches', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['lspGetSemantics'],
      options: { scheme: true },
    });

    const out = consoleSpy.mock.calls.flat().join('\n');
    expect(out).toContain('lspGetSemantics');
    expect(out).toContain('Input Schema');
    expect(out).toContain('definition');
  });

  it('buildDirectToolExampleQuery: emits concrete OQL and top-level tool examples', async () => {
    const { buildDirectToolExampleQuery, getDirectToolDisplayFields } =
      await import('@octocodeai/octocode-tools-core/schema');

    expect(buildDirectToolExampleQuery('oqlSearch')).toEqual({
      schema: 'oql',
      target: 'code',
      from: { kind: 'local', path: '.' },
      where: { kind: 'text', value: 'executeDirectTool' },
      view: 'discovery',
      limit: 5,
    });
    expect(buildDirectToolExampleQuery('ghHistoryResearch')).toMatchObject({
      type: 'prs',
      owner: 'bgauryy',
      repo: 'octocode',
      keywordsToSearch: ['localSearchCode'],
    });
    expect(buildDirectToolExampleQuery('ghHistoryResearch')).not.toHaveProperty(
      'content.patches.ranges.file'
    );

    const oqlTarget = getDirectToolDisplayFields('oqlSearch').find(
      field => field.name === 'target'
    );
    expect(oqlTarget?.type).toContain('materialize');
    expect(oqlTarget?.type).toContain('fixes');
    expect(oqlTarget?.type).toContain('dataflow');
  });

  it('shows the tool list when no positional tool name is given', async () => {
    const { executeToolCommand } =
      await import('../../src/cli/tool-command.js');

    const ok = await executeToolCommand({
      command: 'tools',
      args: [],
      options: {},
    });

    expect(ok).toBe(true);
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('localSearchCode');
  });
});
