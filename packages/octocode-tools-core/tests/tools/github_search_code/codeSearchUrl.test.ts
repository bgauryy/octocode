import { describe, expect, it } from 'vitest';
import { mapCodeSearchProviderResult } from '../../../src/tools/providerMappers/codeSearch.js';

// The `verbose`/`url` branch keyed off a field that is not in the code-search
// schema, so it was dead. These tests pin that no `url` ever leaks onto a
// match, even when the provider item carries one and a bogus `verbose:true`
// is passed.
function providerData() {
  return {
    items: [
      {
        path: 'src/a.ts',
        matches: [
          {
            context: 'const x = 1;',
            positions: [[6, 7]] as [number, number][],
          },
        ],
        url: 'https://github.com/o/r/blob/main/src/a.ts',
        repository: { id: '1', name: 'o/r', url: 'https://github.com/o/r' },
      },
    ],
    totalCount: 1,
    pagination: { currentPage: 1, totalPages: 1, hasMore: false },
  };
}

describe('mapCodeSearchProviderResult — dead verbose/url branch removed', () => {
  it('never emits `url` on content matches, even with verbose:true', () => {
    const out = mapCodeSearchProviderResult(
      providerData() as never,
      {
        match: 'file',
        verbose: true,
      } as never
    );

    const matches = out.results[0].matches;
    expect(matches.length).toBeGreaterThan(0);
    // A snippet is still emitted with matchIndices, just no url.
    expect(matches[0].value).toBe('const x = 1;');
    for (const m of matches) expect('url' in m).toBe(false);
  });

  it('path-match rows likewise carry no url', () => {
    const out = mapCodeSearchProviderResult(
      providerData() as never,
      {
        match: 'path',
        verbose: true,
      } as never
    );

    for (const m of out.results[0].matches) expect('url' in m).toBe(false);
  });
});
