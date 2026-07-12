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

const publicMocks = vi.hoisted(() => ({
  initialize: vi.fn().mockResolvedValue(undefined),
  initializeProviders: vi.fn().mockResolvedValue([]),
  loadToolContent: vi.fn().mockResolvedValue({
    systemPrompt: 'Use Octocode tools carefully.',
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
  afterAll(() => {
    if (oqlEnv.previous === undefined) delete process.env.ENABLE_OQL;
    else process.env.ENABLE_OQL = oqlEnv.previous;
  });

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
            researchGoal: 'Execute localSearchCode via octocode',
            reasoning: 'Executed via octocode tool command',
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
            mainResearchGoal: 'Execute ghSearchCode via octocode',
            researchGoal: 'Execute ghSearchCode via octocode',
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
    const output = consoleSpy.mock.calls
      .map((call: unknown[]) => call.map(String).join(' '))
      .join('\n');
    expect(output).toContain('Command Patterns');
    expect(output).toContain('"keywords":"buildDirectToolCommandPatterns"');
    expect(output).toContain('"pattern":"eval($X)"');
    expect(output).toContain('absolute path');
  });

  it('rejects legacy --input usage and points to the canonical contract', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearchCode'],
      options: {
        input:
          '{"path":".","keywords":"buildDirectToolCommandPatterns","matchContentLength":200,"itemsPerPage":1,"page":1,"maxMatchesPerFile":1}',
      },
    });

    expect(publicMocks.localSearchCode).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Use tools')
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

  it('rejects unknown raw tool fields without executing the tool', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: [
        'ghCloneRepo',
        '{"owner":"bgauryy","repo":"octocode","path":"docs","depth":1}',
      ],
      options: {},
    });

    expect(publicMocks.noop).not.toHaveBeenCalled();
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Unknown field(s): path, depth');
    expect(output).toContain('tools ghCloneRepo --scheme');
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

  it('prints machine-readable JSON for raw tool validation errors with --json', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearchCode'],
      options: { queries: 'null', json: true },
    });

    const parsed = JSON.parse(consoleSpy.mock.calls.flat().join('\n'));
    expect(parsed).toMatchObject({
      kind: 'octocode.toolError',
      tool: 'localSearchCode',
      error: expect.stringContaining('Tool input must be a JSON object'),
    });
    expect(process.exitCode).toBe(2);
  });

  it('gives a specific error when localSearchCode keywords is an array', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearchCode'],
      options: { queries: '{"path":".","keywords":["runCLI"]}' },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('localSearchCode.keywords must be a string');
    expect(output).toContain('ghSearchCode uses keywords as an array');
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
    expect(context).toContain('1. ghSearchCode');
    expect(context).toContain('2. ghCloneRepo');
    expect(context).toContain('3. localSearchCode');
    expect(context).toContain('Quick commands (search/clone/cache fetch)');
    expect(context).not.toContain('Quick commands (search/ls/cat/repo');
    expect(context).not.toMatch(
      /Quick commands \([^)]*\b(?:ls|cat|repo|history|binary|unzip|diff|pkg|lsp|find|grep)\b/
    );
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

  it('renders --scheme help for oqlSearch without throwing', async () => {
    const { showToolHelp } = await import('../../src/cli/tool-command.js');
    await expect(showToolHelp('oqlSearch')).resolves.toBe(true);
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
      toolCount: number;
      commands: { schema: string; fullCatalog: string };
      tools: Array<{
        name: string;
        category: string;
        description: string;
        fields: string;
        schemaCommand: string;
        runCommand: string;
      }>;
    };
    expect(parsed.kind).toBe('octocode.toolCatalog');
    expect(parsed.commands.schema).toBe('tools <name> --scheme --json');
    expect(parsed.commands.fullCatalog).toBe('tools --json --full');

    const { TOOL_DEFINITIONS } = await import('../../src/cli/tool-command.js');
    const names = parsed.tools.map(entry => entry.name).sort();
    expect(names).toEqual(TOOL_DEFINITIONS.map(t => t.name).sort());
    expect(parsed.toolCount).toBe(TOOL_DEFINITIONS.length);

    for (const entry of parsed.tools) {
      expect(typeof entry.name).toBe('string');
      expect(entry).toHaveProperty('category');
      expect(entry).toHaveProperty('description');
      expect(typeof entry.fields).toBe('string');
      expect(entry.schemaCommand).toBe(`tools ${entry.name} --scheme --json`);
      expect(entry.runCommand).toContain('--compact');
    }
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

    const localSearchCode = parsed.tools.find(
      entry => entry.name === 'localSearchCode'
    );
    expect(localSearchCode).toBeDefined();
    expect(localSearchCode?.fullDescription).toMatch(/Search local code/);
    expect(localSearchCode?.inputSchema?.type).toBe('object');
    expect(Array.isArray(localSearchCode?.fields)).toBe(true);
    expect(
      localSearchCode?.fields.some(
        field => typeof field.description === 'string'
      )
    ).toBe(true);
  });

  it('emits a single machine-readable schema for `tools <name> --scheme --json`', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearchCode'],
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
      fields: Array<{ name: string; required: boolean }>;
      commands: {
        catalog: string;
        schema: string;
        runCompact: string;
        runEnvelope: string;
      };
      guidance?: string[];
    };

    expect(parsed.kind).toBe('octocode.toolSchema');
    expect(parsed.name).toBe('localSearchCode');
    expect(parsed.inputSchema.type).toBe('object');
    expect(parsed.fields.some(field => field.name === 'path')).toBe(true);
    expect(parsed.commands.catalog).toBe('tools --json');
    expect(parsed.commands.schema).toBe(
      'tools localSearchCode --scheme --json'
    );
    expect(parsed.commands.runCompact).toContain('--compact');
    expect(parsed.commands.runEnvelope).toContain('tools localSearchCode');
    expect(parsed.guidance?.join('\n')).toContain('absolute path');
  });

  it('deduplicates prose in compact tool schemas', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tools',
      args: ['localSearchCode'],
      options: { scheme: true, json: true, compact: true },
    });

    const output = consoleSpy.mock.calls
      .map((call: unknown[]) => call.map(String).join(' '))
      .join('\n')
      .trim();

    const parsed = JSON.parse(output) as {
      inputSchema: { type?: string };
      fields?: unknown[];
      fieldNames?: string[];
      fullDescription?: string;
      guidance?: string[];
    };

    expect(parsed.inputSchema.type).toBe('object');
    expect(parsed.fields).toBeUndefined();
    expect(parsed.fullDescription).toBeUndefined();
    expect(parsed.fieldNames).toContain('path');
    expect(parsed.guidance?.join('\n')).toContain('absolute path');
  });
});
