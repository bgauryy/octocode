import { describe, expect, it } from 'vitest';
import {
  AstSearchQuerySchema,
  DIRECT_TOOL_SPECIFICATIONS,
  localCompleteMetadata,
  PUBLIC_TOOL_DESCRIPTIONS,
} from '@octocodeai/octocode-core/schema';
import { SYSTEM_PROMPT } from '@octocodeai/octocode-core/mcp';
import * as publicSchemas from '../../src/schema.js';
import {
  DIRECT_TOOL_DEFINITIONS,
  DIRECT_TOOL_DISCOVERY_DEFINITIONS,
} from '@octocodeai/octocode-core/schema';

describe('metadata provenance — core owns executable contracts', () => {
  it('serves the core MCP workflow and evidence boundaries without a second prompt', () => {
    expect(localCompleteMetadata.systemPrompt).toBe(SYSTEM_PROMPT);
    expect(SYSTEM_PROMPT).not.toMatch(
      /localAnalyzeGraph|lspGetSemantics|localSearchCode|localFindFiles|localViewStructure/
    );
    expect(SYSTEM_PROMPT).toContain('content[].text');
    expect(SYSTEM_PROMPT).toContain('structuredContent');
    expect(SYSTEM_PROMPT).toContain('Whole-response pagination');
    expect(SYSTEM_PROMPT).toContain('responseSnapshot');
    expect(SYSTEM_PROMPT).toContain('next.* queries unchanged');
  });

  it('serves the core description and executable validators for every public tool', () => {
    const names = DIRECT_TOOL_DISCOVERY_DEFINITIONS.map(
      definition => definition.name
    );
    expect(names).toHaveLength(11);
    expect(new Set(names).size).toBe(11);
    expect(Object.keys(PUBLIC_TOOL_DESCRIPTIONS)).toEqual(names);
    expect(
      DIRECT_TOOL_SPECIFICATIONS.map(specification => specification.name)
    ).toEqual(names);
    for (const specification of DIRECT_TOOL_SPECIFICATIONS) {
      const runtime = DIRECT_TOOL_DISCOVERY_DEFINITIONS.find(
        definition => definition.name === specification.name
      );
      expect(runtime?.description, specification.name).toBe(
        specification.description
      );
      expect(runtime?.schema, specification.name).toBe(specification.schema);
      expect(runtime?.inputSchema, specification.name).toBe(
        specification.inputSchema
      );
      expect(
        specification.description.trim().length,
        specification.name
      ).toBeGreaterThan(20);
    }
  });

  it.each([
    { operation: 'files', path: '/repo' },
    { operation: 'tree', path: '/repo' },
    { operation: 'tree', treeKind: 'syntax', path: '/repo/index.ts' },
    { operation: 'symbols', path: '/repo' },
    {
      operation: 'match',
      path: '/repo/index.ts',
      pattern: 'console.log($ARG)',
    },
    { operation: 'topology', analysis: 'deadCode', path: '/repo' },
    { operation: 'topology', analysis: 'cycles', path: '/repo' },
    {
      operation: 'topology',
      analysis: 'dependencies',
      path: '/repo',
      file: 'src/index.ts',
    },
    {
      operation: 'topology',
      analysis: 'dependents',
      path: '/repo',
      file: 'src/index.ts',
    },
    {
      operation: 'topology',
      analysis: 'path',
      path: '/repo',
      file: 'src/index.ts',
      target: 'src/direct.ts',
    },
    { operation: 'topology', analysis: 'reachability', path: '/repo' },
  ])('serves the public astSearch operation directly: %j', query => {
    const definition = DIRECT_TOOL_DISCOVERY_DEFINITIONS.find(
      tool => tool.name === 'astSearch'
    );
    expect(definition?.schema).toBe(AstSearchQuerySchema);
    expect(AstSearchQuerySchema.safeParse(query).success).toBe(true);
  });

  it('does not expose a second schema registry beside the direct catalog', () => {
    expect('toolSchemas' in publicSchemas).toBe(false);
    expect('findToolSchema' in publicSchemas).toBe(false);
    expect(DIRECT_TOOL_DEFINITIONS).toHaveLength(11);
  });
});
