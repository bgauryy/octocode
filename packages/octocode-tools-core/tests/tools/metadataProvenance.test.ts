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

describe('metadata provenance — this repository owns schemas and descriptions', () => {
  it('owns one nonempty description for every public tool', () => {
    const names = Object.values(localCompleteMetadata.toolNames);
    expect(names).toHaveLength(17);
    expect(new Set(names).size).toBe(17);
    expect(Object.keys(localCompleteMetadata.tools)).toEqual(names);

    for (const name of names) {
      const spec = localCompleteMetadata.tools[name];
      expect(spec, name).toBeDefined();
      expect(spec?.description.trim().length, name).toBeGreaterThan(20);
      expect(DESCRIPTIONS[name], name).toBe(spec?.description);
    }
  });

  it('loads the repository-owned metadata object without a patch layer', async () => {
    const loaded = await loadToolContent();
    expect(loaded).toBe(localCompleteMetadata);
    expect(loaded.tools).toBe(localCompleteMetadata.tools);
  });

  it('owns one executable query schema for every public tool', () => {
    const names = Object.values(localCompleteMetadata.toolNames);
    expect(Object.keys(localSchemas.toolSchemas)).toEqual(names);
    for (const name of names) {
      expect(localSchemas.findToolSchema(name), name).toBeDefined();
    }
  });

  const cases: Array<[string, z.ZodTypeAny, z.ZodTypeAny]> = [
    [
      'localSearchCode',
      LocalRipgrepQuerySchema,
      localSchemas.RipgrepQuerySchema as z.ZodTypeAny,
    ],
    [
      'localViewStructure',
      LocalViewStructureQuerySchema as z.ZodTypeAny,
      localSchemas.ViewStructureQuerySchema as z.ZodTypeAny,
    ],
    [
      'localFindFiles',
      LocalFindFilesQuerySchema as z.ZodTypeAny,
      localSchemas.FindFilesQuerySchema as z.ZodTypeAny,
    ],
  ];

  it('serves every localAnalyzeGraph operation from its local schema', () => {
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
    it(`${tool}: runtime descriptions retain local source prose`, () => {
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
});
