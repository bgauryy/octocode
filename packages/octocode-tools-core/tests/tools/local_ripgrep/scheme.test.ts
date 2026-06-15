import { describe, expect, it } from 'vitest';

import {
  LocalRipgrepBulkQuerySchema,
  LocalRipgrepQuerySchema,
} from '../../../src/tools/local_ripgrep/scheme.js';

describe('localSearchCode schema', () => {
  const baseQuery = { keywords: 'token', path: '/repo' };

  it('rejects contradictory case flags', () => {
    const result = LocalRipgrepQuerySchema.safeParse({
      ...baseQuery,
      caseSensitive: true,
      caseInsensitive: true,
    });

    expect(result.success).toBe(false);
    if (!result.success) {
      expect(
        result.error.issues.map(issue => issue.message).join('\n')
      ).toMatch(/caseSensitive and caseInsensitive are mutually exclusive/);
    }
  });

  it('rejects multilineDotall without multiline', () => {
    const result = LocalRipgrepQuerySchema.safeParse({
      ...baseQuery,
      multilineDotall: true,
    });

    expect(result.success).toBe(false);
    if (!result.success) {
      expect(
        result.error.issues.map(issue => issue.message).join('\n')
      ).toMatch(/multilineDotall requires multiline=true/);
    }
  });

  it('accepts multilineDotall when multiline is enabled', () => {
    const result = LocalRipgrepQuerySchema.safeParse({
      ...baseQuery,
      multiline: true,
      multilineDotall: true,
    });

    expect(result.success).toBe(true);
  });

  it('keeps bulk parsing relaxed so execution can report per-query errors', () => {
    const result = LocalRipgrepBulkQuerySchema.safeParse({
      queries: [
        { ...baseQuery, caseSensitive: true, caseInsensitive: true },
        { ...baseQuery, keywords: 'valid' },
      ],
    });

    expect(result.success).toBe(true);
  });
});
