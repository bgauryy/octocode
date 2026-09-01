import { describe, expect, it } from 'vitest';
import { ALL_TOOLS } from '../../src/tools/toolConfig.js';
import {
  GITHUB_SEARCH_TOOL_NAME,
  LOCAL_SEARCH_TOOL_NAME,
  STATIC_TOOL_NAMES,
} from '../../../octocode-tools-core/src/tools/toolNames.js';
import { LSP_GET_SEMANTICS_TOOL_NAME } from '../../../octocode-tools-core/src/tools/lsp/shared/semanticTypes.js';
import {
  DIRECT_TOOL_CATEGORIES,
  DIRECT_TOOL_DEFINITIONS,
  DIRECT_TOOL_DISCOVERY_DEFINITIONS,
  DirectToolInputError,
  buildDirectToolExampleQuery,
  executeDirectTool,
  findDirectToolDefinition,
  formatDirectToolMetadataSchemaText,
  formatDirectToolSchemaText,
  formatDirectToolValidationIssues,
  getDirectToolCategory,
  getDirectToolDescription,
  getDirectToolAutoFilledFields,
  getDirectToolDisplayFields,
  prepareDirectToolInput,
  prepareDirectToolInputFromJsonText,
  sortDirectToolNames,
} from '@octocodeai/octocode-tools-core';
import { z } from 'zod';

describe('directToolCatalog', () => {
  it('uses the MCP tool config as the direct tool name/order contract', () => {
    expect(DIRECT_TOOL_DEFINITIONS.map(tool => tool.name)).toEqual(
      ALL_TOOLS.map(tool => tool.name)
    );
  });

  it('discovers exactly the 10 canonical public tools in contract order', () => {
    const names = DIRECT_TOOL_DISCOVERY_DEFINITIONS.map(tool => tool.name);
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
        'ghListReleases',
        'ghSearchDiscussions',
      ])
    );
    for (const legacyName of [
      'ghSearchPullRequests',
      'ghSearchIssues',
      'ghSearchCommits',
      'ghListReleases',
      'ghSearchDiscussions',
    ]) {
      expect(findDirectToolDefinition(legacyName)).toBeUndefined();
    }
  });

  it('exposes query and bulk input schemas for every direct tool', () => {
    for (const tool of DIRECT_TOOL_DEFINITIONS) {
      expect(tool.schema).toBeDefined();
      expect(tool.inputSchema).toBeDefined();
      expect(findDirectToolDefinition(tool.name)?.name).toBe(tool.name);
    }
  });

  it('sorts direct tool names by explicit relevance within category', () => {
    expect(
      sortDirectToolNames([
        STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
        LOCAL_SEARCH_TOOL_NAME,
        GITHUB_SEARCH_TOOL_NAME,
        STATIC_TOOL_NAMES.PACKAGE_SEARCH,
      ])
    ).toEqual([
      GITHUB_SEARCH_TOOL_NAME,
      STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
      LOCAL_SEARCH_TOOL_NAME,
      STATIC_TOOL_NAMES.PACKAGE_SEARCH,
    ]);
  });

  it('exposes the canonical direct tool category order', () => {
    expect(DIRECT_TOOL_CATEGORIES).toEqual([
      'GitHub',
      'Local Code',
      'Package',
      'Other',
    ]);
  });

  it('exposes MCP-owned auto-filled field labels per tool category', () => {
    expect(getDirectToolAutoFilledFields(GITHUB_SEARCH_TOOL_NAME)).toEqual([
      'goal',
      'reasoning',
    ]);
    expect(
      getDirectToolAutoFilledFields(STATIC_TOOL_NAMES.PACKAGE_SEARCH)
    ).toEqual(['goal', 'reasoning']);
    expect(getDirectToolAutoFilledFields(LOCAL_SEARCH_TOOL_NAME)).toEqual([
      'goal',
      'reasoning',
    ]);
    expect(getDirectToolAutoFilledFields(LSP_GET_SEMANTICS_TOOL_NAME)).toEqual([
      'goal',
      'reasoning',
    ]);
  });

  it('categorizes known direct tool names and leaves unknown names as Other', () => {
    expect(getDirectToolCategory(GITHUB_SEARCH_TOOL_NAME)).toBe('GitHub');
    expect(getDirectToolCategory(LOCAL_SEARCH_TOOL_NAME)).toBe('Local Code');
    expect(getDirectToolCategory(LSP_GET_SEMANTICS_TOOL_NAME)).toBe(
      'Local Code'
    );
    expect(getDirectToolCategory(STATIC_TOOL_NAMES.PACKAGE_SEARCH)).toBe(
      'Package'
    );
    expect(getDirectToolCategory('customTool')).toBe('Other');
  });

  it('sorts unknown tools alphabetically within the Other category', () => {
    expect(sortDirectToolNames(['zCustom', 'aCustom'])).toEqual([
      'aCustom',
      'zCustom',
    ]);
  });

  it('formats schema and description fallbacks from the MCP catalog', () => {
    expect(formatDirectToolSchemaText('missingTool')).toBe('{}');
    expect(formatDirectToolMetadataSchemaText(undefined)).toBe('{}');
    expect(formatDirectToolMetadataSchemaText({ foo: 'bar' })).toContain(
      '"foo": "bar"'
    );
    expect(
      getDirectToolDescription(LOCAL_SEARCH_TOOL_NAME, {
        tools: {
          [LOCAL_SEARCH_TOOL_NAME]: {
            description: 'Local search metadata',
          },
        },
      })
    ).toBe(findDirectToolDefinition(LOCAL_SEARCH_TOOL_NAME)!.description);
    expect(getDirectToolDescription(LOCAL_SEARCH_TOOL_NAME, null)).toBe(
      findDirectToolDefinition(LOCAL_SEARCH_TOOL_NAME)!.description
    );
  });

  it('builds display fields and example queries from MCP-owned schemas', () => {
    const localFields = getDirectToolDisplayFields(LOCAL_SEARCH_TOOL_NAME);
    const localByName = Object.fromEntries(
      localFields.map(field => [field.name, field])
    );

    expect(localByName['id']).toBeUndefined();
    expect(localByName['operation']?.required).toBe(true);
    expect(localByName['searchText']?.required).toBe(false);
    expect(localByName['include']?.type).toBe('array<string>');
    expect(localByName['matchContentLength']?.required).toBe(false);
    expect(localByName['page']?.required).toBe(false);
    expect(getDirectToolDisplayFields('missingTool')).toEqual([]);

    expect(buildDirectToolExampleQuery(LOCAL_SEARCH_TOOL_NAME)).toEqual({
      path: '/ABS/repo/src',
      operation: 'text',
      searchText: 'buildDirectToolCommandPatterns',
      maxFiles: 20,
    });
    expect(
      buildDirectToolExampleQuery(STATIC_TOOL_NAMES.GITHUB_CLONE_REPO)
    ).toEqual({ owner: 'bgauryy', repo: 'octocode' });
    expect(buildDirectToolExampleQuery(LSP_GET_SEMANTICS_TOOL_NAME)).toEqual({
      uri: '/ABS/packages/octocode-tools-core/src/scheme/pagination.ts',
      type: 'documentSymbols',
    });
    expect(buildDirectToolExampleQuery('missingTool')).toEqual({});
  });

  it('prepares direct tool input from every CLI-supported JSON payload shape', () => {
    const query = {
      operation: 'text',
      path: '.',
      searchText: 'DIRECT_TOOL_CATEGORIES',
      regex: 'fixed',
      matchContentLength: 200,
      pageSize: 1,
      page: 1,
      maxMatchesPerFile: 1,
    };

    expect(
      prepareDirectToolInputFromJsonText(LOCAL_SEARCH_TOOL_NAME, undefined)
    ).toBeNull();

    const single = prepareDirectToolInput(LOCAL_SEARCH_TOOL_NAME, query, {
      sourceLabel: 'unit-test',
    });
    expect(single).toEqual(
      expect.objectContaining({
        queries: [
          expect.objectContaining({
            regex: 'fixed',
            goal: `Execute ${LOCAL_SEARCH_TOOL_NAME} via unit-test`,
            reasoning: 'Executed via unit-test tool command',
          }),
        ],
      })
    );
    expect(single.queries[0]).not.toHaveProperty('id');

    const bulk = prepareDirectToolInputFromJsonText(
      LOCAL_SEARCH_TOOL_NAME,
      JSON.stringify({
        queries: [query],
      }),
      { sourceLabel: 'unit-test' }
    );
    expect(bulk).toEqual(
      expect.objectContaining({
        queries: expect.any(Array),
      })
    );

    const arrayInput = prepareDirectToolInput(LOCAL_SEARCH_TOOL_NAME, [query]);
    expect(arrayInput.queries).toHaveLength(1);
  });

  it('preserves explicit query context while auto-filling missing GitHub context', () => {
    const prepared = prepareDirectToolInput(
      GITHUB_SEARCH_TOOL_NAME,
      {
        operation: 'code',
        goal: 'goal',
        reasoning: 'because',
        keywords: ['directToolCatalog'],
        pageSize: 1,
        page: 1,
      },
      { sourceLabel: 'unit-test' }
    );

    expect(prepared.queries[0]).toEqual(
      expect.objectContaining({
        goal: 'goal',
        reasoning: 'because',
      })
    );
    expect(prepared.queries[0]).not.toHaveProperty('id');

    const defaulted = prepareDirectToolInput(
      GITHUB_SEARCH_TOOL_NAME,
      {
        operation: 'code',
        keywords: ['directToolCatalog'],
        pageSize: 1,
        page: 1,
      },
      { sourceLabel: 'unit-test' }
    );

    expect(defaulted.queries[0]).toEqual(
      expect.objectContaining({
        goal: `Execute ${GITHUB_SEARCH_TOOL_NAME} via unit-test`,
      })
    );
  });

  it('warns on unknown fields but does NOT hard-fail — strips them and proceeds', () => {
    const warnings: Array<{ fields: string[]; index: number }> = [];

    const prepared = prepareDirectToolInput(
      LOCAL_SEARCH_TOOL_NAME,
      [
        {
          operation: 'text',
          searchText: 'a',
          path: '.',
          legacyLimit: 3,
          bogusKey: true,
        },
        { operation: 'text', searchText: 'b', path: '.', fixed_string: true },
      ],
      {
        sourceLabel: 'unit-test',
        onUnknownFields: (fields, index) => warnings.push({ fields, index }),
      }
    ) as { queries: Array<Record<string, unknown>> };

    // Agent is still warned about the stray keys...
    expect(warnings).toEqual([
      { fields: ['legacyLimit', 'bogusKey'], index: 0 },
      { fields: ['fixed_string'], index: 1 },
    ]);
    // ...but the call proceeds with the valid fields, stray keys stripped.
    expect(prepared.queries[0]).toMatchObject({ searchText: 'a', path: '.' });
    expect(prepared.queries[0]).not.toHaveProperty('bogusKey');
    expect(prepared.queries[0]).not.toHaveProperty('legacyLimit');
    expect(prepared.queries[1]).not.toHaveProperty('fixed_string');
  });

  it('preserves envelope-level fields alongside rebuilt queries', () => {
    const prepared = prepareDirectToolInput(
      LOCAL_SEARCH_TOOL_NAME,
      {
        queries: [{ operation: 'text', searchText: 'a', path: '.' }],
        responseCharLength: 500,
      },
      { sourceLabel: 'unit-test' }
    );

    expect(
      (prepared as { responseCharLength?: number }).responseCharLength
    ).toBe(500);
    expect(prepared.queries).toHaveLength(1);
  });

  it('preserves camelCase fields for direct tool input', () => {
    const prepared = prepareDirectToolInput(
      STATIC_TOOL_NAMES.GITHUB_CLONE_REPO,
      {
        owner: 'bgauryy',
        repo: 'octocode',
        branch: 'main',
        sparsePath: 'packages/octocode-mcp/src/tools',
      },
      { sourceLabel: 'unit-test' }
    );

    expect(prepared.queries[0]).toEqual(
      expect.objectContaining({
        owner: 'bgauryy',
        repo: 'octocode',
        branch: 'main',
        sparsePath: 'packages/octocode-mcp/src/tools',
      })
    );
  });

  it('reports direct tool input errors without CLI-owned parsing logic', () => {
    expect(() =>
      prepareDirectToolInputFromJsonText(LOCAL_SEARCH_TOOL_NAME, '{not-json')
    ).toThrow(new DirectToolInputError('Tool input must be valid JSON.'));
    expect(() => prepareDirectToolInput(LOCAL_SEARCH_TOOL_NAME, 42)).toThrow(
      'Tool input must be a JSON object'
    );
    expect(() => prepareDirectToolInput(LOCAL_SEARCH_TOOL_NAME, [])).toThrow(
      'At least one query is required'
    );
    expect(() => prepareDirectToolInput(LOCAL_SEARCH_TOOL_NAME, [42])).toThrow(
      'Tool input must be a JSON object or an array of objects.'
    );
    expect(() => prepareDirectToolInput('missingTool', {})).toThrow(
      'Unknown tool: missingTool'
    );

    const schemaResult = z.object({ name: z.string() }).safeParse({ name: 1 });
    expect(schemaResult.success).toBe(false);
    if (!schemaResult.success) {
      expect(formatDirectToolValidationIssues(schemaResult.error)).toEqual([
        expect.stringContaining('name:'),
      ]);
    }
  });

  it('validates direct tool input against the canonical MCP bulk schema', () => {
    expect(() =>
      prepareDirectToolInput(LOCAL_SEARCH_TOOL_NAME, {
        operation: 'structural',
        path: '.',
        pattern: 123,
        matchContentLength: 200,
        pageSize: 1,
        page: 1,
        maxMatchesPerFile: 1,
      })
    ).toThrow('Check the query fields.');
  });

  it('returns an MCP result envelope from the direct execution pipeline', async () => {
    const input = prepareDirectToolInput(LOCAL_SEARCH_TOOL_NAME, {
      operation: 'text',
      path: 'src/tools',
      searchText: 'ALL_TOOLS',
      regex: 'fixed',
      matchContentLength: 200,
      pageSize: 1,
      page: 1,
      maxMatchesPerFile: 1,
    });

    const result = await executeDirectTool(LOCAL_SEARCH_TOOL_NAME, input);

    expect(result.content?.length).toBeGreaterThan(0);
    expect(result.content?.[0]?.type).toBe('text');
  });

  it('rejects unknown and invalid direct execution requests before tool logic', async () => {
    // Unknown tool still throws (no catalog entry to build a structured result from).
    await expect(executeDirectTool('missingTool', {})).rejects.toThrow(
      'Unknown tool: missingTool'
    );
    // Invalid INPUT for a known tool now returns a structured error result
    // (not a throw) so every consumer — CLI and MCP — gets a uniform
    // CallToolResult instead of an exception. (input-parse moved inside the
    // execution try in directToolCatalog.)
    const invalid = await executeDirectTool(LOCAL_SEARCH_TOOL_NAME, {
      queries: [],
    });
    expect(invalid.isError).toBe(true);
  });
});
