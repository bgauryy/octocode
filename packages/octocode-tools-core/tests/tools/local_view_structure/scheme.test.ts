import { describe, expect, it } from 'vitest';

import {
  LocalViewStructureBulkQuerySchema,
  LocalViewStructureQuerySchema,
} from '../../../src/tools/local_view_structure/scheme.js';

describe('localViewStructure schema', () => {
  const baseQuery = { path: '/repo' };

  it('rejects contradictory entry filters', () => {
    const result = LocalViewStructureQuerySchema.safeParse({
      ...baseQuery,
      filesOnly: true,
      directoriesOnly: true,
    });

    expect(result.success).toBe(false);
    if (!result.success) {
      expect(
        result.error.issues.map(issue => issue.message).join('\n')
      ).toMatch(/filesOnly and directoriesOnly are mutually exclusive/);
    }
  });

  it('accepts filesOnly alone', () => {
    const result = LocalViewStructureQuerySchema.safeParse({
      ...baseQuery,
      filesOnly: true,
    });

    expect(result.success).toBe(true);
  });

  it('keeps bulk parsing relaxed so execution can report per-query errors', () => {
    const result = LocalViewStructureBulkQuerySchema.safeParse({
      queries: [
        { ...baseQuery, filesOnly: true, directoriesOnly: true },
        { ...baseQuery, path: '/repo/src' },
      ],
    });

    expect(result.success).toBe(true);
  });
});
