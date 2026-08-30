/**
 * P3 drift guard — the engine-free meta catalog (`directToolCatalog.meta.ts`,
 * the source for the `/schema` subpath) duplicates the per-tool name+schema
 * mapping that `toolConfig.ts` (`ALL_TOOLS`) attaches execution fns to. This
 * test imports BOTH (the engine-bearing ALL_TOOLS is fine in tests) and
 * asserts they never diverge in tool set, order, or JSON-schema shape.
 * Runtime definitions contain enabled tools; discovery additionally exposes
 * disabled opt-in schemas with their enabling environment variable.
 */
import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import {
  DIRECT_TOOL_DISCOVERY_DEFINITIONS,
  DIRECT_TOOL_DEFINITIONS,
  findDirectToolDefinition,
} from '../../src/tools/directToolCatalog.meta.js';
import { ALL_TOOLS } from '../../src/tools/toolConfig.js';

describe('direct-tool meta catalog parity with ALL_TOOLS (P3)', () => {
  it('covers exactly the same enabled tools in the same order', () => {
    expect(DIRECT_TOOL_DEFINITIONS.map(t => t.name)).toEqual(
      ALL_TOOLS.map(t => t.name)
    );
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

describe('default read-only tool availability', () => {
  it('keeps every public schema discoverable while optional tools stay disabled', () => {
    expect(DIRECT_TOOL_DEFINITIONS).toHaveLength(15);
    expect(DIRECT_TOOL_DISCOVERY_DEFINITIONS).toHaveLength(17);
    expect(DIRECT_TOOL_DISCOVERY_DEFINITIONS).not.toBe(DIRECT_TOOL_DEFINITIONS);
  });

  it.each(['ghListReleases', 'ghSearchDiscussions'] as const)(
    '%s resolves by name but is disabled without its feature flag',
    name => {
      expect(DIRECT_TOOL_DEFINITIONS.some(t => t.name === name)).toBe(false);
      expect(ALL_TOOLS.some(t => t.name === name)).toBe(false);
      const found = findDirectToolDefinition(name);
      expect(found).toBeDefined();
      expect(found?.disabled?.envVar).toMatch(
        /^ENABLE_(RELEASES|DISCUSSIONS)$/
      );
      expect(() => z.toJSONSchema(found!.inputSchema)).not.toThrow();
    }
  );
});
