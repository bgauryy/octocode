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

describe('metadata provenance — octocode-core owns the shared prompt', () => {
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
    expect(names).toHaveLength(12);
    expect(new Set(names).size).toBe(12);
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

  it('keeps every runtime field description identical to octocode-core', () => {
    const descriptionMap = (schema: z.ZodTypeAny): Map<string, string> => {
      const json = z.toJSONSchema(schema, { io: 'input' }) as {
        properties?: Record<string, { description?: string }>;
        oneOf?: Array<{
          properties?: Record<string, { description?: string }>;
        }>;
      };
      const descriptions = new Map<string, string>();
      for (const branch of json.oneOf ?? [json]) {
        for (const [field, property] of Object.entries(
          branch.properties ?? {}
        )) {
          if (property.description)
            descriptions.set(field, property.description);
        }
      }
      return descriptions;
    };

    const divergent: string[] = [];
    for (const definition of DIRECT_TOOL_DEFINITIONS) {
      const sourceSchema = localSchemas.findToolSchema(definition.name);
      if (!sourceSchema) continue;
      const sourceDescriptions = descriptionMap(sourceSchema);
      const runtimeDescriptions = descriptionMap(definition.schema);
      for (const [field, sourceDescription] of sourceDescriptions) {
        const runtimeDescription = runtimeDescriptions.get(field);
        if (
          runtimeDescription !== undefined &&
          runtimeDescription !== sourceDescription
        ) {
          divergent.push(`${definition.name}.${field}`);
        }
      }
    }

    expect(divergent).toEqual([]);
  });
});
