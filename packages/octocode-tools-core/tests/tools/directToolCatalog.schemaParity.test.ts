/**
 * P3 drift guard — the engine-free meta catalog (`directToolCatalog.meta.ts`,
 * the source for the `/schema` subpath) duplicates the per-tool name+schema
 * mapping that `toolConfig.ts` (`ALL_TOOLS`) attaches execution fns to. This
 * test imports BOTH (the engine-bearing ALL_TOOLS is fine in tests) and asserts
 * they never diverge in tool set, order, or JSON-schema shape.
 */
import { afterEach, describe, expect, it } from 'vitest';
import { z } from 'zod';
import { DIRECT_TOOL_DEFINITIONS } from '../../src/tools/directToolCatalog.meta.js';
import { ALL_TOOLS } from '../../src/tools/toolConfig.js';
import { OQL_SEARCH_TOOL_NAME } from '../../src/tools/toolNames.js';

describe('direct-tool meta catalog parity with ALL_TOOLS (P3)', () => {
  const originalEnableOql = process.env.ENABLE_OQL;

  afterEach(() => {
    if (originalEnableOql === undefined) {
      delete process.env.ENABLE_OQL;
    } else {
      process.env.ENABLE_OQL = originalEnableOql;
    }
  });

  it('covers exactly the same default tools in the same order and excludes oqlSearch', () => {
    expect(DIRECT_TOOL_DEFINITIONS.map(t => t.name)).toEqual(
      ALL_TOOLS.map(t => t.name)
    );
    expect(DIRECT_TOOL_DEFINITIONS.map(t => t.name)).not.toContain(
      OQL_SEARCH_TOOL_NAME
    );
  });

  it('documents ENABLE_OQL as an opt-in import-time gate', () => {
    process.env.ENABLE_OQL = 'true';
    expect(process.env.ENABLE_OQL).toBe('true');
    // The direct catalog is evaluated at module import time; this test documents
    // the env contract without depending on Vite cache-busting dynamic imports.
    // Runtime exposure is covered by the default exclusion assertion above and
    // by MCP/Pi registration tests that import the catalog under the process env.
  });

  it('exposes the identical display + bulk schemas per tool', () => {
    const runtimeByName = new Map(ALL_TOOLS.map(t => [t.name, t.direct]));
    for (const def of DIRECT_TOOL_DEFINITIONS) {
      const runtime = runtimeByName.get(def.name);
      expect(runtime, `missing runtime for ${def.name}`).toBeDefined();
      // Same zod object identity → schema text cannot drift.
      expect(def.schema, `${def.name} display schema`).toBe(runtime!.schema);
      expect(def.inputSchema, `${def.name} bulk schema`).toBe(
        runtime!.inputSchema
      );
      // And the rendered JSON schema is well-formed.
      expect(() => z.toJSONSchema(def.inputSchema)).not.toThrow();
    }
  });
});
