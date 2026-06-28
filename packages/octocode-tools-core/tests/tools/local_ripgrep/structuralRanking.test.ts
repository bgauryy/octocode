import { describe, expect, it } from 'vitest';

import { buildSearchResult } from '../../../src/tools/local_ripgrep/ripgrepResultBuilder.js';
import type { RipgrepQuery } from '../../../src/tools/local_ripgrep/scheme.js';
import type { LocalSearchCodeFile } from '@octocodeai/octocode-core/types';

type FilesShape = { files: Array<{ path: string }> };

// Crafted so path-ascending ([a,m,z]) differs from matchCount-descending
// ([z,m,a]) — making the chosen ordering unambiguous.
const files = (): LocalSearchCodeFile[] =>
  [
    { path: 'src/z.ts', matchCount: 9, matches: [{ line: 1, column: 0, value: 'foo()' }] },
    {
      path: 'src/a.ts',
      matchCount: 1,
      matches: [{ line: 1, column: 0, value: 'export function foo() {}' }],
    },
    { path: 'src/m.ts', matchCount: 5, matches: [{ line: 1, column: 0, value: 'foo()' }] },
  ] as unknown as LocalSearchCodeFile[];

const baseQuery = {
  path: '.',
  pattern: 'foo($$$A)',
  langType: 'ts',
} as unknown as RipgrepQuery;

describe('structural search ordering', () => {
  it('defaults to deterministic path order (skips relevance scoring) for structural mode', async () => {
    const r = (await buildSearchResult(files(), baseQuery, 'structural', [], {
      totalStructuralMatches: 15,
    })) as unknown as FilesShape;
    // AST matches are already precise; order by path (source/position-stable),
    // NOT by the relevance scorer (which would surface matchCount/declaration).
    expect(r.files.map(f => f.path)).toEqual(['src/a.ts', 'src/m.ts', 'src/z.ts']);
  });

  it('still honors an explicit sort on structural mode', async () => {
    const q = { ...baseQuery, sort: 'matchCount' } as unknown as RipgrepQuery;
    const r = (await buildSearchResult(files(), q, 'structural', [], {
      totalStructuralMatches: 15,
    })) as unknown as FilesShape;
    expect(r.files.map(f => f.path)).toEqual(['src/z.ts', 'src/m.ts', 'src/a.ts']);
  });
});
