import { describe, expect, it } from 'vitest';
import { ALL_TOOLS } from '../../src/tools/toolConfig.js';
import { STATIC_TOOL_NAMES } from '../../../octocode-tools-core/src/tools/toolNames.js';
import { LSP_GET_SEMANTICS_TOOL_NAME } from '../../../octocode-tools-core/src/tools/lsp/shared/semanticTypes.js';
import {
  DIRECT_TOOL_CATEGORIES,
  DIRECT_TOOL_DEFINITIONS,
  DirectToolInputError,
  buildDirectToolExampleQuery,
  executeDirectTool,
  findDirectToolDefinition,
  formatDirectToolMetadataSchemaText,
  formatDirectToolSchemaText,
  formatDirectToolValidationIssues,
  getDirectToolCategory,
  getDirectToolDescription,
  formatDirectToolOutputSchemaText,
  getDirectToolAutoFilledFields,
  getDirectToolDisplayFields,
  getDirectToolOutputFields,
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
        STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
        STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
        STATIC_TOOL_NAMES.LOCAL_RIPGREP,
        STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
        STATIC_TOOL_NAMES.PACKAGE_SEARCH,
      ])
    ).toEqual([
      STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
      STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
      STATIC_TOOL_NAMES.LOCAL_RIPGREP,
      STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
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
    expect(
      getDirectToolAutoFilledFields(STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE)
    ).toEqual(['id', 'mainResearchGoal', 'researchGoal', 'reasoning']);
    expect(
      getDirectToolAutoFilledFields(STATIC_TOOL_NAMES.PACKAGE_SEARCH)
    ).toEqual(['id', 'mainResearchGoal', 'researchGoal', 'reasoning']);
    expect(
      getDirectToolAutoFilledFields(STATIC_TOOL_NAMES.LOCAL_RIPGREP)
    ).toEqual(['id', 'researchGoal', 'reasoning']);
    expect(getDirectToolAutoFilledFields(LSP_GET_SEMANTICS_TOOL_NAME)).toEqual([
      'id',
      'researchGoal',
      'reasoning',
    ]);
  });

  it('exposes the canonical direct tool output shape for CLI help', () => {
    const outputFields = getDirectToolOutputFields();

    expect(outputFields).toEqual([
      { name: 'content', type: 'Array<{ type: string; text: string }>' },
      { name: 'structuredContent', type: 'object', optional: true },
      { name: 'isError', type: 'boolean', optional: true },
    ]);

    expect(outputFields[0]).toBeDefined();
    outputFields[0]!.name = 'mutated';

    expect(getDirectToolOutputFields()[0]?.name).toBe('content');
    expect(JSON.parse(formatDirectToolOutputSchemaText())).toEqual({
      content: 'Array<{ type: string; text: string }>',
      structuredContent: 'object (optional)',
      isError: 'boolean (optional)',
    });
  });

  it('categorizes known direct tool names and leaves unknown names as Other', () => {
    expect(getDirectToolCategory(STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE)).toBe(
      'GitHub'
    );
    expect(getDirectToolCategory(STATIC_TOOL_NAMES.LOCAL_RIPGREP)).toBe(
      'Local Code'
    );
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
      getDirectToolDescription(STATIC_TOOL_NAMES.LOCAL_RIPGREP, {
        tools: {
          [STATIC_TOOL_NAMES.LOCAL_RIPGREP]: {
            description: 'Local search metadata',
          },
        },
      })
    ).toBe('Local search metadata');
    expect(
      getDirectToolDescription(STATIC_TOOL_NAMES.LOCAL_RIPGREP, null)
    ).toBe(STATIC_TOOL_NAMES.LOCAL_RIPGREP);
  });

  it('builds display fields and example queries from MCP-owned schemas', () => {
    const localFields = getDirectToolDisplayFields(
      STATIC_TOOL_NAMES.LOCAL_RIPGREP
    );
    const localByName = Object.fromEntries(
      localFields.map(field => [field.name, field])
    );

    expect(localByName['id']).toBeUndefined();
    expect(localByName['keywords']?.required).toBe(false);
    expect(localByName['include']?.type).toBe('array<string>');
    expect(localByName['matchContentLength']?.required).toBe(false);
    expect(localByName['page']?.required).toBe(false);
    expect(getDirectToolDisplayFields('missingTool')).toEqual([]);

    expect(
      buildDirectToolExampleQuery(STATIC_TOOL_NAMES.LOCAL_RIPGREP)
    ).toEqual({
      path: '.',
      keywords: 'runCLI',
    });
    expect(
      buildDirectToolExampleQuery(STATIC_TOOL_NAMES.GITHUB_CLONE_REPO)
    ).toEqual({ owner: 'bgauryy', repo: 'octocode' });
    expect(buildDirectToolExampleQuery(LSP_GET_SEMANTICS_TOOL_NAME)).toEqual(
      expect.objectContaining({
        uri: '/path/to/file.ts',
        type: 'definition',
        symbolName: 'myFunction',
        lineHint: 42,
      })
    );
    expect(buildDirectToolExampleQuery('missingTool')).toEqual({});
  });

  it('prepares direct tool input from every CLI-supported JSON payload shape', () => {
    const query = {
      path: '.',
      keywords: 'DIRECT_TOOL_CATEGORIES',
      fixedString: true,
      matchContentLength: 200,
      itemsPerPage: 1,
      page: 1,
      maxMatchesPerFile: 1,
    };

    expect(
      prepareDirectToolInputFromJsonText(
        STATIC_TOOL_NAMES.LOCAL_RIPGREP,
        undefined
      )
    ).toBeNull();

    const single = prepareDirectToolInput(
      STATIC_TOOL_NAMES.LOCAL_RIPGREP,
      query,
      { sourceLabel: 'unit-test' }
    );
    expect(single).toEqual(
      expect.objectContaining({
        queries: [
          expect.objectContaining({
            id: `${STATIC_TOOL_NAMES.LOCAL_RIPGREP}-1`,
            fixedString: true,
            researchGoal: `Execute ${STATIC_TOOL_NAMES.LOCAL_RIPGREP} via unit-test`,
            reasoning: 'Executed via unit-test tool command',
          }),
        ],
      })
    );

    const bulk = prepareDirectToolInputFromJsonText(
      STATIC_TOOL_NAMES.LOCAL_RIPGREP,
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

    const arrayInput = prepareDirectToolInput(STATIC_TOOL_NAMES.LOCAL_RIPGREP, [
      query,
    ]);
    expect(arrayInput.queries).toHaveLength(1);
  });

  it('preserves explicit query context while auto-filling missing GitHub context', () => {
    const prepared = prepareDirectToolInput(
      STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
      {
        id: 'custom-id',
        mainResearchGoal: 'main',
        researchGoal: 'goal',
        reasoning: 'because',
        keywords: ['directToolCatalog'],
        limit: 1,
        page: 1,
      },
      { sourceLabel: 'unit-test' }
    );

    expect(prepared.queries[0]).toEqual(
      expect.objectContaining({
        id: 'custom-id',
        mainResearchGoal: 'main',
        researchGoal: 'goal',
        reasoning: 'because',
      })
    );

    const defaulted = prepareDirectToolInput(
      STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
      {
        keywords: ['directToolCatalog'],
        limit: 1,
        page: 1,
      },
      { sourceLabel: 'unit-test' }
    );

    expect(defaulted.queries[0]).toEqual(
      expect.objectContaining({
        mainResearchGoal: `Execute ${STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE} via unit-test`,
      })
    );
  });

  it('warns on unknown fields but does NOT hard-fail — strips them and proceeds', () => {
    const warnings: Array<{ fields: string[]; index: number }> = [];

    const prepared = prepareDirectToolInput(
      STATIC_TOOL_NAMES.LOCAL_RIPGREP,
      [
        { keywords: 'a', path: '.', limit: 3, bogusKey: true },
        { keywords: 'b', path: '.', fixed_string: true },
      ],
      {
        sourceLabel: 'unit-test',
        onUnknownFields: (fields, index) => warnings.push({ fields, index }),
      }
    ) as { queries: Array<Record<string, unknown>> };

    // Agent is still warned about the stray keys...
    expect(warnings).toEqual([
      { fields: ['limit', 'bogusKey'], index: 0 },
      { fields: ['fixed_string'], index: 1 },
    ]);
    // ...but the call proceeds with the valid fields, stray keys stripped.
    expect(prepared.queries[0]).toMatchObject({ keywords: 'a', path: '.' });
    expect(prepared.queries[0]).not.toHaveProperty('bogusKey');
    expect(prepared.queries[0]).not.toHaveProperty('limit');
    expect(prepared.queries[1]).not.toHaveProperty('fixed_string');
  });

  it('preserves envelope-level fields alongside rebuilt queries', () => {
    const prepared = prepareDirectToolInput(
      STATIC_TOOL_NAMES.LOCAL_RIPGREP,
      {
        queries: [{ keywords: 'a', path: '.' }],
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
      prepareDirectToolInputFromJsonText(
        STATIC_TOOL_NAMES.LOCAL_RIPGREP,
        '{not-json'
      )
    ).toThrow(new DirectToolInputError('Tool input must be valid JSON.'));
    expect(() =>
      prepareDirectToolInput(STATIC_TOOL_NAMES.LOCAL_RIPGREP, 42)
    ).toThrow('Tool input must be a JSON object');
    expect(() =>
      prepareDirectToolInput(STATIC_TOOL_NAMES.LOCAL_RIPGREP, [])
    ).toThrow('At least one query is required');
    expect(() =>
      prepareDirectToolInput(STATIC_TOOL_NAMES.LOCAL_RIPGREP, [42])
    ).toThrow('Tool input must be a JSON object or an array of objects.');
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
      prepareDirectToolInput(STATIC_TOOL_NAMES.LOCAL_RIPGREP, {
        path: '.',
        pattern: 123,
        matchContentLength: 200,
        itemsPerPage: 1,
        page: 1,
        maxMatchesPerFile: 1,
      })
    ).toThrow('Tool input does not match the expected schema.');
  });

  it('returns an MCP result envelope from the direct execution pipeline', async () => {
    const input = prepareDirectToolInput(STATIC_TOOL_NAMES.LOCAL_RIPGREP, {
      path: 'src/tools/directToolCatalog.ts',
      keywords: 'DIRECT_TOOL_CATEGORIES',
      fixedString: true,
      matchContentLength: 200,
      itemsPerPage: 1,
      page: 1,
      maxMatchesPerFile: 1,
    });

    const result = await executeDirectTool(
      STATIC_TOOL_NAMES.LOCAL_RIPGREP,
      input
    );

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
    const invalid = await executeDirectTool(STATIC_TOOL_NAMES.LOCAL_RIPGREP, {
      queries: [],
    });
    expect(invalid.isError).toBe(true);
  });
});
