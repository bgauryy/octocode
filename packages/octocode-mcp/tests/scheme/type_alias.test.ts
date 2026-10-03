import { describe, it, expect } from 'vitest';

import { LocalSearchQuerySchema } from '@octocodeai/config/schema';
import {
  AstSearchQuerySchema,
  StructureSearchQuerySchema,
} from '@octocodeai/config/schema';

describe('canonical localSearch lexical contract', () => {
  const base = {
    mainGoal: 'test goal',
    reasoning: 'exercise lexical contract',
    searchText: 'foo',
    path: 'src',
  };

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

describe('structureSearch owns filesystem layout', () => {
  it('accepts file discovery and directory trees', () => {
    expect(
      StructureSearchQuerySchema.safeParse({
        mainGoal: 'test goal',
        reasoning: 'exercise filesystem contract',
        operation: 'files',
        path: 'src',
        names: ['*.ts'],
        entryType: 'f',
        sort: 'path',
      }).success
    ).toBe(true);
    expect(
      StructureSearchQuerySchema.safeParse({
        mainGoal: 'test goal',
        reasoning: 'exercise filesystem contract',
        operation: 'tree',
        path: 'src',
        maxDepth: 2,
      }).success
    ).toBe(true);
  });

  it('keeps unsupported aliases rejected', () => {
    expect(
      StructureSearchQuerySchema.safeParse({
        mainGoal: 'test goal',
        reasoning: 'exercise aliases',
        operation: 'files',
        path: 'src',
        entryType: 'file',
      }).success
    ).toBe(false);
    expect(
      StructureSearchQuerySchema.safeParse({
        mainGoal: 'test goal',
        reasoning: 'exercise aliases',
        operation: 'tree',
        path: 'src',
        sort: 'modified',
      }).success
    ).toBe(false);
  });
});

describe('astSearch carries no filesystem operations', () => {
  it('accepts syntaxTree and rejects retired files/tree shapes', () => {
    expect(
      AstSearchQuerySchema.safeParse({
        mainGoal: 'test goal',
        reasoning: 'exercise syntax contract',
        operation: 'syntaxTree',
        path: 'src/index.ts',
      }).success
    ).toBe(true);
    for (const retired of [
      { operation: 'files', path: 'src', names: ['*.ts'] },
      { operation: 'tree', treeKind: 'syntax', path: 'src/index.ts' },
      { operation: 'tree', path: 'src' },
    ]) {
      expect(
        AstSearchQuerySchema.safeParse({
          mainGoal: 'test goal',
          reasoning: 'exercise retired shapes',
          ...retired,
        }).success
      ).toBe(false);
    }
  });
});
