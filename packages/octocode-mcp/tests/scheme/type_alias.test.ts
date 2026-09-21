import { describe, it, expect } from 'vitest';

import { LocalSearchQuerySchema } from '@octocodeai/config/schema';
import { AstSearchQuerySchema } from '@octocodeai/config/schema';

describe('canonical localSearch lexical contract', () => {
  const base = { reasoning: 'exercise lexical contract', searchText: 'foo', path: 'src' };

  it('accepts the explicit regex modes', () => {
    for (const regex of ['literal', 'rust', 'pcre2']) {
      expect(LocalSearchQuerySchema.safeParse({ ...base, regex }).success).toBe(
        true
      );
    }
  });

  it('rejects the removed operation and structural fields', () => {
    expect(
      LocalSearchQuerySchema.safeParse({ ...base, operation: 'text' }).success
    ).toBe(false);
    expect(
      LocalSearchQuerySchema.safeParse({ ...base, operation: 'files' }).success
    ).toBe(false);
  });

  it('rejects legacy aliases and structural result fields', () => {
    expect(
      LocalSearchQuerySchema.safeParse({ ...base, mode: 'discovery' }).success
    ).toBe(false);
    expect(
      LocalSearchQuerySchema.safeParse({ ...base, langType: 'ts' }).success
    ).toBe(true);
  });
});

describe('canonical astSearch filesystem contract', () => {
  it('accepts file discovery and filesystem tree operations', () => {
    expect(
      AstSearchQuerySchema.safeParse({
        reasoning: 'exercise filesystem contract',
        operation: 'files',
        path: 'src',
        names: ['*.ts'],
        entryType: 'f',
        sort: 'path',
      }).success
    ).toBe(true);
    expect(
      AstSearchQuerySchema.safeParse({
        reasoning: 'exercise filesystem contract',
        operation: 'tree',
        treeKind: 'filesystem',
        path: 'src',
        sort: 'time',
      }).success
    ).toBe(true);
  });

  it('keeps unsupported aliases rejected', () => {
    expect(
      AstSearchQuerySchema.safeParse({
        operation: 'files',
        path: 'src',
        entryType: 'file',
      }).success
    ).toBe(false);
    expect(
      AstSearchQuerySchema.safeParse({
        operation: 'tree',
        treeKind: 'filesystem',
        path: 'src',
        sort: 'modified',
      }).success
    ).toBe(false);
  });
});
