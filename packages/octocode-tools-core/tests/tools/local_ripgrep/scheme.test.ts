import { describe, expect, it } from 'vitest';

import {
  LocalRipgrepBulkQuerySchema,
  LocalRipgrepQuerySchema,
} from '../../../src/tools/local_ripgrep/scheme.js';

describe('localSearchCode schema', () => {
  const baseQuery = { searchText: 'token', path: '/repo' };

  it('accepts the caseMode enum values', () => {
    for (const caseMode of ['smart', 'sensitive', 'insensitive'] as const) {
      expect(
        LocalRipgrepQuerySchema.safeParse({ ...baseQuery, caseMode }).success
      ).toBe(true);
    }
  });

  it('rejects an invalid caseMode value', () => {
    const result = LocalRipgrepQuerySchema.safeParse({
      ...baseQuery,
      caseMode: 'both',
    });
    expect(result.success).toBe(false);
  });

  it('accepts multiline:"dotall"', () => {
    const result = LocalRipgrepQuerySchema.safeParse({
      ...baseQuery,
      multiline: 'dotall',
    });

    expect(result.success).toBe(true);
  });

  it('accepts unique output when output is "matchOnly"', () => {
    const result = LocalRipgrepQuerySchema.safeParse({
      ...baseQuery,
      output: 'matchOnly',
      unique: 'count',
    });

    expect(result.success).toBe(true);
  });

  it('rejects removed semanticRanking input instead of accepting a no-op flag', () => {
    const result = LocalRipgrepQuerySchema.safeParse({
      ...baseQuery,
      semanticRanking: true,
    });

    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.error.issues.map(issue => issue.message).join('\n')).toMatch(
        /Unrecognized key.*semanticRanking|unrecognized.*semanticRanking/i
      );
    }
  });

  it('rejects unique output without output:"matchOnly"', () => {
    const result = LocalRipgrepQuerySchema.safeParse({
      ...baseQuery,
      unique: 'list',
    });

    expect(result.success).toBe(false);
    if (!result.success) {
      expect(
        result.error.issues.map(issue => issue.message).join('\n')
      ).toMatch(/unique requires output:"matchOnly"/);
    }
  });

  it('rejects a non-default unique enum in structural mode', () => {
    const result = LocalRipgrepQuerySchema.safeParse({
      path: '/repo',
      mode: 'structural',
      pattern: 'eval($X)',
      unique: 'count',
    });

    expect(result.success).toBe(false);
    if (!result.success) {
      expect(
        result.error.issues.map(issue => issue.message).join('\n')
      ).toMatch(/unique.*not valid with mode:"structural"/);
    }
  });

  it('keeps bulk parsing relaxed so execution can report per-query errors', () => {
    const result = LocalRipgrepBulkQuerySchema.safeParse({
      queries: [
        { ...baseQuery, unique: 'list' },
        { ...baseQuery, searchText: 'valid' },
      ],
    });

    expect(result.success).toBe(true);
  });
});
