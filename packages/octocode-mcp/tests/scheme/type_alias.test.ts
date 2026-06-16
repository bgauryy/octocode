import { describe, it, expect } from 'vitest';

import { LocalRipgrepQuerySchema } from '../../../octocode-tools-core/src/tools/local_ripgrep/scheme.js';
import { LocalFindFilesQuerySchema } from '../../../octocode-tools-core/src/tools/local_find_files/scheme.js';
import { LocalViewStructureQuerySchema } from '../../../octocode-tools-core/src/tools/local_view_structure/scheme.js';

describe('localSearchCode langType (one public field)', () => {
  const base = { keywords: 'foo', path: 'src' };

  it('accepts langType', () => {
    const result = LocalRipgrepQuerySchema.safeParse({
      ...base,
      langType: 'ts',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect((result.data as { langType?: string }).langType).toBe('ts');
    }
  });

  it('does not expose `type` on the public schema (stripped, not honored)', () => {
    const result = LocalRipgrepQuerySchema.safeParse({ ...base, type: 'ts' });
    expect(result.success).toBe(true);
    if (result.success) {
      expect((result.data as { type?: string }).type).toBeUndefined();
    }
  });

  it('accepts only supported workflow modes', () => {
    expect(
      LocalRipgrepQuerySchema.safeParse({ ...base, mode: 'discovery' }).success
    ).toBe(true);
    expect(
      LocalRipgrepQuerySchema.safeParse({ ...base, mode: 'compact' }).success
    ).toBe(false);
  });
});

describe('localFindFiles entryType (one public field)', () => {
  const base = { path: 'src' };

  it('accepts entryType', () => {
    const result = LocalFindFilesQuerySchema.safeParse({
      ...base,
      entryType: 'd',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect((result.data as { entryType?: string }).entryType).toBe('d');
    }
  });

  it('accepts only supported entryType and sortBy values', () => {
    expect(
      LocalFindFilesQuerySchema.safeParse({
        ...base,
        entryType: 'f',
        sortBy: 'modified',
      }).success
    ).toBe(true);
    expect(
      LocalFindFilesQuerySchema.safeParse({ ...base, entryType: 'file' })
        .success
    ).toBe(false);
    expect(
      LocalFindFilesQuerySchema.safeParse({ ...base, sortBy: 'time' }).success
    ).toBe(false);
  });

  it('does not expose `type` on the public schema (stripped, not honored)', () => {
    const result = LocalFindFilesQuerySchema.safeParse({ ...base, type: 'f' });
    expect(result.success).toBe(true);
    if (result.success) {
      expect((result.data as { type?: string }).type).toBeUndefined();
    }
  });
});

describe('localViewStructure sortBy values', () => {
  const base = { path: 'src' };

  it('accepts only supported sort fields', () => {
    expect(
      LocalViewStructureQuerySchema.safeParse({ ...base, sortBy: 'time' })
        .success
    ).toBe(true);
    expect(
      LocalViewStructureQuerySchema.safeParse({ ...base, sortBy: 'modified' })
        .success
    ).toBe(false);
  });
});
