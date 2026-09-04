import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  initialize: vi.fn().mockResolvedValue(undefined),
  initializeProviders: vi.fn().mockResolvedValue([]),
  loadToolContent: vi.fn().mockResolvedValue({
    systemPrompt: 'Server instructions.',
    prompts: {},
    toolNames: {},
    baseSchema: {},
    tools: {
      ghSearch: {
        name: 'ghSearch',
        description: 'Search GitHub code, repositories, or trees.',
        schema: { operation: 'operation', keywords: 'terms', owner: 'owner' },
        hints: { hasResults: [], empty: [] },
      },
      localSearch: {
        name: 'localSearch',
        description: 'Local search.',
        schema: { path: 'dir', searchText: 'regex' },
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
  localSearch: vi.fn().mockResolvedValue({
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

    if (toolName === 'localSearch') {
      return mocks.localSearch(input);
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
    expect(output).toContain('localSearch');
    expect(output).toContain('Full protocol: context --full');
    expect(output).toContain('ghSearch');
    expect(output).toContain('Choose operation:"code"');
    expect(output).toContain('localSearch');
    expect(output).not.toContain('[path*');
    expect(output).not.toContain('workspaceSymbol');
    expect(output).not.toContain('diagnostic');
    expect(output).toContain('tools <name>');
    expect(process.exitCode).toBeUndefined();
  });

  it('rejects the removed tools --list alias', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: [],
      options: { list: true },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Unsupported tools option: --list');
    expect(process.exitCode).toBe(2);
  });

  it('rejects the removed tools list alias', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['list'],
      options: {},
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Unknown tool: list');
    expect(process.exitCode).toBe(3);
  });

  it('printToolsContext: prints full context to stdout', async () => {
    const { printToolsContext } = await import('../../src/cli/tool-command.js');

    await printToolsContext();

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Octocode CLI — Agent Context');
    expect(output).toContain('tools <name>');
    expect(output).toContain('Protocol: schema first');
    expect(output).toContain('Tools (');
    expect(output).not.toContain('Server instructions.');
    expect(output).toContain('Output contract');
  });

  it('A2: default context uses compact field lists, not full JSON schemas', async () => {
    const { getToolsContextString } =
      await import('../../src/cli/tool-command.js');

    const compact = await getToolsContextString();
    const full = await getToolsContextString({ full: true });

    // Schemas are no longer embedded in context — read them on demand via octocode tools <name>
    expect(compact).not.toContain('"$schema"');
    expect(compact).toContain('Protocol: schema first');
    expect(full).toContain('RESEARCH LOOP');
    // full mode includes the complete description text on a separate line
    expect(full).toContain('Choose operation:"code"');
    expect(full).toContain('Best for repeated reads');
  });

  it('A1: --compact emits minified structuredContent only', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');
    mocks.localSearch.mockResolvedValueOnce({
      content: [{ type: 'text', text: 'results:\n  - id: x' }],
      structuredContent: {
        results: [{ id: 'x' }],
        evidence: { answerReady: true },
      },
      isError: false,
    });

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        queries: '{"operation":"text","path":".","searchText":"x"}',
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
      args: ['localSearch'],
      options: { format: 'tool' },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    const def = JSON.parse(output.trim());
    expect(def.name).toBe('localSearch');
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

  it('rejects clone execution when ENABLE_CLONE=false', async () => {
    const previous = process.env.ENABLE_CLONE;
    const previousMode = process.env.OCTOCODE_STORAGE_MODE;
    process.env.ENABLE_CLONE = 'false';
    process.env.OCTOCODE_STORAGE_MODE = 'persistent';
    try {
      const { toolCommand } = await import('../../src/cli/tool-command.js');
      await toolCommand.handler!({
        command: 'tools',
        args: ['ghCloneRepo'],
        options: {
          queries: '{"owner":"octocat","repo":"Hello-World"}',
          compact: true,
        },
      });

      const output = consoleSpy.mock.calls.flat().join('\n');
      expect(output).toContain("Tool 'ghCloneRepo' is disabled");
      expect(mocks.cloneRepo).not.toHaveBeenCalled();
      expect(process.exitCode).toBe(3);
    } finally {
      if (previous === undefined) delete process.env.ENABLE_CLONE;
      else process.env.ENABLE_CLONE = previous;
      if (previousMode === undefined) delete process.env.OCTOCODE_STORAGE_MODE;
      else process.env.OCTOCODE_STORAGE_MODE = previousMode;
    }
  });

  it('reports clone unavailable in memory-only storage mode', async () => {
    const previousMode = process.env.OCTOCODE_STORAGE_MODE;
    process.env.OCTOCODE_STORAGE_MODE = 'memory';
    try {
      const { toolCommand } = await import('../../src/cli/tool-command.js');
      await toolCommand.handler!({
        command: 'tools',
        args: ['ghCloneRepo'],
        options: {
          queries: '{"owner":"octocat","repo":"Hello-World"}',
          compact: true,
        },
      });

      const output = consoleSpy.mock.calls.flat().join('\n');
      expect(output).toContain("Tool 'ghCloneRepo' is disabled");
      expect(output).toContain('OCTOCODE_STORAGE_MODE=persistent');
      expect(mocks.cloneRepo).not.toHaveBeenCalled();
      expect(process.exitCode).toBe(3);
    } finally {
      if (previousMode === undefined) delete process.env.OCTOCODE_STORAGE_MODE;
      else process.env.OCTOCODE_STORAGE_MODE = previousMode;
    }
  });

  it('getToolsContextString: excludes metadata-only tools that are not active CLI tools', async () => {
    const { getToolsContextString } =
      await import('../../src/cli/tool-command.js');

    const context = await getToolsContextString();

    expect(context).not.toContain('legacyTool');
    expect(context).not.toContain('Legacy tool.');
    expect(context).toContain('Tools (');
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

  it('showToolHelp: GitHub tool shows goal in auto-filled hint', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['ghSearch'],
      options: { scheme: true },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('goal');
    expect(output).not.toContain('mainResearchGoal');
    expect(output).not.toContain('researchGoal');
    expect(output).toContain('ghSearch');
    expect(output).toContain('keywords');
  });

  it('showToolHelp: local tool does not expose legacy goal names', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: { scheme: true },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('localSearch');
    expect(output).toContain('Input Schema');

    expect(output).not.toContain('mainResearchGoal');
    expect(output).not.toContain('researchGoal');
  });

  it('brief schema help emits signatures without verbose field prose', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['ghSearch'],
      options: { scheme: true, brief: true },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('ghSearch');
    expect(output).toContain('keywords?:array<string>');
    expect(output).toContain("tools ghSearch --queries '");
    expect(output).not.toContain('ANDed; keep a phrase as one item');
    expect(output).not.toContain('Runtime: local CLI and MCP');
    expect(output).not.toContain('Auto-filled');
  });

  it('brief schema help renders union ownership within a lean budget', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: { scheme: true, brief: true },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Variants');
    expect(output).toContain('text:');
    expect(output).toContain('structural:');
    expect(output).toContain('searchText*:string');
    expect(output).toContain('pattern?:string');
    expect(output).toContain(
      'full fields: tools localSearch --scheme --json --compact'
    );
    expect(Buffer.byteLength(output)).toBeLessThanOrEqual(1900);
  });

  it('keeps unscoped LSP variants in compact schema output', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['lspGetSemantics'],
      options: { scheme: true, json: true, compact: true },
    });

    const parsed = JSON.parse(consoleSpy.mock.calls.flat().join('\n')) as {
      variants?: Array<{ name: string; requires?: string[] }>;
    };
    expect(parsed.variants?.map(variant => variant.name)).toEqual([
      'anchored',
      'document',
      'workspace',
    ]);
    expect(parsed.variants?.[0]?.requires).toEqual([
      'uri',
      'type',
      'symbolName',
      'lineHint',
    ]);
  });

  it('ghCloneRepo: executes with owner and repo fields', async () => {
    process.env.ENABLE_CLONE = 'true';
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    try {
      await toolCommand.handler!({
        command: 'tools',
        args: ['ghCloneRepo'],
        options: { queries: '{"owner":"bgauryy","repo":"octocode-mcp"}' },
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
    } finally {
      delete process.env.ENABLE_CLONE;
    }
  });

  it('ghCloneRepo: branch is forwarded correctly', async () => {
    process.env.ENABLE_CLONE = 'true';
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    try {
      await toolCommand.handler!({
        command: 'tools',
        args: ['ghCloneRepo'],
        options: {
          queries: '{"owner":"bgauryy","repo":"octocode-mcp","branch":"main"}',
        },
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
    } finally {
      delete process.env.ENABLE_CLONE;
    }
  });

  it('accepts an array of query objects directly', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        queries:
          '[{"operation":"text","path":".","searchText":"foo","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1},{"operation":"text","path":"src","searchText":"bar","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}]',
      },
    });

    expect(mocks.localSearch).toHaveBeenCalledWith(
      expect.objectContaining({
        queries: expect.arrayContaining([
          expect.objectContaining({ path: '.', searchText: 'foo' }),
          expect.objectContaining({ path: 'src', searchText: 'bar' }),
        ]),
      })
    );
  });

  it('forwards envelope-level fields like responseCharOffset to the tool', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        queries:
          '{"queries":[{"operation":"text","path":".","searchText":"foo","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}],"responseCharOffset":500}',
      },
    });

    const callArg = mocks.localSearch.mock.calls[0]?.[0];
    expect(callArg).toEqual(
      expect.objectContaining({
        queries: [expect.objectContaining({ path: '.', searchText: 'foo' })],
        responseCharOffset: 500,
      })
    );
  });

  it('errors when more than two positional args are supplied', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: [
        'localSearch',
        '{"operation":"text","path":".","searchText":"x","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
        'extra',
      ],
      options: {},
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Pass tool input with --queries');
    expect(process.exitCode).toBe(2);
  });

  it('errors on non-string / non-object / non-array raw payload (e.g. number)', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: { queries: '42' },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Tool input must be a JSON object');
    expect(process.exitCode).toBe(2);
  });

  it('errors when queries array is empty', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: { queries: '{"queries":[]}' },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('At least one query is required');
    expect(process.exitCode).toBe(2);
  });

  it('uses canonical query keys for localSearch pagination', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');
    const consoleErrorSpy = vi
      .spyOn(console, 'error')
      .mockImplementation(() => {});

    try {
      await toolCommand.handler!({
        command: 'tools',
        args: ['localSearch'],
        options: {
          queries:
            '{"operation":"text","path":".","searchText":"x","regex":"fixed","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
        },
      });

      expect(mocks.localSearch).toHaveBeenCalledWith(
        expect.objectContaining({
          queries: [
            expect.objectContaining({
              regex: 'fixed',
              pageSize: 1,
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
    mocks.localSearch.mockResolvedValueOnce({
      content: [],
      structuredContent: { status: 'ok', count: 3 },
    });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        queries:
          '{"operation":"text","path":".","searchText":"x","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
      },
    });

    const allArgs = consoleSpy.mock.calls.flat().join('\n');
    expect(allArgs).toContain('"status": "ok"');
  });

  it('printToolResult: falls back to JSON.stringify(result) when no content and no structuredContent', async () => {
    mocks.localSearch.mockResolvedValueOnce({
      content: [],
    });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        queries:
          '{"operation":"text","path":".","searchText":"x","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
      },
    });

    const allArgs = consoleSpy.mock.calls.flat().join('\n');
    expect(allArgs).toContain('"content"');
  });

  it('printToolResult: --json mode prints the full MCP CallToolResult envelope', async () => {
    mocks.localSearch.mockResolvedValueOnce({
      content: [{ type: 'text', text: 'yaml output' }],
      structuredContent: { kind: 'results', items: ['a', 'b'] },
      isError: false,
    });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        json: true,
        queries:
          '{"operation":"text","path":".","searchText":"x","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
      },
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
    mocks.localSearch.mockResolvedValueOnce({
      content: [{ type: 'text', text: 'yaml output' }],
      structuredContent: {
        base: '/repo/src',
        results: [{ id: 'q1', status: 'hasResults', data: {} }],
      },
    });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        json: true,
        queries:
          '{"operation":"text","path":".","searchText":"x","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
      },
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
    mocks.localSearch.mockResolvedValueOnce({
      content: [{ type: 'text', text: 'out' }],
      structuredContent: { answer: 42 },
    });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        json: true,
        queries:
          '{"operation":"text","path":".","searchText":"x","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
      },
    });

    const raw = consoleSpy.mock.calls.flat().join('\n');
    const parsed = JSON.parse(raw);
    expect(parsed.content).toEqual([{ type: 'text', text: 'out' }]);
    expect(parsed.structuredContent).toEqual({ answer: 42 });
  });

  it('sets exitCode TOOL (5) when tool returns isError: true', async () => {
    mocks.localSearch.mockResolvedValueOnce({
      content: [{ type: 'text', text: 'failed' }],
      isError: true,
    });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        queries:
          '{"operation":"text","path":".","searchText":"x","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
      },
    });

    expect(process.exitCode).toBe(5);
  });

  it('handles non-Error thrown value in tool execution', async () => {
    mocks.localSearch.mockRejectedValueOnce('string error');

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        queries:
          '{"operation":"text","path":".","searchText":"x","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
      },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Tool execution failed.');
    expect(process.exitCode).toBe(5);
  });

  it('handles non-Error thrown by the execution function', async () => {
    mocks.localSearch.mockRejectedValueOnce(42);

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        queries:
          '{"operation":"text","path":".","searchText":"x","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
      },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');

    expect(output).toContain('Tool execution failed.');
    expect(process.exitCode).toBe(5);
  });

  it('getDisplayFields: returns MCP display fields for canonical tools', async () => {
    const { getDisplayFields, TOOL_DEFINITIONS } =
      await import('../../src/cli/tool-command.js');

    const githubTool = TOOL_DEFINITIONS.find(tool => tool.name === 'ghSearch');
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
    // npmSearch now surfaces two curated examples: the exact-package lookup and a
    // paged keyword-discovery example ({"packageName":"schema validation","page":1}).
    // The paged example legitimately includes "page"; assert the primary example stays clean of pagination noise.
    expect(output).toContain('{"packageName":"zod"}');
    expect(output).not.toContain('"limit"');
  });

  it('ghSearch repository help includes the unified schema', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['ghSearch'],
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

      args: ['localSearch'],
      options: {
        queries:
          '[{"operation":"text","path":".","searchText":"ok","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1},{"operation":"text","path":".","searchText":999,"matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}]',
      },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Check the query fields.');
    expect(process.exitCode).toBe(2);
  });

  it('sortToolNames: tools in the same category maintain stable relative order', async () => {
    const { getToolsContextString } =
      await import('../../src/cli/tool-command.js');

    const context = await getToolsContextString();

    const ghIdx = context.indexOf('ghSearch');
    const cloneIdx = context.indexOf('ghCloneRepo');
    expect(ghIdx).toBeGreaterThan(-1);
    expect(cloneIdx).toBeGreaterThan(-1);

    expect(ghIdx).toBeLessThan(cloneIdx);
  });

  it('preserves user-supplied goal and reasoning', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['ghSearch'],
      options: {
        queries: JSON.stringify({
          operation: 'code',
          goal: 'my goal',
          reasoning: 'my reasoning',
          keywords: ['test'],
        }),
      },
    });

    expect(mocks.noop).toHaveBeenCalledWith(
      expect.objectContaining({
        queries: [
          expect.objectContaining({
            goal: 'my goal',
            reasoning: 'my reasoning',
          }),
        ],
      })
    );
    expect(mocks.noop.mock.calls[0]?.[0].queries[0]).not.toHaveProperty('id');
  });

  it('showAvailableTools: returns null metadata gracefully when loadToolContent fails', async () => {
    mocks.loadToolContent.mockRejectedValueOnce(
      new Error('metadata unavailable')
    );

    const { showAvailableTools } =
      await import('../../src/cli/tool-command.js');

    await expect(showAvailableTools()).resolves.toBeUndefined();

    const output = consoleSpy.mock.calls.flat().join('\n');

    expect(output).toContain('localSearch');
  });

  it('printToolResult: uses structuredContent when result.content is undefined', async () => {
    mocks.localSearch.mockResolvedValueOnce({
      structuredContent: { found: true },
    } as unknown as { content: []; structuredContent: { found: boolean } });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        queries:
          '{"operation":"text","path":".","searchText":"x","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
      },
    });

    const out = consoleSpy.mock.calls.flat().join('\n');
    expect(out).toContain('"found": true');
  });

  it('printToolResult: content blocks with non-string text are filtered out', async () => {
    mocks.localSearch.mockResolvedValueOnce({
      content: [
        { type: 'image', data: 'base64...' },
        { type: 'text', text: '' },
        { type: 'text', text: 'real output' },
      ],
    } as unknown as { content: Array<{ type: string; text?: string }> });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        queries:
          '{"operation":"text","path":".","searchText":"x","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
      },
    });

    const out = consoleSpy.mock.calls.flat().join('\n');
    expect(out).toContain('real output');
    expect(out).not.toContain('base64');
  });

  it('printToolResult: JSON mode preserves null structuredContent in the envelope', async () => {
    mocks.localSearch.mockResolvedValueOnce({
      content: [{ type: 'text', text: 'txt' }],
      structuredContent: null,
    } as unknown as { content: Array<{ type: string; text: string }> });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        json: true,
        queries:
          '{"operation":"text","path":".","searchText":"x","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
      },
    });

    const parsed = JSON.parse(consoleSpy.mock.calls.flat().join('\n'));
    expect(parsed.content).toEqual([{ type: 'text', text: 'txt' }]);
    expect(parsed.structuredContent).toBeNull();
  });

  it('printToolResult: JSON mode preserves primitive structuredContent in the envelope', async () => {
    mocks.localSearch.mockResolvedValueOnce({
      content: [{ type: 'text', text: 'txt' }],
      structuredContent: 'just a string',
    } as unknown as { content: Array<{ type: string; text: string }> });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        json: true,
        queries:
          '{"operation":"text","path":".","searchText":"x","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
      },
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

  it('buildDirectToolExampleQuery: emits concrete top-level tool examples', async () => {
    const { buildDirectToolExampleQuery } =
      await import('@octocodeai/octocode-tools-core/schema');

    expect(buildDirectToolExampleQuery('ghSearchHistory')).toMatchObject({
      operation: 'pullRequests',
      owner: 'bgauryy',
      repo: 'octocode',
      keywords: ['localSearch'],
    });
    expect(buildDirectToolExampleQuery('ghSearchHistory')).not.toHaveProperty(
      'number'
    );
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
    expect(output).toContain('localSearch');
  });
});
