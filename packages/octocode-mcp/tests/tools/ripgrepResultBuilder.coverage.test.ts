import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  buildSearchResult,
  applyRipgrepVerbosity,
} from '../../src/tools/local_ripgrep/ripgrepResultBuilder.js';
import { promises as fs } from 'fs';

vi.mock('fs', () => ({
  promises: {
    stat: vi.fn(),
  },
}));

const mockFsStat = vi.mocked(fs.stat);

const makeFile = (path: string, matchCount: number, matches?: number): any => {
  const n = matches ?? matchCount;
  return {
    path,
    matchCount,
    matches: Array.from({ length: n }, (_, j) => ({
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
  };
};

const baseQuery = (extra: Record<string, unknown> = {}): any => ({
  path: '/test',
  pattern: 'longpattern',
  researchGoal: 'test',
  reasoning: 'test',
  ...extra,
});

beforeEach(() => {
  vi.clearAllMocks();
  mockFsStat.mockResolvedValue({
    mtime: new Date('2024-01-01T00:00:00.000Z'),
  } as any);
});

describe('buildSearchResult - showFileLastModified (lines 44, 116, 257-259)', () => {
  it('stats each file and attaches modified time when showFileLastModified is set', async () => {
    const files = [makeFile('/test/a.ts', 1), makeFile('/test/b.ts', 1)];
    const result = await buildSearchResult(
      files,
      baseQuery({ showFileLastModified: true, verbose: true }),
      'rg',
      []
    );
    expect(mockFsStat).toHaveBeenCalled();
    expect(result.files?.[0].modified).toBe('2024-01-01T00:00:00.000Z');
  });

  it('falls back to path tiebreak using modified time when match counts tie', async () => {
    mockFsStat
      .mockResolvedValueOnce({
        mtime: new Date('2020-01-01T00:00:00.000Z'),
      } as any)
      .mockResolvedValueOnce({
        mtime: new Date('2024-01-01T00:00:00.000Z'),
      } as any);
    const files = [makeFile('/test/old.ts', 1), makeFile('/test/new.ts', 1)];
    const result = await buildSearchResult(
      files,
      baseQuery({ showFileLastModified: true }),
      'rg',
      []
    );
    expect(result.files?.map(f => f.path)).toEqual([
      '/test/new.ts',
      '/test/old.ts',
    ]);
  });

  it('does not call fs.stat when showFileLastModified is absent', async () => {
    const files = [makeFile('/test/a.ts', 1)];
    await buildSearchResult(files, baseQuery(), 'rg', []);
    expect(mockFsStat).not.toHaveBeenCalled();
  });

  it('handles fs.stat rejection by leaving modified undefined', async () => {
    mockFsStat.mockRejectedValueOnce(new Error('nope'));
    const files = [makeFile('/test/a.ts', 1)];
    const result = await buildSearchResult(
      files,
      baseQuery({ showFileLastModified: true }),
      'rg',
      []
    );
    expect(result.files?.[0].modified).toBeUndefined();
  });
});

describe('buildSearchResult - maxFiles limiting (lines 57-58, 136)', () => {
  it('limits files and emits a "Results limited" hint', async () => {
    const files = [
      makeFile('/test/a.ts', 3),
      makeFile('/test/b.ts', 2),
      makeFile('/test/c.ts', 1),
    ];
    const result = await buildSearchResult(
      files,
      baseQuery({ maxFiles: 2 }),
      'rg',
      []
    );
    expect(result.files?.length).toBe(2);
    const hints = (result.hints ?? []).join('\n');
    expect(hints).toContain('Results limited to 2 files');
    expect(hints).toContain('found 3 matching');
  });

  it('does not limit when maxFiles >= file count', async () => {
    const files = [makeFile('/test/a.ts', 1), makeFile('/test/b.ts', 1)];
    const result = await buildSearchResult(
      files,
      baseQuery({ maxFiles: 10 }),
      'rg',
      []
    );
    expect(result.files?.length).toBe(2);
    expect((result.hints ?? []).join('\n')).not.toContain('Results limited');
  });
});

describe('buildSearchResult - file-list modes (lines 78-79, 97, 103, 106)', () => {
  it('count mode: empties matches, uses stats.matchCount, matchCount||1', async () => {
    const files = [makeFile('/test/a.ts', 0, 0)];
    const result = await buildSearchResult(
      files,
      baseQuery({ count: true }),
      'rg',
      [],
      { matchCount: 42, fileCount: 1 } as any
    );
    expect(result.files?.[0].matches).toEqual([]);
    expect(result.files?.[0].matchCount).toBe(1);
    expect(result.files?.[0].pagination).toBeUndefined();
  });

  it('countMatches mode: file-list mode summed fallback when stats absent', async () => {
    const files = [makeFile('/test/a.ts', 5, 5), makeFile('/test/b.ts', 3, 3)];
    const result = await buildSearchResult(
      files,
      baseQuery({ countMatches: true }),
      'rg',
      []
    );
    expect(result.files?.[0].matches).toEqual([]);
    expect(result.files?.[1].matches).toEqual([]);
  });

  it('filesOnly mode: matches emptied, sums individual matchCounts', async () => {
    const files = [makeFile('/test/a.ts', 4, 4)];
    const result = await buildSearchResult(
      files,
      baseQuery({ filesOnly: true }),
      'rg',
      []
    );
    expect(result.files?.[0].matches).toEqual([]);
    expect(result.files?.[0].matchCount).toBe(4);
  });
});

describe('buildSearchResult - per-file match pagination (lines 106, 143)', () => {
  it('sets file.pagination and emits "more matches" hint when matches exceed matchesPerPage', async () => {
    const files = [makeFile('/test/a.ts', 12, 12)];
    const result = await buildSearchResult(files, baseQuery(), 'rg', []);
    const file = result.files?.[0];
    expect(file?.matches.length).toBe(10);
    expect(file?.pagination).toBeDefined();
    expect(file?.pagination?.hasMore).toBe(true);
    const hints = (result.hints ?? []).join('\n');
    expect(hints).toContain('have more matches');
  });

  it('no per-file pagination when matches fit within matchesPerPage', async () => {
    const files = [makeFile('/test/a.ts', 3, 3)];
    const result = await buildSearchResult(files, baseQuery(), 'rg', []);
    expect(result.files?.[0].pagination).toBeUndefined();
    expect((result.hints ?? []).join('\n')).not.toContain('have more matches');
  });
});

describe('buildSearchResult - warnings passthrough', () => {
  it('includes warnings when provided', async () => {
    const files = [makeFile('/test/a.ts', 1)];
    const result = await buildSearchResult(files, baseQuery(), 'rg', [
      'a warning',
    ]);
    expect(result.warnings).toEqual(['a warning']);
  });
});

describe('applyRipgrepVerbosity - pass-through contract', () => {
  const baseResult = (overrides: Record<string, unknown> = {}): any => ({
    files: [
      {
        path: '/test/a.ts',
        matchCount: 2,
        matches: [{ line: 7, column: 1, value: 'm' }],
      },
    ],
    pagination: {
      currentPage: 1,
      totalPages: 1,
      filesPerPage: 10,
      totalFiles: 1,
      hasMore: false,
    },
    hints: ['some hint'],
    ...overrides,
  });

  it('verbose:false — preserves full files[] and original hints', () => {
    const result = baseResult();
    const out = applyRipgrepVerbosity(result, baseQuery({ verbose: false }), {
      totalMatches: 2,
      totalFiles: 1,
    });
    expect(out.files).toEqual(result.files);
    expect(out.hints).toEqual(result.hints);
  });

  it('verbose:false — preserves files[] when top file has no matches', () => {
    const result = baseResult({
      files: [{ path: '/test/a.ts', matchCount: 0, matches: [] }],
    });
    const out = applyRipgrepVerbosity(result, baseQuery({ verbose: false }), {
      totalMatches: 0,
      totalFiles: 1,
    });
    expect(out.files).toEqual(result.files);
  });

  it('verbose:false — returns result unchanged when files is empty', () => {
    const result = baseResult({ files: [] });
    const out = applyRipgrepVerbosity(result, baseQuery({ verbose: false }), {
      totalMatches: 0,
      totalFiles: 0,
    });
    expect(out.files).toEqual([]);
    expect(out.hints).toEqual(result.hints);
  });

  it('verbose:false — returns result unchanged when status is set', () => {
    const r = baseResult({ status: 'empty' });
    const out = applyRipgrepVerbosity(r, baseQuery({ verbose: false }), {
      totalMatches: 0,
      totalFiles: 0,
    });
    expect(out).toBe(r);
    expect(out.files?.length).toBe(1);
  });

  it('verbose:true — all hints preserved', () => {
    const allHints = [
      'Large result set - narrow: add type',
      'keep me 1',
      'keep me 2',
      'payload is large advisory',
    ];
    const result: any = {
      files: [],
      pagination: {
        currentPage: 1,
        totalPages: 1,
        filesPerPage: 10,
        totalFiles: 0,
        hasMore: false,
      },
      hints: [...allHints],
    };
    const out = applyRipgrepVerbosity(result, baseQuery({ verbose: true }), {
      totalMatches: 0,
      totalFiles: 0,
    });
    expect(out.hints).toEqual(allHints);
  });

  it('omitted verbose — returns result untouched', () => {
    const result: any = { files: [], pagination: {}, hints: ['x'] };
    const out = applyRipgrepVerbosity(result, baseQuery({}), {
      totalMatches: 0,
      totalFiles: 0,
    });
    expect(out).toBe(result);
  });
});

describe('buildSearchResult - empty results', () => {
  it('handles zero files without throwing', async () => {
    const result = await buildSearchResult([], baseQuery(), 'rg', []);
    expect(result.files).toEqual([]);
    expect(result.pagination.totalFiles).toBe(0);
    expect(result.pagination.totalPages).toBe(0);
  });
});

describe('buildSearchResult - compareModifiedDescending branches (266-274)', () => {
  it('both modified missing -> stable path order (line 266)', async () => {
    mockFsStat.mockRejectedValue(new Error('nope'));
    const files = [makeFile('/test/b.ts', 1), makeFile('/test/a.ts', 1)];
    const result = await buildSearchResult(
      files,
      baseQuery({ showFileLastModified: true }),
      'rg',
      []
    );
    expect(result.files?.map(f => f.path)).toEqual([
      '/test/a.ts',
      '/test/b.ts',
    ]);
  });

  it('one modified present sorts before one missing (lines 267-268)', async () => {
    mockFsStat
      .mockResolvedValueOnce({
        mtime: new Date('2024-01-01T00:00:00.000Z'),
      } as any)
      .mockRejectedValueOnce(new Error('nope'));
    const files = [makeFile('/test/has.ts', 1), makeFile('/test/none.ts', 1)];
    const result = await buildSearchResult(
      files,
      baseQuery({ showFileLastModified: true }),
      'rg',
      []
    );
    expect(result.files?.[0].path).toBe('/test/has.ts');
  });

  it('valid dates sort newest first (line 275)', async () => {
    mockFsStat
      .mockResolvedValueOnce({
        mtime: new Date('2020-06-01T00:00:00.000Z'),
      } as any)
      .mockResolvedValueOnce({
        mtime: new Date('2025-06-01T00:00:00.000Z'),
      } as any);
    const files = [
      makeFile('/test/older.ts', 1),
      makeFile('/test/newer.ts', 1),
    ];
    const result = await buildSearchResult(
      files,
      baseQuery({ showFileLastModified: true }),
      'rg',
      []
    );
    expect(result.files?.map(f => f.path)).toEqual([
      '/test/newer.ts',
      '/test/older.ts',
    ]);
  });
});
