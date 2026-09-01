import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const publicMocks = vi.hoisted(() => ({
  initialize: vi.fn().mockResolvedValue(undefined),
  initializeProviders: vi.fn().mockResolvedValue([]),
  loadToolContent: vi.fn().mockResolvedValue({
    systemPrompt: 'Use Octocode tools carefully.',
    prompts: {},
    toolNames: {},
    baseSchema: {
      goal: 'goal',
      reasoning: 'reasoning',
      bulkQuery: (toolName: string) => `queries for ${toolName}`,
    },
    tools: {
      ghSearch: {
        name: 'ghSearch',
        description: 'Search GitHub code, repositories, or trees.',
        schema: { operation: 'Search operation' },
        hints: { hasResults: [], empty: [] },
      },
      localSearch: {
        name: 'localSearch',
        description: 'Search local code with ripgrep.',
        schema: {
          path: 'Path to search',
          searchText: 'Pattern to find',
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
  localSearch: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'tool output' }],
  }),
  ghSearch: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'github output' }],
  }),
  noop: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'ok' }],
  }),
}));

// Schema/help path now imports the engine-free `/schema` subpath (P3) — that is
// where `loadToolContent` and the meta/schema fns live.
vi.mock('@octocodeai/octocode-tools-core/schema', async importOriginal => {
  const actual =
    await importOriginal<
      typeof import('@octocodeai/octocode-tools-core/schema')
    >();
  return {
    ...actual,
    loadToolContent: publicMocks.loadToolContent,
  };
});

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

  it('rejects positional JSON payloads and requires --queries', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: [
        'localSearch',
        '{"operation":"text","path":".","searchText":"runCLI","regex":"fixed","include":["ts","tsx"],"maxFiles":5,"matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
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
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['ghSearch'],
      options: {
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
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        json: true,
        queries:
          '{"operation":"text","path":".","searchText":"runCLI","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
      },
    });

    expect(publicMocks.localSearch).toHaveBeenCalledTimes(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('"content"')
    );
  });

  it('shows schema help when a tool is selected without input', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

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
    const { toolCommand } = await import('../../src/cli/tool-command.js');

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
    expect(output).toContain('Command Patterns');
    expect(output).toContain('"searchText":"buildDirectToolCommandPatterns"');
    expect(output).toContain('"pattern":"eval($X)"');
    expect(output).toContain('absolute path');
  });

  it('treats removed --input as an unsupported tool flag', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        input:
          '{"operation":"text","path":".","searchText":"buildDirectToolCommandPatterns","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
      },
    });

    expect(publicMocks.localSearch).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Unsupported tool flags: --input')
    );
    expect(process.exitCode).toBe(2);
  });

  it('rejects legacy tool-specific flags and requires one JSON payload', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

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
      expect.stringContaining('Unsupported tool flags')
    );
    expect(process.exitCode).toBe(2);
  });

  it('rejects invalid JSON payloads for canonical tool usage', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        queries: '{"operation":"text","path":".","searchText":"runCLI"',
      },
    });

    expect(publicMocks.localSearch).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Tool input must be valid JSON')
    );
    expect(process.exitCode).toBe(2);
  });

  it('schema validation failure should show error', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        queries:
          '{"operation":"text","path":".","searchText":999,"matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
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
    const { toolCommand } = await import('../../src/cli/tool-command.js');

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
  });

  it('suggests only fields valid for the selected tool variant', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        queries:
          '{"operation":"text","path":".","searchText":"runCLI","maxResults":5}',
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

    const { executeToolCommand, toolCommand } =
      await import('../../src/cli/tool-command.js');

    const ok = await executeToolCommand({
      command: 'tools',
      args: ['localSearch'],
      options: {
        queries:
          '{"operation":"text","path":".","searchText":"runCLI","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
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
          '{"operation":"text","path":".","searchText":"runCLI","matchContentLength":200,"pageSize":1,"page":1,"maxMatchesPerFile":1}',
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
    const { toolCommand } = await import('../../src/cli/tool-command.js');

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
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch', 'localSearch', 'localAnalyzeGraph'],
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
      'localAnalyzeGraph',
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
    const { toolCommand } = await import('../../src/cli/tool-command.js');

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
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: { queries: 'null' },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Tool input must be a JSON object');
  });

  it('prints machine-readable JSON for raw tool validation errors with --json', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

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
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearch'],
      options: {
        queries: '{"operation":"text","path":".","keywords":["runCLI"]}',
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
      await import('../../src/cli/tool-command.js');

    const context = await getToolsContextString({ full: true });

    expect(publicMocks.loadToolContent).toHaveBeenCalledTimes(1);
    expect(context).toContain('TOOL CALLS');
    expect(context).toContain('tools <name>');
    expect(context).toContain('Use Octocode tools carefully.');
    expect(context).toContain('1. ghSearch');
    expect(context).toContain('2. ghSearchHistory');
    expect(context).toContain('3. ghGetHistoryItem');
    expect(context).toContain('5. ghCloneRepo');
    expect(context).toContain('6. localSearch');
    expect(context).not.toMatch(
      /\b(?:ghSearchPullRequests|ghSearchIssues|ghSearchCommits|prNumber|issueNumber)\b/
    );
    expect(context).toContain('`cache fetch` materializes content locally');
    expect(context).toContain(
      'MCP returns complete YAML text in content[].text plus full structuredContent'
    );
    expect(context).toContain('inspect each row status for mixed batches');
    expect(context).toContain(
      'Follow row data.next only while row data.pagination.hasMore. responsePagination windows content[].text only and remains visible in structuredContent.'
    );
    expect(context).not.toContain('mode:"discovery"');
    expect(context).not.toContain('Cheap modes: concise:true');
    expect(context).toContain(
      'structuredContent.responsePagination: object content[].text char window; structured data remains complete'
    );
    expect(context).not.toContain(
      'Follow returned data.next/data.pagination only when hasMore.'
    );
    expect(context).not.toMatch(
      /Quick commands \([^)]*\b(?:search|ls|cat|repo|history|binary|unzip|diff|pkg|lsp|find|grep)\b/
    );
    // full mode includes complete tool descriptions
    expect(context).toContain('Choose operation:"code"');
    expect(context).toContain('Best for repeated reads');
  });

  it('builds a lean default tools context (compact field lists)', async () => {
    const { getToolsContextString, TOOL_DEFINITIONS } =
      await import('../../src/cli/tool-command.js');

    const context = await getToolsContextString();

    // lean mode includes short tool descriptions inline
    expect(context).toContain('1. ghSearch — Choose operation:"code"');
    expect(context).not.toContain('"$schema"');
    expect(context).toContain('Protocol: schema first');
    expect(context).toContain(
      'Follow row data.next only while row data.pagination.hasMore. responsePagination windows content[].text only and remains visible in structuredContent.'
    );
    expect(context).not.toContain('Use Octocode tools carefully.');
    expect(context.length).toBeLessThanOrEqual(4000);
    for (const tool of TOOL_DEFINITIONS) {
      expect(context).toContain(tool.name);
    }
  });

  it('supports minimal agent context for cheapest tool-name orientation', async () => {
    const { getToolsContextString, TOOL_DEFINITIONS } =
      await import('../../src/cli/tool-command.js');

    const context = await getToolsContextString({ minimal: true });

    expect(context).toContain('Octocode CLI — Minimal Context');
    expect(context).toContain('Protocol: schema first');
    expect(context).toContain(
      'Output: YAML default; --compact structured JSON. Input validation rejects the call; runtime row errors stay indexed and isolated.'
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
    const { showToolHelp, TOOL_DEFINITIONS } =
      await import('../../src/cli/tool-command.js');

    for (const tool of TOOL_DEFINITIONS) {
      await expect(showToolHelp(tool.name)).resolves.toBe(true);
    }
  });

  // Bug 2: bare `tools --json` (no tool name) must emit a lean machine-readable
  // discovery catalog, not the human-readable help text or every full schema.
  it('emits a lean JSON tool catalog for bare `tools --json`', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

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
    expect(parsed.commands.run).toContain('--compact');

    const { TOOL_DEFINITIONS } = await import('../../src/cli/tool-command.js');
    const names = parsed.tools.map(entry => entry.name);
    expect(names).toEqual(TOOL_DEFINITIONS.map(t => t.name));
    expect(parsed.toolCount).toBe(TOOL_DEFINITIONS.length);
    expect(names).toEqual([
      'ghSearch',
      'ghGetFileContent',
      'ghSearchHistory',
      'ghGetHistoryItem',
      'npmSearch',
      'ghCloneRepo',
      'localSearch',
      'localAnalyzeGraph',
      'localGetFileContent',
      'lspGetSemantics',
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
    const { toolCommand } = await import('../../src/cli/tool-command.js');

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
    expect(localSearch?.fullDescription).toMatch(/Choose operation:"text"/);
    expect(localSearch?.inputSchema?.type).toBe('object');
    expect(Array.isArray(localSearch?.fields)).toBe(true);
    expect(
      localSearch?.fields.some(field => typeof field.description === 'string')
    ).toBe(true);
  });

  it('emits a single machine-readable schema for `tools <name> --scheme --json`', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

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
        runEnvelope: string;
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
    expect(parsed.commands.runEnvelope).toContain('tools localSearch');
    expect(parsed.guidance?.join('\n')).toContain('absolute path');
    expect(parsed.relations?.join('\n').toLowerCase()).toContain('structural');
    expect(parsed.variants?.map(variant => variant.name)).toEqual([
      'text',
      'structural',
      'files',
      'tree',
    ]);
    const variants = new Map(
      parsed.variants?.map(variant => [variant.name, variant.fields])
    );
    expect(variants.get('text')).toContain('searchText');
    expect(variants.get('text')).not.toContain('pattern');
    expect(variants.get('structural')).toContain('pattern');
    expect(variants.get('files')).toContain('pathRegex');
    expect(variants.get('tree')).toContain('namePattern');
  });

  it('pretty-prints compact JSON when --pretty is supplied', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

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
    const { toolCommand } = await import('../../src/cli/tool-command.js');

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
    expect(parsed.commands.run).toContain('--compact');
    expect(parsed.guidance?.join('\n')).toContain('absolute path');
    expect(parsed.relations?.join('\n').toLowerCase()).toContain('structural');
    expect(parsed.variants?.map(variant => variant.name)).toEqual([
      'text',
      'structural',
      'files',
      'tree',
    ]);
    const variants = new Map(
      parsed.variants?.map(variant => [variant.name, variant.fields])
    );
    const fieldGroups = parsed.fieldGroups ?? [];
    expect(variants.get('text')).toContain('searchText*:string');
    expect(
      variants.get('text')?.some(field => field.startsWith('pattern'))
    ).toBe(false);
    expect(variants.get('structural')).toEqual(
      expect.arrayContaining(['pattern?:string', 'rule?:string'])
    );
    expect(variants.get('files')).toContain('pathRegex?:string');
    expect(fieldGroups).toContainEqual(
      expect.objectContaining({
        variants: ['files', 'tree'],
        fields: expect.arrayContaining(['pageSize?:integer 1-50']),
      })
    );
    expect(fieldGroups).toContainEqual(
      expect.objectContaining({
        variants: ['text', 'structural'],
        fields: expect.arrayContaining(['pageSize?:integer 1-1000']),
      })
    );
    expect(variants.get('tree')).toContain('namePattern?:string');
    expect(parsed.fields?.some(field => field.startsWith('searchText'))).toBe(
      false
    );
    expect(parsed.fields?.some(field => field.startsWith('time.'))).toBe(false);
    expect(parsed.variants?.every(variant => !('example' in variant))).toBe(
      true
    );
    expect(Buffer.byteLength(output)).toBeLessThanOrEqual(3600);
  });
});
