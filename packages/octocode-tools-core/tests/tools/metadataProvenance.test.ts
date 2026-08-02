import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import { completeMetadata } from '@octocodeai/octocode-core';
import * as coreSchemas from '@octocodeai/octocode-core/schemas';

import { DESCRIPTIONS } from '../../src/tools/toolMetadata/descriptions.js';
import { loadToolContent } from '../../src/tools/toolMetadata/state.js';

// Runtime query schemas (what MCP/CLI actually serve), per tool.
import { LocalRipgrepQuerySchema } from '../../src/tools/local_ripgrep/scheme.js';
import { LocalViewStructureQuerySchema } from '../../src/tools/local_view_structure/scheme.js';
import { LocalFindFilesQuerySchema } from '../../src/tools/local_find_files/scheme.js';
import { LocalFindDeadCodeQuerySchema } from '../../src/tools/local_dead_code/scheme.js';

// PROVENANCE CONTRACT: every tool description, field description, and the
// system prompt served by MCP/CLI must originate in
// @octocodeai/octocode-core (src/resources) — interface and brain packages
// may tighten bounds/defaults but must not author or fork prose. When this
// test fails, fix the prose IN CORE, then (if needed) drop the local
// .describe() so core's text flows through describeQuerySchema.
describe('metadata provenance — octocode-core is the only prose source', () => {
  it('served tool descriptions are byte-identical to core', () => {
    for (const [name, spec] of Object.entries(completeMetadata.tools)) {
      expect(DESCRIPTIONS[name], name).toBe(spec.description);
    }
  });

  it('loadToolContent returns core metadata verbatim (no patch layer)', async () => {
    const loaded = await loadToolContent();
    expect(loaded).toBe(completeMetadata);
    expect(loaded.systemPrompt).toBe(completeMetadata.systemPrompt);
  });

  const cases: Array<[string, z.ZodTypeAny, z.ZodTypeAny]> = [
    [
      'localSearchCode',
      LocalRipgrepQuerySchema,
      coreSchemas.RipgrepQuerySchema as z.ZodTypeAny,
    ],
    [
      'localViewStructure',
      LocalViewStructureQuerySchema as z.ZodTypeAny,
      coreSchemas.ViewStructureQuerySchema as z.ZodTypeAny,
    ],
    [
      'localFindFiles',
      LocalFindFilesQuerySchema as z.ZodTypeAny,
      coreSchemas.FindFilesQuerySchema as z.ZodTypeAny,
    ],
    [
      'localFindDeadCode',
      LocalFindDeadCodeQuerySchema as z.ZodTypeAny,
      coreSchemas.FindDeadCodeQuerySchema as z.ZodTypeAny,
    ],
  ];

  for (const [tool, runtimeSchema, coreSchema] of cases) {
    it(`${tool}: runtime field descriptions match core where core defines prose`, () => {
      const runtimeJson = z.toJSONSchema(runtimeSchema, { io: 'input' }) as {
        properties?: Record<string, { description?: string }>;
      };
      const coreJson = z.toJSONSchema(coreSchema, { io: 'input' }) as {
        properties?: Record<string, { description?: string }>;
      };
      const divergent: string[] = [];
      for (const [field, coreProp] of Object.entries(
        coreJson.properties ?? {}
      )) {
        const runtimeProp = runtimeJson.properties?.[field];
        if (!runtimeProp || !coreProp.description) continue;
        if (
          runtimeProp.description &&
          runtimeProp.description !== coreProp.description
        ) {
          divergent.push(field);
        }
      }
      expect(divergent, `${tool} forks core prose for: ${divergent}`).toEqual(
        []
      );
    });
  }
});
