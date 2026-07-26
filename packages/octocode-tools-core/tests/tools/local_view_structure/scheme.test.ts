import { describe, expect, it } from 'vitest';

import {
  LocalViewStructureBulkQuerySchema,
  LocalViewStructureQuerySchema,
} from '../../../src/tools/local_view_structure/scheme.js';

describe('localViewStructure schema', () => {
  const baseQuery = { path: '/repo' };

  it('accepts the entryType enum values (f / d)', () => {
    for (const entryType of ['f', 'd'] as const) {
      expect(
        LocalViewStructureQuerySchema.safeParse({ ...baseQuery, entryType })
          .success
      ).toBe(true);
    }
  });

  it('rejects an invalid entryType value', () => {
    expect(
      LocalViewStructureQuerySchema.safeParse({ ...baseQuery, entryType: 'x' })
        .success
    ).toBe(false);
  });

  it('accepts excludeDir so callers can override default pruning', () => {
    expect(
      LocalViewStructureQuerySchema.safeParse({
        ...baseQuery,
        recursive: true,
        excludeDir: [],
      }).success
    ).toBe(true);
  });

  it('documents the effective maxDepth default for each mode', () => {
    const description =
      LocalViewStructureQuerySchema.shape.maxDepth.description ?? '';

    // Overrides the (inaccurate) inherited core text; states the real defaults.
    expect(description).toMatch(/immediate children/i);
    expect(description).toMatch(/or 5 if recursive:true/i);
    expect(description).toMatch(/recursive:true is not required/i);
  });

  it('keeps bulk parsing relaxed so execution can report per-query errors', () => {
    const result = LocalViewStructureBulkQuerySchema.safeParse({
      queries: [
        { ...baseQuery, entryType: 'f' },
        { ...baseQuery, path: '/repo/src' },
      ],
    });

    expect(result.success).toBe(true);
  });
});
