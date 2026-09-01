import { describe, expect, it } from 'vitest';
import { z } from 'zod';

import { localCompleteMetadata } from '../../src/toolContract/metadata.js';
import * as localSchemas from '../../src/toolContract/schemas.js';
import { DESCRIPTIONS } from '../../src/tools/toolMetadata/descriptions.js';
import { loadToolContent } from '../../src/tools/toolMetadata/state.js';
import { LocalRipgrepQuerySchema } from '../../src/tools/local_ripgrep/scheme.js';
import { LocalViewStructureQuerySchema } from '../../src/tools/local_view_structure/scheme.js';
import { LocalFindFilesQuerySchema } from '../../src/tools/local_find_files/scheme.js';
import { LocalAnalyzeGraphQuerySchema } from '../../src/tools/local_analyze_graph/scheme.js';
import {
  DIRECT_TOOL_DEFINITIONS,
  DIRECT_TOOL_DISCOVERY_DEFINITIONS,
} from '../../src/tools/directToolCatalog/toolCatalogDefinitions.js';

describe('metadata provenance — tools-core owns executable contracts', () => {
  it('serves the current MCP output guidance without a local patch', () => {
    expect(localCompleteMetadata.systemPrompt).not.toContain(
      'restores full YAML text'
    );
    expect(localCompleteMetadata.systemPrompt).toContain(
      'MCP returns complete YAML text in content[].text'
    );
  });

  it('provides one locally owned nonempty description for every public tool', () => {
    const names = DIRECT_TOOL_DISCOVERY_DEFINITIONS.map(
      definition => definition.name
    );
    expect(names).toHaveLength(10);
    expect(new Set(names).size).toBe(10);
    expect(Object.keys(DESCRIPTIONS)).toEqual(names);

    for (const name of names) {
      expect(DESCRIPTIONS[name]?.trim().length, name).toBeGreaterThan(20);
      expect(
        DIRECT_TOOL_DISCOVERY_DEFINITIONS.find(
          definition => definition.name === name
        )?.description,
        name
      ).toBe(DESCRIPTIONS[name]);
    }
  });

  it('loads the shared metadata object without a patch layer', async () => {
    const loaded = await loadToolContent();
    expect(loaded).toBe(localCompleteMetadata);
    expect(loaded.tools).toBe(localCompleteMetadata.tools);
  });

  it('provides one executable query schema for every public tool', () => {
    for (const definition of DIRECT_TOOL_DISCOVERY_DEFINITIONS) {
      expect(definition.schema, definition.name).toBeDefined();
      expect(definition.inputSchema, definition.name).toBeDefined();
    }
  });

  const cases: Array<[string, z.ZodTypeAny, z.ZodTypeAny]> = [
    [
      'local.text',
      LocalRipgrepQuerySchema,
      localSchemas.RipgrepQuerySchema as z.ZodTypeAny,
    ],
    [
      'local.tree',
      LocalViewStructureQuerySchema as z.ZodTypeAny,
      localSchemas.ViewStructureQuerySchema as z.ZodTypeAny,
    ],
    [
      'local.files',
      LocalFindFilesQuerySchema as z.ZodTypeAny,
      localSchemas.FindFilesQuerySchema as z.ZodTypeAny,
    ],
  ];

  it('serves every localAnalyzeGraph operation from its shared schema', () => {
    for (const query of [
      { operation: 'deadCode', path: '.' },
      { operation: 'cycles', path: '.' },
      { operation: 'dependencies', path: '.', file: 'src/index.ts' },
      { operation: 'dependents', path: '.', file: 'src/index.ts' },
      {
        operation: 'path',
        path: '.',
        file: 'src/index.ts',
        target: 'src/direct.ts',
      },
      { operation: 'reachability', path: '.' },
    ]) {
      expect(LocalAnalyzeGraphQuerySchema.safeParse(query).success).toBe(true);
    }
  });

  for (const [tool, runtimeSchema, sourceSchema] of cases) {
    it(`${tool}: runtime descriptions retain shared source prose`, () => {
      const runtimeJson = z.toJSONSchema(runtimeSchema, { io: 'input' }) as {
        properties?: Record<string, { description?: string }>;
      };
      const sourceJson = z.toJSONSchema(sourceSchema, { io: 'input' }) as {
        properties?: Record<string, { description?: string }>;
      };
      const divergent: string[] = [];
      for (const [field, sourceProp] of Object.entries(
        sourceJson.properties ?? {}
      )) {
        const runtimeProp = runtimeJson.properties?.[field];
        if (!runtimeProp || !sourceProp.description) continue;
        if (
          runtimeProp.description &&
          runtimeProp.description !== sourceProp.description
        ) {
          divergent.push(field);
        }
      }
      expect(divergent, `${tool} forks local prose for: ${divergent}`).toEqual(
        []
      );
    });
  }

  it('does not expose a second schema registry beside the direct catalog', () => {
    expect('toolSchemas' in localSchemas).toBe(false);
    expect('findToolSchema' in localSchemas).toBe(false);
    expect(DIRECT_TOOL_DEFINITIONS).toHaveLength(10);
  });
});
