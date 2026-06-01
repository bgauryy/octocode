/**
 * Branch coverage tests for ripgrepResultBuilder.ts
 * Targeting lines 171-179: _getStructuredResultSizeHints with large result set
 * - totalMatches > 100 || files.length > 20
 * - !query.type && !query.include
 * - !query.excludeDir?.length
 * - query.pattern.length < 5
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { buildSearchResult } from '../../src/tools/local_ripgrep/ripgrepResultBuilder.js';
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
    // 25 files * 5 matches = 125 total matches > 100, files.length 25 > 20
    const files = makeFiles(25, 5);
    const query = {
      path: '/test',
      pattern: 'ab', // length < 5
      researchGoal: 'test',
      reasoning: 'test',
      // No type, no include, no excludeDir
    } as any;

    const result = await buildSearchResult(files, query, 'rg', []);

    expect(result.hints).toBeDefined();
    const hintsStr = result.hints!.join('\n');
    // New strict policy: one combined line that names every available lever.
    expect(hintsStr).toContain('Large result set');
    expect(hintsStr).toContain('add type or include');
    expect(hintsStr).toContain('add excludeDir');
    expect(hintsStr).toContain('lengthen pattern');
  });

  it('A1: itemsPerPage pages FILES (top-level), matchesPerFile caps matches/file', async () => {
    // 5 files, 4 matches each. itemsPerPage=2 → 2 files on page 1 (NOT 2 matches).
    // matchesPerFile=1 → each shown file carries at most 1 match.
    const files = makeFiles(5, 4);
    const query = {
      path: '/test',
      pattern: 'match',
      itemsPerPage: 2,
      matchesPerFile: 1,
      page: 1,
      researchGoal: 'test',
      reasoning: 'test',
    } as any;

    const result = await buildSearchResult(files, query, 'rg', []);

    // itemsPerPage governs the FILE page size now (the cross-tool top-level axis).
    expect(result.files).toHaveLength(2);
    expect(result.pagination?.filesPerPage).toBe(2);
    expect(result.pagination?.totalFiles).toBe(5);
    expect(result.pagination?.hasMore).toBe(true);
    // matchesPerFile caps the inner axis.
    expect(result.files[0]!.matches).toHaveLength(1);
    // Cursor hint uses the unified `page`.
    expect((result.hints ?? []).join('\n')).toContain('Next: page=2');
  });

  it('emits an out-of-range hint when page exceeds total pages (E2)', async () => {
    const files = makeFiles(3, 2); // 3 files → 1 page at filesPerPage=10
    const query = {
      path: '/test',
      pattern: 'match',
      page: 999,
      researchGoal: 'test',
      reasoning: 'test',
    } as any;

    const result = await buildSearchResult(files, query, 'rg', []);
    const hintsStr = (result.hints ?? []).join('\n');
    expect(hintsStr).toMatch(/outside available range|page 999 is/i);
  });

  it('suggests type/include when neither is set', async () => {
    const files = makeFiles(25, 5);
    const query = {
      path: '/test',
      pattern: 'x',
      researchGoal: 'test',
      reasoning: 'test',
    } as any;

    const result = await buildSearchResult(files, query, 'rg', []);

    expect(result.hints?.some(h => h.includes('add type or include'))).toBe(
      true
    );
  });

  it('suggests excludeDir when none is set', async () => {
    const files = makeFiles(25, 5);
    const query = {
      path: '/test',
      pattern: 'ab',
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
      pattern: 'hi', // length 2 < 5
      researchGoal: 'test',
      reasoning: 'test',
    } as any;

    const result = await buildSearchResult(files, query, 'rg', []);

    expect(result.hints?.some(h => h.includes('lengthen pattern'))).toBe(true);
  });

  it('does NOT suggest type/include when query.type is already set', async () => {
    const files = makeFiles(25, 5);
    const query = {
      path: '/test',
      pattern: 'ab',
      type: 'ts',
      researchGoal: 'test',
      reasoning: 'test',
    } as any;

    const result = await buildSearchResult(files, query, 'rg', []);

    expect(result.hints?.some(h => h.includes('add type or include'))).toBe(
      false
    );
  });

  it('should NOT add excludeDir hint when query.excludeDir has items', async () => {
    const files = makeFiles(25, 5);
    const query = {
      path: '/test',
      pattern: 'ab',
      excludeDir: ['node_modules'],
      researchGoal: 'test',
      reasoning: 'test',
    } as any;

    const result = await buildSearchResult(files, query, 'rg', []);

    expect(result.hints?.some(h => h.includes('add excludeDir'))).toBe(false);
  });

  it('should NOT add pattern hint when query.pattern.length >= 5', async () => {
    const files = makeFiles(25, 5);
    const query = {
      path: '/test',
      pattern: 'longer', // length 6 >= 5
      researchGoal: 'test',
      reasoning: 'test',
    } as any;

    const result = await buildSearchResult(files, query, 'rg', []);

    expect(result.hints?.some(h => h.includes('lengthen pattern'))).toBe(false);
  });

  it('orders files by match count before path for relevance-first search results', async () => {
    const files = [
      ...makeFiles(1, 1).map(file => ({ ...file, path: '/test/a.ts' })),
      ...makeFiles(1, 3).map(file => ({ ...file, path: '/test/b.ts' })),
      ...makeFiles(1, 2).map(file => ({ ...file, path: '/test/c.ts' })),
    ];
    const query = {
      path: '/test',
      pattern: 'match',
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
