import { describe, it, expect, vi, beforeEach } from 'vitest';
import { buildSearchResult } from '../../../octocode-tools-core/src/tools/local_ripgrep/ripgrepResultBuilder.js';
import { promises as fs } from 'fs';

vi.mock('fs', () => ({
  promises: {
    stat: vi.fn(),
  },
}));

describe('ripgrepResultBuilder - _getStructuredResultSizeHints (lines 171-179)', () => {
  const mockFsStat = vi.mocked(fs.stat);

  beforeEach(() => {
    vi.clearAllMocks();
    mockFsStat.mockResolvedValue({
      mtime: new Date('2024-01-01'),
    } as any);
  });

  const makeFiles = (count: number, matchesPerFile: number = 5) =>
    Array.from({ length: count }, (_, i) => ({
      path: `/test/file${i}.ts`,
      matchCount: matchesPerFile,
      matches: Array.from({ length: matchesPerFile }, (_, j) => ({
        line: j + 1,
        column: 1,
        value: 'match',
        location: {
          byteOffset: 0,
          byteLength: 5,
          charOffset: 0,
          charLength: 5,
          line: j + 1,
          column: 1,
        },
      })),
    }));

  it('emits a single combined large-result-set recovery line when all 3 levers are open', async () => {
    const files = makeFiles(25, 5);
    const query = {
      path: '/test',
      keywords: 'ab',
      researchGoal: 'test',
      reasoning: 'test',
    } as any;

    const result = await buildSearchResult(files, query, 'rg', []);

    expect(result.hints).toBeDefined();
    const hintsStr = result.hints!.join('\n');
    expect(hintsStr).toContain('Large result set');
    expect(hintsStr).toContain('add langType or include');
    expect(hintsStr).toContain('add excludeDir');
    expect(hintsStr).toContain('lengthen pattern');
  });

  it('A1: itemsPerPage pages FILES (top-level), maxMatchesPerFile caps matches/file', async () => {
    const files = makeFiles(5, 4);
    const query = {
      path: '/test',
      keywords: 'match',
      itemsPerPage: 2,
      maxMatchesPerFile: 1,
      page: 1,
      researchGoal: 'test',
      reasoning: 'test',
    } as any;

    const result = await buildSearchResult(files, query, 'rg', []);

    expect(result.files).toHaveLength(2);
    expect(result.pagination?.filesPerPage).toBe(2);
    expect(result.pagination?.totalFiles).toBe(5);
    expect(result.pagination?.hasMore).toBe(true);
    expect(result.files![0]!.matches).toHaveLength(1);
    expect((result.hints ?? []).join('\n')).toContain('Next: page=2');
  });

  it('emits an out-of-range hint when page exceeds total pages (E2)', async () => {
    const files = makeFiles(3, 2);
    const query = {
      path: '/test',
      keywords: 'match',
      page: 999,
      researchGoal: 'test',
      reasoning: 'test',
    } as any;

    const result = await buildSearchResult(files, query, 'rg', []);
    const hintsStr = (result.hints ?? []).join('\n');
    expect(hintsStr).toMatch(/outside available range|page 999 is/i);
  });

  it('suggests langType/include when neither is set', async () => {
    const files = makeFiles(25, 5);
    const query = {
      path: '/test',
      keywords: 'x',
      researchGoal: 'test',
      reasoning: 'test',
    } as any;

    const result = await buildSearchResult(files, query, 'rg', []);

    expect(result.hints?.some(h => h.includes('add langType or include'))).toBe(
      true
    );
  });

  it('suggests excludeDir when none is set', async () => {
    const files = makeFiles(25, 5);
    const query = {
      path: '/test',
      keywords: 'ab',
      researchGoal: 'test',
      reasoning: 'test',
    } as any;

    const result = await buildSearchResult(files, query, 'rg', []);

    expect(result.hints?.some(h => h.includes('add excludeDir'))).toBe(true);
  });

  it('suggests lengthening the pattern when it is short', async () => {
    const files = makeFiles(25, 5);
    const query = {
      path: '/test',
      keywords: 'hi',
      researchGoal: 'test',
      reasoning: 'test',
    } as any;

    const result = await buildSearchResult(files, query, 'rg', []);

    expect(result.hints?.some(h => h.includes('lengthen pattern'))).toBe(true);
  });

  it('does NOT suggest langType/include when query.langType is already set', async () => {
    const files = makeFiles(25, 5);
    const query = {
      path: '/test',
      keywords: 'ab',
      langType: 'ts',
      researchGoal: 'test',
      reasoning: 'test',
    } as any;

    const result = await buildSearchResult(files, query, 'rg', []);

    expect(result.hints?.some(h => h.includes('add langType or include'))).toBe(
      false
    );
  });

  it('should NOT add excludeDir hint when query.excludeDir has items', async () => {
    const files = makeFiles(25, 5);
    const query = {
      path: '/test',
      keywords: 'ab',
      excludeDir: ['node_modules'],
      researchGoal: 'test',
      reasoning: 'test',
    } as any;

    const result = await buildSearchResult(files, query, 'rg', []);

    expect(result.hints?.some(h => h.includes('add excludeDir'))).toBe(false);
  });

  it('should NOT add pattern hint when query.keywords.length >= 5', async () => {
    const files = makeFiles(25, 5);
    const query = {
      path: '/test',
      keywords: 'longer',
      researchGoal: 'test',
      reasoning: 'test',
    } as any;

    const result = await buildSearchResult(files, query, 'rg', []);

    expect(result.hints?.some(h => h.includes('lengthen pattern'))).toBe(false);
  });

  it('exposes totalFilesFound in pagination when results are capped by maxFiles', async () => {
    const files = makeFiles(20, 2);
    const query = {
      path: '/test',
      keywords: 'match',
      maxFiles: 5,
      researchGoal: 'test',
      reasoning: 'test',
    } as any;

    const result = await buildSearchResult(files, query, 'rg', []);

    expect(result.pagination?.totalFiles).toBe(5);
    expect((result.pagination as any).totalFilesFound).toBe(20);
  });

  it('does NOT add totalFilesFound when results are not limited', async () => {
    const files = makeFiles(3, 2);
    const query = {
      path: '/test',
      keywords: 'match',
      researchGoal: 'test',
      reasoning: 'test',
    } as any;

    const result = await buildSearchResult(files, query, 'rg', []);

    expect((result.pagination as any).totalFilesFound).toBeUndefined();
  });

  it('emits enumeration hint when results span multiple pages', async () => {
    const files = makeFiles(5, 2);
    const query = {
      path: '/test',
      keywords: 'match',
      itemsPerPage: 2,
      page: 1,
      researchGoal: 'test',
      reasoning: 'test',
    } as any;

    const result = await buildSearchResult(files, query, 'rg', []);

    const hintsStr = (result.hints ?? []).join('\n');
    expect(hintsStr).toContain('page=2');
  });

  it('does NOT emit enumeration hint on the last page', async () => {
    const files = makeFiles(4, 2);
    const query = {
      path: '/test',
      keywords: 'match',
      itemsPerPage: 2,
      page: 2, // last page
      researchGoal: 'test',
      reasoning: 'test',
    } as any;

    const result = await buildSearchResult(files, query, 'rg', []);

    const hintsStr = (result.hints ?? []).join('\n');
    expect(hintsStr).not.toContain('paginated — use page=2');
  });

  it('orders files by match count before path for relevance-first search results', async () => {
    const files = [
      ...makeFiles(1, 1).map(file => ({ ...file, path: '/test/a.ts' })),
      ...makeFiles(1, 3).map(file => ({ ...file, path: '/test/b.ts' })),
      ...makeFiles(1, 2).map(file => ({ ...file, path: '/test/c.ts' })),
    ];
    const query = {
      path: '/test',
      keywords: 'match',
      researchGoal: 'test',
      reasoning: 'test',
    } as any;

    const result = await buildSearchResult(files, query, 'rg', []);

    expect(result.files?.map(file => file.path)).toEqual([
      '/test/b.ts',
      '/test/c.ts',
      '/test/a.ts',
    ]);
  });
});
