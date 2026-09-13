import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const publicMocks = vi.hoisted(() => ({
  initialize: vi.fn().mockResolvedValue(undefined),
  initializeProviders: vi.fn().mockResolvedValue([]),

  localSearch: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'tool output' }],
  }),
  ghSearch: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'github output' }],
  }),
  ghGetFileContent: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'file content output' }],
  }),
  noop: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'ok' }],
  }),
}));

// Execution path is dynamically imported from `/direct`.
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

    if (toolName === 'localSearch') {
      return publicMocks.localSearch(input);
    }
    if (toolName === 'ghSearch') {
      return publicMocks.ghSearch(input);
    }
    if (toolName === 'ghGetFileContent') {
      return publicMocks.ghGetFileContent(input);
    }
    return publicMocks.noop(input);
  });

  return {
    ...actual,
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

  it('rejects positional JSON payloads and requires --queries', async () => {
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: [
        'localSearch',
        '{"path":".","searchText":"runCLI","regex":"literal","include":["ts","tsx"],"maxFiles":5,"matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
      ],
      options: {},
    });

    expect(publicMocks.localSearch).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('--queries')
    );
    expect(process.exitCode).toBe(2);
  });

  it('accepts JSON bulk payloads from --queries', async () => {
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['ghSearch'],
      options: {
        yaml: true,
        queries:
          '{"queries":[{"operation":"code","keywords":["tool"],"owner":"bgauryy","repo":"octocode-mcp"}],"responseCharLength":1200}',
      },
    });

    expect(publicMocks.initialize).toHaveBeenCalledTimes(1);
    expect(publicMocks.initializeProviders).toHaveBeenCalledTimes(1);
    expect(publicMocks.ghSearch).toHaveBeenCalledWith(
      expect.objectContaining({
        queries: [
          expect.objectContaining({
            keywords: ['tool'],
            operation: 'code',
            owner: 'bgauryy',
            repo: 'octocode-mcp',
            goal: 'Execute ghSearch via octocode',
            reasoning: 'Executed via octocode tool command',
          }),
        ],
      })
    );
    expect(consoleSpy).toHaveBeenCalledWith('github output');
  });

  it('supports JSON output mode for canonical tool execution', async () => {
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        json: true,
        queries:
          '{"path":".","searchText":"runCLI","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
      },
    });

    expect(publicMocks.localSearch).toHaveBeenCalledTimes(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('"content"')
    );
  });

  it.each([false, true])(
    'honors compact=%s when JSON output is explicitly selected',
    async compact => {
      const { toolCommand } =
        await import('../../src/cli/tool-command/command.js');
      const result = { content: [{ type: 'text', text: 'same evidence' }] };
      publicMocks.localSearch.mockResolvedValueOnce(result);

      await toolCommand.handler!({
        command: 'tools',
        args: ['localSearch'],
        options: {
          json: true,
          compact,
          queries: '{"path":".","searchText":"runCLI"}',
        },
      });

      expect(consoleSpy).toHaveBeenCalledWith(
        JSON.stringify(result, null, compact ? 0 : 2)
      );
    }
  );

  it('shows schema help when a tool is selected without input', async () => {
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {},
    });

    expect(publicMocks.localSearch).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('localSearch')
    );
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Input Schema')
    );
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining(
        'local CLI and MCP share the same tools-core runner'
      )
    );
  });

  it('shows schema help when --scheme is provided', async () => {
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: { scheme: true },
    });

    expect(publicMocks.localSearch).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('Example'));
    const output = consoleSpy.mock.calls
      .map((call: unknown[]) => call.map(String).join(' '))
      .join('\n');
    expect(output).toContain('Command Pattern');
    expect(output).toContain('"searchText":"buildDirectToolCommandPatterns"');
    expect(output).toContain('absolute path');
  });

  it('treats removed --input as an unsupported tool flag', async () => {
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        input:
          '{"path":".","searchText":"buildDirectToolCommandPatterns","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
      },
    });

    expect(publicMocks.localSearch).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Unknown localSearch flag: --input')
    );
    expect(process.exitCode).toBe(2);
  });

  it('rejects flags that are not schema fields with a suggestion', async () => {
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        path: '.',
        keywords: 'runCLI',
      },
    });

    expect(publicMocks.localSearch).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Unknown localSearch flag: --keywords')
    );
    expect(process.exitCode).toBe(2);
  });

  it('builds a query from schema field flags without --queries', async () => {
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {},
      raw: [
        'tools',
        'localSearch',
        '--path',
        '.',
        '--search-text',
        'runCLI',
        '--max-files',
        '5',
        '--whole-word',
        '--include',
        '*.ts',
        '--include',
        '*.mjs',
      ],
    });

    expect(publicMocks.localSearch).toHaveBeenCalledTimes(1);
    const callArg = publicMocks.localSearch.mock.calls[0]![0] as {
      queries: Array<Record<string, unknown>>;
    };
    expect(callArg.queries[0]).toEqual(
      expect.objectContaining({
        path: '.',
        searchText: 'runCLI',
        maxFiles: 5,
        wholeWord: true,
        include: ['*.ts', '*.mjs'],
      })
    );
    expect(process.exitCode).toBeUndefined();
  });

  it('rejects invalid JSON payloads for canonical tool usage', async () => {
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        queries: '{"path":".","searchText":"runCLI"',
      },
    });

    expect(publicMocks.localSearch).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Tool input must be valid JSON')
    );
    expect(process.exitCode).toBe(2);
  });

  it('keeps malformed batch input strict and gives a runnable batch envelope', async () => {
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        queries:
          '{"queries":[{"path":".","searchText":"first"},{"path":".","searchText":"second"}',
      },
    });

    expect(publicMocks.localSearch).not.toHaveBeenCalled();
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Tool input must be valid JSON');
    const diagnostic = JSON.parse(output) as { details?: string[] };
    const example = diagnostic.details?.[0]?.match(/--queries '([^']+)'/);
    expect(example).not.toBeNull();
    const { buildDirectToolCommandPatterns } =
      await import('@octocodeai/octocode-core/schema');
    expect(JSON.parse(example![1]!)).toEqual({
      queries: [buildDirectToolCommandPatterns('localSearch')[0]!.query],
    });
    expect(process.exitCode).toBe(2);
  });

  it('schema validation failure should show error', async () => {
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        queries:
          '{"path":".","searchText":999,"matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
      },
    });

    expect(publicMocks.localSearch).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Check the query fields.')
    );
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('searchText:')
    );
    expect(process.exitCode).toBe(2);
  });

  it('rejects unknown raw tool fields without executing the tool', async () => {
    process.env.ENABLE_CLONE = 'true';
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    try {
      await toolCommand.handler!({
        command: 'tools',
        args: ['ghCloneRepo'],
        options: {
          queries:
            '{"owner":"bgauryy","repo":"octocode","path":"docs","depth":1}',
        },
      });

      expect(publicMocks.noop).not.toHaveBeenCalled();
      const output = consoleSpy.mock.calls.flat().join('\n');
      expect(output).toContain('Unknown field(s): path, depth');
      expect(output).toContain('tools ghCloneRepo --scheme');
      expect(process.exitCode).toBe(2);
    } finally {
      delete process.env.ENABLE_CLONE;
    }
  });

  it('suggests only fields valid for the selected tool variant', async () => {
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        queries: '{"path":".","searchText":"runCLI","maxResults":5}',
      },
    });

    expect(publicMocks.localSearch).not.toHaveBeenCalled();
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain("'maxResults' → did you mean 'maxFiles'?");
    expect(output).not.toContain("did you mean 'limit'");
    expect(process.exitCode).toBe(2);
  });

  it('tool execution throwing should show error and return false', async () => {
    const err = new Error('Ripgrep launcher failed.');
    publicMocks.localSearch.mockRejectedValueOnce(err);

    const { executeToolCommand } =
      await import('../../src/cli/tool-command/execute.js');
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    const ok = await executeToolCommand({
      command: 'tools',
      args: ['localSearch'],
      options: {
        queries:
          '{"path":".","searchText":"runCLI","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
      },
    });

    expect(ok).toBe(false);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Ripgrep launcher failed.')
    );

    process.exitCode = undefined;
    consoleSpy.mockClear();

    publicMocks.localSearch.mockRejectedValueOnce(err);

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        queries:
          '{"path":".","searchText":"runCLI","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
      },
    });

    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Ripgrep launcher failed.')
    );
    expect(process.exitCode).toBe(5);

    vi.mocked(publicMocks.localSearch).mockResolvedValue({
      content: [{ type: 'text', text: 'tool output' }],
    });
  });

  it('shows multiple tool schemas when given multiple tool-name args', async () => {
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch', 'localSearch'],
      options: {},
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('localSearch');
    expect(output).toContain('localSearch');
  });

  it('emits one lean JSON envelope for multiple compact schemas', async () => {
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch', 'localSearch', 'astSearch'],
      options: { scheme: true, json: true, compact: true },
    });

    const parsed = JSON.parse(consoleSpy.mock.calls.flat().join('\n')) as {
      kind: string;
      schemas: Array<{
        name: string;
        description: string;
        inputSchema?: unknown;
        output?: unknown;
        outputScope?: unknown;
      }>;
    };
    expect(parsed.kind).toBe('octocode.toolSchemas.compact');
    expect(parsed.schemas.map(schema => schema.name)).toEqual([
      'localSearch',
      'localSearch',
      'astSearch',
    ]);
    expect(
      parsed.schemas.every(schema => schema.inputSchema === undefined)
    ).toBe(true);
    expect(parsed.schemas.every(schema => schema.output === undefined)).toBe(
      true
    );
    expect(
      parsed.schemas.every(schema => schema.outputScope === undefined)
    ).toBe(true);
    expect(
      parsed.schemas.every(schema => schema.description.length < 200)
    ).toBe(true);
  });

  it('keeps the combined GitHub history schema envelope within its frozen budget', async () => {
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['ghSearchHistory', 'ghGetHistoryItem'],
      options: { scheme: true, json: true, compact: true },
    });

    const output = consoleSpy.mock.calls.flat().join('\n').trim();
    const parsed = JSON.parse(output) as {
      schemas: Array<Record<string, unknown> & { name: string }>;
    };
    expect(parsed.schemas.map(schema => schema.name)).toEqual([
      'ghSearchHistory',
      'ghGetHistoryItem',
    ]);
    expect(parsed.schemas.every(schema => !('outputSchema' in schema))).toBe(
      true
    );
    expect(output).not.toMatch(
      /\b(?:ghSearchPullRequests|ghSearchIssues|ghSearchCommits|prNumber|issueNumber)\b/
    );
    expect(output.length).toBeLessThanOrEqual(5778);
  });

  it('shows error and tool help when --queries input cannot be parsed into a valid tool input', async () => {
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: { queries: 'null' },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Tool input must be a JSON object');
  });

  it('prints machine-readable JSON for raw tool validation errors with --json', async () => {
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: { queries: 'null', json: true },
    });

    const parsed = JSON.parse(consoleSpy.mock.calls.flat().join('\n'));
    expect(parsed).toMatchObject({
      kind: 'octocode.toolError',
      tool: 'localSearch',
      error: expect.stringContaining('Tool input must be a JSON object'),
    });
    expect(process.exitCode).toBe(2);
  });

  it('gives a specific error when localSearch keywords is an array', async () => {
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        yaml: true,
        queries: '{"path":".","keywords":["runCLI"]}',
      },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain(
      'localSearch does not accept keywords; set searchText to one string.'
    );
    expect(output).toContain('"path":"/ABS/repo"');
    expect(output).toContain('tools localSearch --scheme --brief');
    expect(output).not.toContain('ghSearch');
    expect(process.exitCode).toBe(2);
  });

  it('builds tools context from MCP instructions and tool schemas (--full)', async () => {
    const { getToolsContextString } =
      await import('../../src/cli/tool-command/context.js');

    const context = await getToolsContextString({ full: true });

    expect(context).toContain('TOOL CALLS');
    expect(context).toContain('tools <name>');
    expect(context).toContain('Choose a tool for the next unresolved question');
    expect(context).toContain('1. ghSearch');
    expect(context).toContain('2. ghSearchHistory');
    expect(context).toContain('3. ghGetHistoryItem');
    expect(context).toContain('5. ghCloneRepo');
    expect(context).toContain('6. localSearch');
    expect(context).not.toMatch(
      /\b(?:ghSearchPullRequests|ghSearchIssues|ghSearchCommits|prNumber|issueNumber)\b/
    );
    expect(context).toContain(
      'use the clone tool when a cached checkout is needed'
    );
    expect(context).toContain('CLI JSON modes omit that duplicate text');
    expect(context).toContain(
      'ordered rows: index, optional status/meta, and data'
    );
    expect(context).toContain(
      'Follow executable next.* calls with their scope and snapshot unchanged. Partial scan/depth state can require continuation even when hasMore is false. Whole-response pagination may split results; a restart discards earlier pages.'
    );
    expect(context).not.toContain('mode:"discovery"');
    expect(context).not.toContain('Cheap modes: concise:true');
    expect(context).not.toContain('structuredContent.responsePagination');
    expect(context).not.toContain(
      'Follow returned data.next/data.pagination only when hasMore.'
    );
    expect(context).not.toContain('only while row data.pagination.hasMore');
    expect(context).not.toMatch(
      /Quick commands \([^)]*\b(?:search|ls|cat|repo|history|binary|unzip|diff|pkg|lsp|find|grep)\b/
    );
    // full mode includes complete tool descriptions
    expect(context).toContain(
      'Discover GitHub repositories, indexed code, or a repository tree.'
    );
    expect(context).toContain('Cache a shallow GitHub checkout');
  });

  it('builds a lean default tools context (compact field lists)', async () => {
    const { getToolsContextString } =
      await import('../../src/cli/tool-command/context.js');
    const { TOOL_DEFINITIONS } =
      await import('../../src/cli/tool-command/registry.js');

    const context = await getToolsContextString();

    // lean mode includes short tool descriptions inline
    expect(context).toContain('1. ghSearch — Discover GitHub repositories');
    expect(context).not.toContain('"$schema"');
    expect(context).toContain(
      'Check every requested fact against returned evidence'
    );
    expect(context).toContain(
      'Follow executable next.* calls with their scope and snapshot unchanged. Partial scan/depth state can require continuation even when hasMore is false. Whole-response pagination may split results; a restart discards earlier pages.'
    );
    expect(context).not.toContain('Use Octocode tools carefully.');
    expect(context.length).toBeLessThanOrEqual(4200);
    for (const tool of TOOL_DEFINITIONS) {
      expect(context).toContain(tool.name);
    }
  });

  it('supports minimal agent context for cheapest tool-name orientation', async () => {
    const { getToolsContextString } =
      await import('../../src/cli/tool-command/context.js');
    const { TOOL_DEFINITIONS } =
      await import('../../src/cli/tool-command/registry.js');

    const context = await getToolsContextString({ minimal: true });

    expect(context).toContain('Octocode CLI — Minimal Context');
    expect(context).toContain('Protocol: answer the next unresolved question');
    expect(context).toContain(
      'Output: minified structured JSON by default; --yaml human view. Input validation rejects the call; runtime row errors stay indexed and isolated.'
    );
    expect(context).not.toContain('Output contract (all tools)');
    expect(context).not.toContain('Use Octocode tools carefully.');
    expect(context.length).toBeLessThanOrEqual(750);
    for (const tool of TOOL_DEFINITIONS) {
      expect(context).toContain(tool.name);
    }
  });

  // Bug 1: `tools <name> --scheme` must never throw a ReferenceError for any
  // tool name (regression: OQL_TOOL_NAME was referenced but never defined, so
  // the --json envelope section of showToolHelp blew up at runtime).
  it('renders --scheme help for every direct tool without throwing', async () => {
    const { showToolHelp } = await import('../../src/cli/tool-command/help.js');
    const { TOOL_DEFINITIONS } =
      await import('../../src/cli/tool-command/registry.js');

    for (const tool of TOOL_DEFINITIONS) {
      await expect(showToolHelp(tool.name)).resolves.toBe(true);
    }
  });

  // Bug 2: bare `tools --json` (no tool name) must emit a lean machine-readable
  // discovery catalog, not the human-readable help text or every full schema.
  it('emits a lean JSON tool catalog for bare `tools --json`', async () => {
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: [],
      options: { json: true },
    });

    const output = consoleSpy.mock.calls
      .map((call: unknown[]) => call.map(String).join(' '))
      .join('\n')
      .trim();

    const parsed = JSON.parse(output) as {
      kind: string;
      output: string;
      toolCount: number;
      commands: { schema: string; fullCatalog: string; run: string };
      tools: Array<{
        name: string;
        category: string;
        description: string;
        fields: string;
        availability: { enabled: boolean; envVar?: string };
      }>;
    };
    expect(parsed.kind).toBe('octocode.toolCatalog');
    expect(parsed.output).toBe(
      'results[].{index,status?,meta,data?}; tool payload and continuations are row-local under data'
    );
    expect(parsed.commands.schema).toBe(
      'tools <name> --scheme --json --compact'
    );
    expect(parsed.commands.fullCatalog).toBe('tools --json --full');
    expect(parsed.commands.run).toContain('--queries');

    const { TOOL_DEFINITIONS } =
      await import('../../src/cli/tool-command/registry.js');
    const names = parsed.tools.map(entry => entry.name);
    expect(names).toEqual(TOOL_DEFINITIONS.map(t => t.name));
    expect(parsed.toolCount).toBe(TOOL_DEFINITIONS.length);
    expect(names).toEqual([
      'ghSearch',
      'ghGetFileContent',
      'ghSearchHistory',
      'ghGetHistoryItem',
      'artifactSearch',
      'ghCloneRepo',
      'localSearch',
      'astSearch',
      'astRewrite',
      'localFetch',
      'lspSearch',
    ]);
    expect(names).not.toEqual(
      expect.arrayContaining([
        'ghSearchPullRequests',
        'ghSearchIssues',
        'ghSearchCommits',
      ])
    );
    for (const entry of parsed.tools) {
      expect(typeof entry.name).toBe('string');
      expect(entry).toHaveProperty('category');
      expect(entry).toHaveProperty('description');
      expect(typeof entry.fields).toBe('string');
    }
    expect(output.length).toBeLessThanOrEqual(4000);
  });

  it('keeps the full all-tool schema dump behind `tools --json --full`', async () => {
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: [],
      options: { json: true, full: true },
    });

    const output = consoleSpy.mock.calls
      .map((call: unknown[]) => call.map(String).join(' '))
      .join('\n')
      .trim();

    const parsed = JSON.parse(output) as {
      kind: string;
      toolCount: number;
      commands: { list: string; schema: string };
      tools: Array<{
        name: string;
        fullDescription?: string;
        inputSchema?: { type?: string };
        fields: Array<{ name: string; description?: string }>;
      }>;
    };
    expect(parsed.kind).toBe('octocode.toolCatalog.full');
    expect(parsed.commands.list).toBe('tools --json');
    expect(parsed.commands.schema).toBe('tools <name> --scheme --json');
    expect(parsed.toolCount).toBe(parsed.tools.length);
    expect(parsed.tools.every(entry => !('outputSchema' in entry))).toBe(true);

    const localSearch = parsed.tools.find(
      entry => entry.name === 'localSearch'
    );
    expect(localSearch).toBeDefined();
    expect(localSearch?.fullDescription).toMatch(/literal or regex/);
    expect(localSearch?.inputSchema?.type).toBe('object');
    expect(Array.isArray(localSearch?.fields)).toBe(true);
    expect(
      localSearch?.fields.some(field => typeof field.description === 'string')
    ).toBe(true);
  });

  it('emits a single machine-readable schema for `tools <name> --scheme --json`', async () => {
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: { scheme: true, json: true },
    });

    const output = consoleSpy.mock.calls
      .map((call: unknown[]) => call.map(String).join(' '))
      .join('\n')
      .trim();

    const parsed = JSON.parse(output) as {
      kind: string;
      name: string;
      inputSchema: { type?: string };
      outputSchema?: never;
      variants?: Array<{
        name: string;
        fields?: string[];
      }>;
      fields: Array<{ name: string; required: boolean }>;
      commands: {
        catalog: string;
        schema: string;
        runCompact: string;
        runJson: string;
      };
      guidance?: string[];
      relations?: string[];
    };

    expect(parsed.kind).toBe('octocode.toolSchema');
    expect(parsed.name).toBe('localSearch');
    expect(parsed.inputSchema.type).toBe('object');
    expect(parsed).not.toHaveProperty('outputSchema');
    expect(parsed.fields.some(field => field.name === 'path')).toBe(true);
    expect(parsed.commands.catalog).toBe('tools --json');
    expect(parsed.commands.schema).toBe('tools localSearch --scheme --json');
    expect(parsed.commands.runCompact).toContain('--compact');
    expect(parsed.commands.runJson).toContain('tools localSearch');
    expect(parsed.guidance?.join('\n')).toContain('absolute path');
    expect(parsed.relations?.join('\n').toLowerCase()).toContain('regex');
    expect(parsed.variants?.map(variant => variant.name)).toEqual(['lexical']);
    const variants = new Map(
      parsed.variants?.map(variant => [variant.name, variant.fields])
    );
    expect(variants.get('lexical')).toBeUndefined();
  });

  it('pretty-prints compact JSON when --pretty is supplied', async () => {
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: { scheme: true, json: true, compact: true, pretty: true },
    });

    const output = consoleSpy.mock.calls
      .map((call: unknown[]) => call.map(String).join(' '))
      .join('\n')
      .trim();

    expect(output).toContain('\n  "kind"');
    expect(JSON.parse(output).kind).toBe('octocode.toolSchema.compact');
  });

  it('deduplicates prose in compact tool schemas', async () => {
    const { toolCommand } =
      await import('../../src/cli/tool-command/command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: { scheme: true, json: true, compact: true },
    });

    const output = consoleSpy.mock.calls
      .map((call: unknown[]) => call.map(String).join(' '))
      .join('\n')
      .trim();

    const parsed = JSON.parse(output) as {
      kind: string;
      inputSchema?: unknown;
      output?: never;
      outputScope?: never;
      variants?: Array<{
        name: string;
        fields?: string[];
      }>;
      fieldGroups?: Array<{ variants: string[]; fields: string[] }>;
      fields?: string[];
      fieldNames?: string[];
      fullDescription?: string;
      guidance?: string[];
      relations?: string[];
      commands: { full: string; run: string };
    };

    expect(parsed.kind).toBe('octocode.toolSchema.compact');
    expect(parsed.inputSchema).toBeUndefined();
    expect(parsed.output).toBeUndefined();
    expect(parsed.outputScope).toBeUndefined();
    expect(parsed.fullDescription).toBeUndefined();
    expect(parsed.fieldNames).toBeUndefined();
    expect(parsed.fields?.some(field => field.startsWith('path*:'))).toBe(true);
    expect(parsed.commands.full).toBe('tools localSearch --scheme --json');
    expect(parsed.commands.run).toContain('--queries');
    expect(parsed.guidance?.join('\n')).toContain('absolute path');
    expect(parsed.relations?.join('\n').toLowerCase()).toContain('regex');
    expect(parsed.variants?.map(variant => variant.name)).toEqual(['lexical']);
    const variants = new Map(
      parsed.variants?.map(variant => [variant.name, variant.fields])
    );
    const fieldGroups = parsed.fieldGroups ?? [];
    expect(variants.get('lexical')).toEqual([]);
    expect(fieldGroups).toEqual([]);
    expect(parsed.fields?.some(field => field.startsWith('searchText'))).toBe(
      true
    );
    expect(parsed.fields?.some(field => field.startsWith('time.'))).toBe(false);
    expect(parsed.variants?.every(variant => !('example' in variant))).toBe(
      true
    );
    expect(Buffer.byteLength(output)).toBeLessThanOrEqual(3600);
  });

  it('rejects removed directory mode before invoking the file reader', async () => {
    const { executeToolCommand } =
      await import('../../src/cli/tool-command/execute.js');
    const ok = await executeToolCommand({
      command: 'tools',
      args: ['ghGetFileContent'],
      options: {
        compact: true,
        queries: JSON.stringify({
          owner: 'octocat',
          repo: 'Hello-World',
          path: 'src',
          type: 'directory',
        }),
      },
    });

    expect(ok).toBe(false);
    expect(publicMocks.ghGetFileContent).not.toHaveBeenCalled();
  });
});
