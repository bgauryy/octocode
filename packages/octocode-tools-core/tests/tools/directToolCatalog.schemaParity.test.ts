/**
 * P3 drift guard — the engine-free meta catalog (`directToolCatalog.meta.ts`,
 * the source for the `/schema` subpath) and `toolConfig.ts` (`ALL_TOOLS`) both
 * derive names and schemas from one engine-free specification. This test
 * imports both (the engine-bearing ALL_TOOLS is fine in tests) and asserts
 * they never diverge in tool set, order, or JSON-schema shape.
 * Runtime and discovery expose the same canonical public tools.
 */
import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import {
  DIRECT_TOOL_DISCOVERY_DEFINITIONS,
  DIRECT_TOOL_DEFINITIONS,
  findDirectToolDefinition,
} from '../../src/tools/directToolCatalog/toolCatalogDefinitions.js';
import { DIRECT_TOOL_SPECIFICATIONS } from '../../src/tools/directToolCatalog/toolSpecifications.js';
import { ALL_TOOLS } from '../../src/tools/toolConfig.js';

describe('direct-tool meta catalog parity with ALL_TOOLS (P3)', () => {
  it('feeds discovery and runtime from one engine-free name/schema specification', () => {
    const publicBarrelSource = readFileSync(
      new URL('../../src/index.ts', import.meta.url),
      'utf8'
    );
    const toolConfigSource = readFileSync(
      new URL('../../src/tools/toolConfig.ts', import.meta.url),
      'utf8'
    );
    const definitionsSource = readFileSync(
      new URL(
        '../../src/tools/directToolCatalog/toolCatalogDefinitions.ts',
        import.meta.url
      ),
      'utf8'
    );

    expect(toolConfigSource).toContain('DIRECT_TOOL_SPECIFICATIONS');
    expect(toolConfigSource).toContain(
      "from './directToolCatalog/toolSpecifications.js'"
    );
    expect(toolConfigSource).not.toContain("from './toolSchemaImports.js'");
    expect(definitionsSource).toContain('DIRECT_TOOL_SPECIFICATIONS');
    expect(definitionsSource).toContain("from './toolSpecifications.js'");
    expect(definitionsSource).not.toContain("from '../toolSchemaImports.js'");
    for (const legacyToolModule of [
      'tools/github_search_code/',
      'tools/github_search_repos/',
      'tools/github_view_repo_structure/',
    ]) {
      expect(publicBarrelSource).not.toContain(legacyToolModule);
    }
    expect(DIRECT_TOOL_SPECIFICATIONS).toHaveLength(10);
    expect(DIRECT_TOOL_SPECIFICATIONS.map(tool => tool.name)).not.toEqual(
      expect.arrayContaining(['ghListReleases', 'ghSearchDiscussions'])
    );
    expect(
      DIRECT_TOOL_SPECIFICATIONS.every(
        specification =>
          specification.title.trim().length > 0 &&
          specification.description.trim().length > 0
      )
    ).toBe(true);
  });

  it('covers exactly the same enabled tools in the same order', () => {
    expect(DIRECT_TOOL_DEFINITIONS.map(t => t.name)).toEqual(
      ALL_TOOLS.map(t => t.name)
    );
  });

  it('exposes the identical display + bulk schemas per tool', () => {
    const runtimeByName = new Map(ALL_TOOLS.map(t => [t.name, t.direct]));
    const specificationByName = new Map(
      DIRECT_TOOL_SPECIFICATIONS.map(specification => [
        specification.name,
        specification,
      ])
    );
    for (const def of DIRECT_TOOL_DEFINITIONS) {
      const runtime = runtimeByName.get(def.name);
      expect(runtime, `missing runtime for ${def.name}`).toBeDefined();
      // Same zod object identity → schema text cannot drift.
      expect(def.schema, `${def.name} display schema`).toBe(runtime!.schema);
      expect(def.inputSchema, `${def.name} bulk schema`).toBe(
        runtime!.inputSchema
      );
      // And the rendered JSON schema is well-formed.
      expect(() => {
        const jsonSchema = z.toJSONSchema(def.inputSchema, { io: 'input' });
        z.fromJSONSchema(jsonSchema);
      }).not.toThrow();
    }
    for (const def of DIRECT_TOOL_DISCOVERY_DEFINITIONS) {
      const specification = specificationByName.get(def.name);
      expect(
        specification,
        `missing specification for ${def.name}`
      ).toBeDefined();
      expect(def.schema).toBe(specification!.schema);
      expect(def.inputSchema).toBe(specification!.inputSchema);
      expect(def.title).toBe(specification!.title);
      expect(def.description).toBe(specification!.description);
    }
  });

  it.each([
    {
      tool: 'npmSearch',
      valid: [{ packageName: 'zod' }, { keywords: ['schema'] }],
      invalid: [{}, { packageName: 'zod', keywords: ['schema'] }],
    },
    {
      tool: 'ghGetFileContent',
      valid: [
        { owner: 'o', repo: 'r', path: 'p' },
        { owner: 'o', repo: 'r', path: 'p', startLine: 1, endLine: 2 },
        { owner: 'o', repo: 'r', path: 'p', matchString: 'needle' },
      ],
      invalid: [
        { owner: 'o', repo: 'r', path: 'p', startLine: 1 },
        {
          owner: 'o',
          repo: 'r',
          path: 'p',
          fullContent: true,
          matchString: 'needle',
        },
      ],
    },
    {
      tool: 'localGetFileContent',
      valid: [
        { path: '/tmp/p' },
        { path: '/tmp/p', startLine: 1, endLine: 2 },
        { path: '/tmp/p', matchString: 'needle' },
      ],
      invalid: [
        { path: '/tmp/p', endLine: 2 },
        { path: '/tmp/p', fullContent: true, matchString: 'needle' },
      ],
    },
    {
      tool: 'ghSearchHistory',
      valid: [
        { operation: 'pullRequests', keywords: ['schema'] },
        { operation: 'issues', owner: 'o', repo: 'r' },
        { operation: 'commits', owner: 'o', repo: 'r', branch: 'main' },
      ],
      invalid: [
        { operation: 'issues', owner: 'o', repo: 'r', number: 1 },
        {
          operation: 'commits',
          owner: 'o',
          repo: 'r',
          includeDiff: true,
        },
      ],
    },
    {
      tool: 'ghGetHistoryItem',
      valid: [
        { operation: 'pullRequest', owner: 'o', repo: 'r', number: 1 },
        { operation: 'issue', owner: 'o', repo: 'r', number: 1 },
        { operation: 'commit', owner: 'o', repo: 'r', ref: 'abc' },
        {
          operation: 'compare',
          owner: 'o',
          repo: 'r',
          base: 'main',
          head: 'feature',
        },
      ],
      invalid: [
        { operation: 'pullRequest', owner: 'o', repo: 'r' },
        { operation: 'issue', owner: 'o', repo: 'r', issueNumber: 1 },
        {
          operation: 'commit',
          owner: 'o',
          repo: 'r',
          ref: 'abc',
          branch: 'main',
        },
        {
          operation: 'compare',
          owner: 'o',
          repo: 'r',
          base: 'main',
        },
      ],
    },
    {
      tool: 'lspGetSemantics',
      valid: [
        { uri: '/tmp/p.ts', type: 'documentSymbols' },
        {
          uri: '/tmp/p.ts',
          type: 'definition',
          symbolName: 'run',
          lineHint: 1,
        },
        { type: 'workspaceSymbol', symbolName: 'run' },
      ],
      invalid: [
        { type: 'definition' },
        { uri: '/tmp/p.ts', type: 'definition', symbolName: 'run' },
        { type: 'workspaceSymbol' },
      ],
    },
  ] as const)(
    '$tool generated JSON Schema preserves executable conditional validation',
    ({ tool, valid, invalid }) => {
      const definition = findDirectToolDefinition(tool);
      expect(definition).toBeDefined();
      const executable = definition!.inputSchema;
      const generated = z.fromJSONSchema(
        z.toJSONSchema(executable, { io: 'input' })
      );

      for (const query of valid) {
        const input = { queries: [query] };
        expect(executable.safeParse(input).success).toBe(true);
        expect(generated.safeParse(input).success).toBe(true);
      }
      for (const query of invalid) {
        const input = { queries: [query] };
        expect(executable.safeParse(input).success).toBe(false);
        expect(generated.safeParse(input).success).toBe(false);
      }
    }
  );
});

describe('default read-only tool availability', () => {
  it('publishes one schema per canonical capability', () => {
    expect(DIRECT_TOOL_DEFINITIONS).toHaveLength(10);
    expect(DIRECT_TOOL_DISCOVERY_DEFINITIONS).toHaveLength(10);
    expect(DIRECT_TOOL_DISCOVERY_DEFINITIONS).not.toBe(DIRECT_TOOL_DEFINITIONS);
  });

  it.each([
    'local.text',
    'local.files',
    'local.tree',
    'github.code',
    'github.repositories',
    'github.tree',
    'ghListReleases',
    'ghSearchDiscussions',
  ] as const)('%s compatibility schema is removed', name => {
    expect(ALL_TOOLS.some(tool => tool.name === name)).toBe(false);
    expect(findDirectToolDefinition(name)).toBeUndefined();
  });
});
