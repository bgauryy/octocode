import { describe, it, expect } from 'vitest';
import { applyExactMatchRanking } from '../../src/tools/github_search_code/finalizer.js';

const g = (id: string, matches: Array<{ path: string; value?: string }>) => ({
  id,
  owner: id.split('/')[0]!,
  repo: id.split('/')[1]!,
  matches,
});

describe('applyExactMatchRanking', () => {
  it('floats a whole-word match above a fuzzy substring match within a group', () => {
    const groups = [
      g('o/r', [
        { path: 'src/util.ts', value: 'const x = createStoreImpl()' },
        { path: 'src/store.ts', value: 'export function createStore() {}' },
      ]),
    ];
    const ranked = applyExactMatchRanking(groups, ['createStore']);
    expect(ranked[0]!.matches[0]!.path).toBe('src/store.ts');
  });

  it('floats an exact-filename match to the top', () => {
    const groups = [
      g('o/r', [
        { path: 'src/helpers.ts', value: 'import { useStore }' },
        { path: 'src/useStore.ts', value: 'whatever' },
      ]),
    ];
    const ranked = applyExactMatchRanking(groups, ['useStore']);
    expect(ranked[0]!.matches[0]!.path).toBe('src/useStore.ts');
  });

  it('ranks a group with an exact hit above a larger fuzzy-only group', () => {
    const fuzzy = g('a/big', [
      { path: 'a.ts', value: 'createStoreImpl' },
      { path: 'b.ts', value: 'CreateStoreFactory' },
      { path: 'c.ts', value: 'xCreateStore' },
    ]);
    const exact = g('b/small', [{ path: 'store.ts', value: 'createStore()' }]);
    const ranked = applyExactMatchRanking([fuzzy, exact], ['createStore']);
    expect(ranked[0]!.id).toBe('b/small');
  });

  it('falls back to match-count then id ordering when no exact hits', () => {
    const few = g('a/few', [{ path: 'a.ts', value: 'zzz' }]);
    const many = g('b/many', [
      { path: 'b.ts', value: 'zzz' },
      { path: 'c.ts', value: 'zzz' },
    ]);
    const ranked = applyExactMatchRanking([few, many], ['nomatch']);
    expect(ranked[0]!.id).toBe('b/many');
  });

  it('is a no-op-safe when keywords is empty', () => {
    const groups = [g('o/r', [{ path: 'a.ts', value: 'x' }])];
    const ranked = applyExactMatchRanking(groups, []);
    expect(ranked).toHaveLength(1);
    expect(ranked[0]!.matches).toHaveLength(1);
  });
});
