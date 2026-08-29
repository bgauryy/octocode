/**
 * P3 drift guard — the engine-free meta catalog (`directToolCatalog.meta.ts`,
 * the source for the `/schema` subpath) duplicates the per-tool name+schema
 * mapping that `toolConfig.ts` (`ALL_TOOLS`) attaches execution fns to. This
 * test imports BOTH (the engine-bearing ALL_TOOLS is fine in tests) and
 * asserts they never diverge in tool set, order, or JSON-schema shape.
 * DIRECT_TOOL_DEFINITIONS stays enabled-tools-only, matching ALL_TOOLS exactly.
 * DIRECT_TOOL_DISCOVERY_DEFINITIONS additionally includes opt-in schemas with
 * explicit disabled metadata so local CLI discovery and exact-name lookup agree.
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
  it('covers exactly the same default tools in the same order', () => {
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

describe('opt-in tool discoverability without the env var', () => {
  it('keeps every public schema in the discovery catalog with explicit availability', () => {
    expect(DIRECT_TOOL_DISCOVERY_DEFINITIONS).toHaveLength(17);
    expect(
      DIRECT_TOOL_DISCOVERY_DEFINITIONS.filter(tool => tool.disabled).map(
        tool => [tool.name, tool.disabled?.envVar]
      )
    ).toEqual([
      ['ghListReleases', 'ENABLE_RELEASES'],
      ['ghSearchDiscussions', 'ENABLE_DISCUSSIONS'],
    ]);
  });

  it.each(['ghListReleases', 'ghSearchDiscussions'] as const)(
    '%s resolves by name with a disabled marker when its env var is unset',
    name => {
      // Test env has neither ENABLE_RELEASES nor ENABLE_DISCUSSIONS set, so
      // both are absent from the executable registry...
      expect(DIRECT_TOOL_DEFINITIONS.some(t => t.name === name)).toBe(false);
      expect(ALL_TOOLS.some(t => t.name === name)).toBe(false);
      // ...but still resolve by exact name and discovery, with enough schema for
      // `--scheme` to render, and a marker `execute.ts` uses to fail clearly
      // instead of falling through to a generic "Unknown tool".
      const found = findDirectToolDefinition(name);
      expect(found).toBeDefined();
      expect(found?.disabled?.envVar).toMatch(/^ENABLE_/);
      expect(() => z.toJSONSchema(found!.inputSchema)).not.toThrow();
    }
  );
});
