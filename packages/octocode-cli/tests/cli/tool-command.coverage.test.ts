/**
 * Coverage-gap tests for src/cli/tool-command.ts
 *
 * Targets the uncovered branches and functions that the primary
 * tool-command.test.ts doesn't reach:
 *
 *  - showAvailableTools (list by category)
 *  - printToolsContext / getToolsContextString
 *  - formatMetadataSchemaText fallback (metadata-only tool)
 *  - describeSchemaType: enum, array<items>, array<no-items>,
 *    multi-type Array, fallback 'value'
 *  - buildExampleValue: all named-field branches + type branches
 *  - normalizeKey: kebab and underscore keys in query input
 *  - buildToolPayload: array payload, responseCharOffset, >2 args error,
 *    non-string/non-object/empty-array payload errors
 *  - printToolResult: structuredContent fallback, bare-result fallback,
 *    stripTsvRedundantResults non-TSV passthrough
 *  - executeToolCommand: no-name → list, 'list' keyword, --list flag,
 *    unknown tool, --schema flag, isError result, --json/-o json flags
 *  - showToolHelp: GitHub tool (mainResearchGoal hint), unknown tool
 *  - githubCloneRepo execution
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// ─── shared mocks ─────────────────────────────────────────────────────────────

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
      // Metadata-only tool (not in TOOL_DEFINITIONS) → formatMetadataSchemaText path
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

  // Schema with various field types to exercise describeSchemaType
  const richSchema = githubBase.extend({
    keywordsToSearch: z.array(z.string()),
    owner: z.string().optional(),
    repo: z.string().optional(),
    limit: z.number().optional(),
  });

  return {
    initialize: mocks.initialize,
    initializeProviders: mocks.initializeProviders,
    loadToolContent: mocks.loadToolContent,
    executeRipgrepSearch: mocks.localSearchCode,
    executeFetchContent: mocks.noop,
    executeFindFiles: mocks.noop,
    executeViewStructure: mocks.noop,
    executeGotoDefinition: mocks.noop,
    executeFindReferences: mocks.noop,
    executeCallHierarchy: mocks.noop,
    fetchMultipleGitHubFileContents: mocks.noop,
    searchMultipleGitHubCode: mocks.noop,
    searchMultipleGitHubPullRequests: mocks.noop,
    searchMultipleGitHubRepos: mocks.noop,
    exploreMultipleRepositoryStructures: mocks.noop,
    executeCloneRepo: mocks.cloneRepo,
    searchPackages: mocks.noop,
    RipgrepQuerySchema: localBase.extend({
      path: z.string(),
      pattern: z.string(),
      fixedString: z.boolean().optional(),
      include: z.array(z.string()).optional(),
      limit: z.number().optional(),
    }),
    FetchContentQuerySchema: localBase.extend({ path: z.string() }),
    FindFilesQuerySchema: localBase.extend({ path: z.string() }),
    ViewStructureQuerySchema: localBase.extend({ path: z.string() }),
    // LSP schemas with realistic required fields to exercise buildExampleValue branches
    LSPGotoDefinitionQuerySchema: localBase.extend({
      uri: z.string(),
      symbolName: z.string(),
      lineHint: z.number(),
    }),
    LSPFindReferencesQuerySchema: localBase.extend({
      uri: z.string(),
      symbolName: z.string(),
      lineHint: z.number(),
      includeDeclaration: z.boolean(),
    }),
    LSPCallHierarchyQuerySchema: localBase.extend({
      uri: z.string(),
      symbolName: z.string(),
      lineHint: z.number(),
      direction: z.enum(['incoming', 'outgoing']),
    }),
    FileContentQuerySchema: githubBase.extend({
      owner: z.string(),
      repo: z.string(),
      path: z.string(),
    }),
    GitHubCodeSearchQuerySchema: richSchema,
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

// ─── suite ────────────────────────────────────────────────────────────────────

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

  // ── showAvailableTools ────────────────────────────────────────────────────

  it('showAvailableTools: lists tools grouped by category', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    // No tool arg → falls through to showAvailableTools
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
    expect(output).toContain('Tip:');
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

  // ── printToolsContext / getToolsContextString ─────────────────────────────

  it('printToolsContext: prints full context to stdout', async () => {
    const { printToolsContext } = await import('../../src/cli/tool-command.js');

    await printToolsContext();

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('CLI Contract:');
    expect(output).toContain('Server instructions.');
  });

  it('getToolsContextString: includes metadata-only tool via formatMetadataSchemaText', async () => {
    const { getToolsContextString } =
      await import('../../src/cli/tool-command.js');

    const context = await getToolsContextString();

    // legacyTool is in metadata but NOT in TOOL_DEFINITIONS → formatMetadataSchemaText
    expect(context).toContain('legacyTool');
    expect(context).toContain('"foo": "Foo description"');
    expect(context).toContain('"bar": "Bar description"');
  });

  // ── unknown tool ─────────────────────────────────────────────────────────

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

  // ── showToolHelp ─────────────────────────────────────────────────────────

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
    // local tools do not show mainResearchGoal in auto-filled hint
    expect(output).not.toContain('mainResearchGoal');
  });

  // ── githubCloneRepo execution ─────────────────────────────────────────────

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
        format: 'tsv',
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

  // ── buildToolPayload: array payload ──────────────────────────────────────

  it('accepts an array of query objects directly', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: [
        'localSearchCode',
        '[{"path":".","pattern":"foo"},{"path":"src","pattern":"bar"}]',
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
        '{"queries":[{"path":".","pattern":"foo"}],"responseCharOffset":500}',
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
      args: ['localSearchCode', '{"path":".","pattern":"x"}', 'extra'],
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

  // ── normalizeKey ─────────────────────────────────────────────────────────

  it('normaliseKey: converts kebab-case query keys to camelCase', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    // keys-to-search → keysToSearch (not a real field, but validates normalisation)
    // Use a real field: 'fixed-string' → 'fixedString' for localSearchCode
    await toolCommand.handler!({
      command: 'tool',
      args: [
        'localSearchCode',
        '{"path":".","pattern":"x","fixed-string":true}',
      ],
      options: { tool: 'localSearchCode' },
    });

    expect(mocks.localSearchCode).toHaveBeenCalledWith(
      expect.objectContaining({
        queries: [expect.objectContaining({ fixedString: true })],
      })
    );
  });

  // ── printToolResult: fallback branches ───────────────────────────────────

  it('printToolResult: falls back to structuredContent when content is empty', async () => {
    mocks.localSearchCode.mockResolvedValueOnce({
      content: [],
      structuredContent: { status: 'ok', count: 3 },
    });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['localSearchCode', '{"path":".","pattern":"x"}'],
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
      args: ['localSearchCode', '{"path":".","pattern":"x"}'],
      options: { tool: 'localSearchCode' },
    });

    const allArgs = consoleSpy.mock.calls.flat().join('\n');
    expect(allArgs).toContain('"content"');
  });

  it('printToolResult: --json mode with non-TSV structuredContent passes through', async () => {
    mocks.localSearchCode.mockResolvedValueOnce({
      content: [{ type: 'text', text: 'yaml output' }],
      structuredContent: { kind: 'results', items: ['a', 'b'] },
    });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['localSearchCode', '{"path":".","pattern":"x"}'],
      options: { tool: 'localSearchCode', json: true },
    });

    const allArgs = consoleSpy.mock.calls.flat().join('\n');
    // structuredContent is passed through as-is (non-TSV)
    expect(allArgs).toContain('"kind"');
    expect(allArgs).toContain('"items"');
    // results array NOT stripped (not TSV format)
    expect(allArgs).not.toContain('yaml output');
  });

  it('printToolResult: --json mode strips results from TSV structuredContent', async () => {
    mocks.localSearchCode.mockResolvedValueOnce({
      content: [{ type: 'text', text: 'yaml output' }],
      structuredContent: {
        format: 'tsv',
        columns: ['path', 'line'],
        rows: 'path\tline\nfoo.ts\t1',
        results: [{ id: 'q1', status: 'hasResults', data: {} }],
      },
    });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['localSearchCode', '{"path":".","pattern":"x"}'],
      options: { tool: 'localSearchCode', json: true },
    });

    const raw = consoleSpy.mock.calls.flat().join('\n');
    const parsed = JSON.parse(raw);
    expect(parsed.format).toBe('tsv');
    expect(parsed.columns).toEqual(['path', 'line']);
    expect(parsed.rows).toContain('foo.ts');
    // results must be stripped
    expect(parsed.results).toBeUndefined();
  });

  it('printToolResult: -o json flag also selects JSON mode', async () => {
    mocks.localSearchCode.mockResolvedValueOnce({
      content: [{ type: 'text', text: 'out' }],
      structuredContent: { answer: 42 },
    });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['localSearchCode', '{"path":".","pattern":"x"}'],
      options: { tool: 'localSearchCode', o: 'json' },
    });

    const raw = consoleSpy.mock.calls.flat().join('\n');
    const parsed = JSON.parse(raw);
    expect(parsed.answer).toBe(42);
  });

  // ── isError result ────────────────────────────────────────────────────────

  it('sets exitCode 1 when tool returns isError: true', async () => {
    mocks.localSearchCode.mockResolvedValueOnce({
      content: [{ type: 'text', text: 'failed' }],
      isError: true,
    });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['localSearchCode', '{"path":".","pattern":"x"}'],
      options: { tool: 'localSearchCode' },
    });

    expect(process.exitCode).toBe(1);
  });

  // ── non-Error exception ───────────────────────────────────────────────────

  it('handles non-Error thrown value in tool execution', async () => {
    mocks.localSearchCode.mockRejectedValueOnce('string error');

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['localSearchCode', '{"path":".","pattern":"x"}'],
      options: { tool: 'localSearchCode' },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Tool execution failed.');
    expect(process.exitCode).toBe(1);
  });

  it('handles non-Error thrown by the execution function', async () => {
    // Verify that tool.execute throwing a non-Error (string) is handled gracefully.
    // The outer catch in executeToolCommand has an instanceof Error guard.
    mocks.localSearchCode.mockRejectedValueOnce(42); // non-Error number

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['localSearchCode', '{"path":".","pattern":"x"}'],
      options: { tool: 'localSearchCode' },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    // Falls into the non-Error branch → generic fallback message
    expect(output).toContain('Tool execution failed.');
    expect(process.exitCode).toBe(1);
  });

  // ── getDisplayFields edge cases ───────────────────────────────────────────

  it('getDisplayFields: handles schema with enum, array, and multi-type fields', async () => {
    const { z } = await import('zod/v4');
    const { getDisplayFields } = await import('../../src/cli/tool-command.js');

    const tool = {
      name: 'testTool',
      schema: z.object({
        id: z.string(),
        researchGoal: z.string(),
        reasoning: z.string(),
        mode: z.enum(['fast', 'full', 'off']),
        tags: z.array(z.string()),
        count: z.number(),
        verbose: z.boolean(),
      }),

      execute: async () => ({}) as any,
    };

    const fields = getDisplayFields(tool);
    const byName = Object.fromEntries(fields.map(f => [f.name, f]));

    // enum field
    expect(byName['mode']?.type).toContain('enum(');
    expect(byName['mode']?.type).toContain('fast');

    // array field
    expect(byName['tags']?.type).toBe('array<string>');

    // auto-filled fields must be absent
    expect(byName['id']).toBeUndefined();
    expect(byName['researchGoal']).toBeUndefined();
    expect(byName['reasoning']).toBeUndefined();
  });

  it('getDisplayFields: array field with no items schema falls back to array<value>', async () => {
    const { z } = await import('zod/v4');
    const { getDisplayFields } = await import('../../src/cli/tool-command.js');

    // z.array(z.unknown()) → JSON schema items will be empty/missing
    const tool = {
      name: 'unknownArrayTool',
      schema: z.object({
        id: z.string(),
        researchGoal: z.string(),
        reasoning: z.string(),
        data: z.array(z.unknown()),
      }),

      execute: async () => ({}) as any,
    };

    const fields = getDisplayFields(tool);
    const dataField = fields.find(f => f.name === 'data');
    // Should gracefully degrade to array<value> or array<...>
    expect(dataField?.type).toContain('array');
  });

  // ── buildExampleValue via showToolHelp example output ────────────────────

  it('buildExampleValue: packageSearch example includes ecosystem=npm and name=react', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['packageSearch'],
      options: { tool: 'packageSearch', schema: true },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('"ecosystem"');
    expect(output).toContain('npm');
    expect(output).toContain('"name"');
    expect(output).toContain('react');
  });

  it('buildExampleValue: githubSearchRepositories example includes keywordsToSearch array', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['githubSearchRepositories'],
      options: { tool: 'githubSearchRepositories', schema: true },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('"keywordsToSearch"');
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

  // ── multi-query validation: first fails, stops at first failure ───────────

  it('reports first failing query in a multi-query array', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      // second query has pattern as a number → validation failure
      args: [
        'localSearchCode',
        '[{"path":".","pattern":"ok"},{"path":".","pattern":999}]',
      ],
      options: { tool: 'localSearchCode' },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Tool input does not match the expected schema.');
    expect(process.exitCode).toBe(1);
  });

  // ── sortToolNames: same-category compare returns 0 ───────────────────────

  it('sortToolNames: tools in the same category maintain stable relative order', async () => {
    const { getToolsContextString } =
      await import('../../src/cli/tool-command.js');

    const context = await getToolsContextString();

    // Both githubSearchCode and githubCloneRepo are in the GitHub category.
    // They must both appear and their order must be stable (no crash).
    const ghIdx = context.indexOf('githubSearchCode');
    const cloneIdx = context.indexOf('githubCloneRepo');
    expect(ghIdx).toBeGreaterThan(-1);
    expect(cloneIdx).toBeGreaterThan(-1);
    // githubSearchCode appears before githubCloneRepo in TOOL_DEFINITIONS
    expect(ghIdx).toBeLessThan(cloneIdx);
  });

  // ── applyDefaultQueryFields: pre-filled fields are NOT overwritten ────────

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

  // ── getOptionalToolMetadata: returns null when loadToolContent throws ─────

  it('showAvailableTools: returns null metadata gracefully when loadToolContent fails', async () => {
    mocks.loadToolContent.mockRejectedValueOnce(
      new Error('metadata unavailable')
    );

    const { showAvailableTools } =
      await import('../../src/cli/tool-command.js');

    // Should NOT throw — getOptionalToolMetadata catches the error
    await expect(showAvailableTools()).resolves.toBeUndefined();

    const output = consoleSpy.mock.calls.flat().join('\n');
    // Still lists tools even without metadata (falls back to tool name)
    expect(output).toContain('localSearchCode');
  });

  // ── printToolResult: non-array content → empty textBlocks ────────────────

  it('printToolResult: uses structuredContent when result.content is undefined', async () => {
    mocks.localSearchCode.mockResolvedValueOnce({
      // intentionally omit `content` to hit the non-array branch
      structuredContent: { found: true },
    } as unknown as { content: []; structuredContent: { found: boolean } });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['localSearchCode', '{"path":".","pattern":"x"}'],
      options: { tool: 'localSearchCode' },
    });

    const out = consoleSpy.mock.calls.flat().join('\n');
    expect(out).toContain('"found": true');
  });

  it('printToolResult: content blocks with non-string text are filtered out', async () => {
    mocks.localSearchCode.mockResolvedValueOnce({
      content: [
        { type: 'image', data: 'base64...' }, // no .text field
        { type: 'text', text: '' }, // empty string filtered out
        { type: 'text', text: 'real output' }, // kept
      ],
    } as unknown as { content: Array<{ type: string; text?: string }> });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['localSearchCode', '{"path":".","pattern":"x"}'],
      options: { tool: 'localSearchCode' },
    });

    const out = consoleSpy.mock.calls.flat().join('\n');
    expect(out).toContain('real output');
    expect(out).not.toContain('base64');
  });

  // ── stripTsvRedundantResults: falsy payload passes through unchanged ──────

  it('stripTsvRedundantResults: null structuredContent in JSON mode emits null', async () => {
    mocks.localSearchCode.mockResolvedValueOnce({
      content: [{ type: 'text', text: 'txt' }],
      structuredContent: null, // null !== undefined so payload = null → !payload branch
    } as unknown as { content: Array<{ type: string; text: string }> });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['localSearchCode', '{"path":".","pattern":"x"}'],
      options: { tool: 'localSearchCode', json: true },
    });

    const out = consoleSpy.mock.calls.flat().join('\n');
    // stripTsvRedundantResults(!null) → return null → JSON.stringify(null) = 'null'
    expect(out.trim()).toBe('null');
  });

  it('stripTsvRedundantResults: non-object payload passes through unchanged', async () => {
    mocks.localSearchCode.mockResolvedValueOnce({
      content: [{ type: 'text', text: 'txt' }],
      structuredContent: 'just a string', // not an object
    } as unknown as { content: Array<{ type: string; text: string }> });

    const { toolCommand } = await import('../../src/cli/tool-command.js');

    await toolCommand.handler!({
      command: 'tool',
      args: ['localSearchCode', '{"path":".","pattern":"x"}'],
      options: { tool: 'localSearchCode', json: true },
    });

    const out = consoleSpy.mock.calls.flat().join('\n');
    expect(out.trim()).toBe('"just a string"');
  });

  // ── buildExampleValue: default fallback + boolean branch ─────────────────

  it('buildExampleValue: lspFindReferences example exercises boolean and unknown-name branches', async () => {
    const { toolCommand } = await import('../../src/cli/tool-command.js');

    // lspFindReferences has `includeDeclaration: z.boolean()` (required) and
    // `symbolName: z.string()` whose name is not in the switch — covers the
    // `return true` boolean branch and the `default: return name` fallback.
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

    // lspCallHierarchy has `direction: z.enum([...])` — exercises the enum type path
    await toolCommand.handler!({
      command: 'tool',
      args: ['lspCallHierarchy'],
      options: { tool: 'lspCallHierarchy', schema: true },
    });

    const out = consoleSpy.mock.calls.flat().join('\n');
    expect(out).toContain('direction');
    // The example should contain an enum value
    expect(out).toContain('incoming');
  });

  // ── executeToolCommand: toolName from args.options.tool (not positional) ──

  it('resolves tool name from --tool option when no positional arg given', async () => {
    const { executeToolCommand } =
      await import('../../src/cli/tool-command.js');

    const ok = await executeToolCommand({
      command: 'tool',
      args: [], // no positional arg
      options: {
        tool: 'localSearchCode',
        queries: '{"path":".","pattern":"x"}',
      },
    });

    expect(ok).toBe(true);
    expect(mocks.localSearchCode).toHaveBeenCalledTimes(1);
  });

  // ── executeToolCommand: neither args[0] nor options.tool → show list ──────

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
