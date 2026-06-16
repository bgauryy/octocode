import { describe, expect, it } from 'vitest';

import {
  LocalFindFilesBulkQuerySchema,
  LocalFindFilesQuerySchema,
} from '../../../src/tools/local_find_files/scheme.js';

describe('localFindFiles schema', () => {
  const baseQuery = { path: '/repo' };

  it('rejects an inverted depth range', () => {
    const result = LocalFindFilesQuerySchema.safeParse({
      ...baseQuery,
      minDepth: 4,
      maxDepth: 2,
    });

    expect(result.success).toBe(false);
    if (!result.success) {
      expect(
        result.error.issues.map(issue => issue.message).join('\n')
      ).toMatch(/minDepth must be less than or equal to maxDepth/);
    }
  });

  it('accepts an ordered depth range', () => {
    const result = LocalFindFilesQuerySchema.safeParse({
      ...baseQuery,
      minDepth: 1,
      maxDepth: 3,
    });

    expect(result.success).toBe(true);
  });

  it('strips the removed regexType compatibility field instead of rejecting', () => {
    const result = LocalFindFilesQuerySchema.safeParse({
      ...baseQuery,
      regex: '.*\\.ts$',
      regexType: 'posix-extended',
    });

    expect(result.success).toBe(true);
    if (result.success) {
      expect('regexType' in result.data).toBe(false);
    }
  });

  it('keeps bulk parsing relaxed so execution can report per-query errors', () => {
    const result = LocalFindFilesBulkQuerySchema.safeParse({
      queries: [
        { ...baseQuery, minDepth: 4, maxDepth: 2 },
        { ...baseQuery, names: ['*.ts'] },
      ],
    });

    expect(result.success).toBe(true);
  });

  it('strips the removed legacy name alias instead of rejecting', () => {
    const result = LocalFindFilesBulkQuerySchema.safeParse({
      queries: [{ ...baseQuery, name: '*.ts' }],
    });

    expect(result.success).toBe(true);
    if (result.success) {
      expect('name' in result.data.queries[0]).toBe(false);
    }
  });
});
