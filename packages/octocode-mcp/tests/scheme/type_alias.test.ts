import { describe, it, expect } from 'vitest';

import { LocalSearchQuerySchema } from '../../../octocode-tools-core/src/tools/local_search/scheme.js';

function rejectedKeys(error: { issues: unknown[] }): string[] {
  const visit = (issue: unknown): string[] => {
    if (!issue || typeof issue !== 'object') return [];
    const value = issue as {
      code?: string;
      keys?: string[];
      errors?: unknown[][];
    };
    if (value.code === 'unrecognized_keys') return value.keys ?? [];
    return (value.errors ?? []).flatMap(branch => branch.flatMap(visit));
  };
  return error.issues.flatMap(visit);
}

describe('localSearch text operation aliases', () => {
  const base = { operation: 'text', searchText: 'foo', path: 'src' };

  it('accepts langType', () => {
    const result = LocalSearchQuerySchema.safeParse({
      ...base,
      langType: 'ts',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect((result.data as { langType?: string }).langType).toBe('ts');
    }
  });

  it('rejects the legacy `type` key on the public schema (strict, not honored)', () => {
    const result = LocalSearchQuerySchema.safeParse({ ...base, type: 'ts' });
    expect(result.success).toBe(false);
    if (!result.success) {
      const keys = rejectedKeys(result.error);
      expect(keys).toContain('type');
    }
  });

  it('uses resultView and rejects the legacy mode field', () => {
    expect(
      LocalSearchQuerySchema.safeParse({ ...base, resultView: 'discovery' })
        .success
    ).toBe(true);
    expect(
      LocalSearchQuerySchema.safeParse({ ...base, mode: 'discovery' }).success
    ).toBe(false);
  });
});

describe('localSearch files operation aliases', () => {
  const base = { operation: 'files', path: 'src' };

  it('accepts entryType', () => {
    const result = LocalSearchQuerySchema.safeParse({
      ...base,
      entryType: 'd',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect((result.data as { entryType?: string }).entryType).toBe('d');
    }
  });

  it('accepts only supported entryType and sort values', () => {
    expect(
      LocalSearchQuerySchema.safeParse({
        ...base,
        entryType: 'f',
        sort: 'modified',
      }).success
    ).toBe(true);
    expect(
      LocalSearchQuerySchema.safeParse({ ...base, entryType: 'file' }).success
    ).toBe(false);
    expect(
      LocalSearchQuerySchema.safeParse({ ...base, sort: 'time' }).success
    ).toBe(false);
  });

  it('rejects the legacy `type` key instead of aliasing it', () => {
    const result = LocalSearchQuerySchema.safeParse({ ...base, type: 'f' });
    expect(result.success).toBe(false);
    if (!result.success) {
      const keys = rejectedKeys(result.error);
      expect(keys).toContain('type');
    }
  });
});

describe('localSearch tree sort values', () => {
  const base = { operation: 'tree', path: 'src' };

  it('accepts only supported sort fields', () => {
    expect(
      LocalSearchQuerySchema.safeParse({ ...base, sort: 'time' }).success
    ).toBe(true);
    expect(
      LocalSearchQuerySchema.safeParse({ ...base, sort: 'modified' }).success
    ).toBe(false);
  });
});
