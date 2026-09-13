import { describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  searchContentRipgrep: vi.fn(),
}));

vi.mock('../../../src/tools/local_ripgrep/searchContentRipgrep.js', () => ({
  searchContentRipgrep: mocks.searchContentRipgrep,
}));

const { runTypedLexicalSearch } =
  await import('../../../src/tools/local_search/typedLexicalService.js');

describe('runTypedLexicalSearch', () => {
  it('adapts the public lexical query once before using the legacy runner', async () => {
    mocks.searchContentRipgrep.mockResolvedValue({ files: [] });

    await runTypedLexicalSearch({
      path: '/repo',
      searchText: 'symbol',
      regex: 'literal',
      resultView: 'files',
      pageSize: 25,
      reverse: true,
    } as never);

    expect(mocks.searchContentRipgrep).toHaveBeenCalledWith(
      expect.objectContaining({
        path: '/repo',
        searchText: 'symbol',
        regex: 'fixed',
        mode: 'paginated',
        output: 'files',
        itemsPerPage: 25,
        sortReverse: true,
      })
    );
  });
});
