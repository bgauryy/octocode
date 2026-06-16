import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  buildSearchResult,
  finalizeRipgrepResult,
} from '../../../octocode-tools-core/src/tools/local_ripgrep/ripgrepResultBuilder.js';
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
  keywords: 'longpattern',
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

describe('buildSearchResult - exact localSearchCode output fields', () => {
  it('does not attach modified timestamps because localSearchCode does not expose that option', async () => {
    const files = [makeFile('/test/a.ts', 1), makeFile('/test/b.ts', 1)];
    const result = await buildSearchResult(files, baseQuery(), 'rg', []);
    expect(mockFsStat).not.toHaveBeenCalled();
    expect(result.files?.map(f => f.path)).toEqual([
      '/test/a.ts',
      '/test/b.ts',
    ]);
    expect(result.files?.[0]?.modified).toBeUndefined();
  });

  it('does not call fs.stat for regular results', async () => {
    const files = [makeFile('/test/a.ts', 1)];
    await buildSearchResult(files, baseQuery(), 'rg', []);
    expect(mockFsStat).not.toHaveBeenCalled();
  });

  it('emits concrete next-step hints for regular match results', async () => {
    const files = [makeFile('/test/a.ts', 1)];
    const result = await buildSearchResult(files, baseQuery(), 'rg', []);
    const hints = (result.hints ?? []).join('\n');
    expect(hints).toContain('localGetFileContent');
    expect(hints).toContain('lspGetSemantics');
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
      baseQuery({ countLinesPerFile: true }),
      'rg',
      [],
      { matchCount: 42, fileCount: 1 } as any
    );
    expect(result.files?.[0]?.matches).toBeUndefined();
    expect(result.files?.[0]?.matchCount).toBe(1);
    expect(result.files?.[0]?.pagination).toBeUndefined();
  });

  it('countMatchesPerFile mode: file-list mode summed fallback when stats absent', async () => {
    const files = [makeFile('/test/a.ts', 5, 5), makeFile('/test/b.ts', 3, 3)];
    const result = await buildSearchResult(
      files,
      baseQuery({ countMatchesPerFile: true }),
      'rg',
      []
    );
    expect(result.files?.[0]?.matches).toBeUndefined();
    expect(result.files?.[1]?.matches).toBeUndefined();
  });

  it('filesOnly mode: matches emptied, matchCount omitted (rg -l reports no counts)', async () => {
    const files = [makeFile('/test/a.ts', 4, 4)];
    const result = await buildSearchResult(
      files,
      baseQuery({ filesOnly: true }),
      'rg',
      []
    );
    expect(result.files?.[0]?.matches).toBeUndefined();
    expect(result.files?.[0]?.matchCount).toBeUndefined();
    expect(result.pagination?.totalMatches).toBeUndefined();
  });

  it('filesWithoutMatch mode: matchCount omitted (rg -L reports no counts)', async () => {
    const files = [makeFile('/test/a.ts', 1, 0)];
    const result = await buildSearchResult(
      files,
      baseQuery({ filesWithoutMatch: true }),
      'rg',
      []
    );
    expect(result.files?.[0]?.matches).toBeUndefined();
    expect(result.files?.[0]?.matchCount).toBeUndefined();
    expect(result.pagination?.totalMatches).toBeUndefined();
  });
});

describe('buildSearchResult - per-file match pagination (lines 106, 143)', () => {
  it('sets file.pagination and emits "more matches" hint when matches exceed matchesPerPage', async () => {
    const files = [makeFile('/test/a.ts', 12, 12)];
    const result = await buildSearchResult(files, baseQuery(), 'rg', []);
    const file = result.files?.[0];
    expect(file?.matches?.length).toBe(10);
    expect(file?.pagination).toBeDefined();
    expect(file?.pagination?.hasMore).toBe(true);
    const hints = (result.hints ?? []).join('\n');
    expect(hints).toContain('have more matches');
  });

  it('uses matchPage to continue per-file match pagination without losing matches', async () => {
    const files = [makeFile('/test/a.ts', 12, 12)];
    const result = await buildSearchResult(
      files,
      { ...baseQuery(), matchPage: 2 },
      'rg',
      []
    );
    const file = result.files?.[0];
    expect(file?.matches?.map(m => m.line)).toEqual([11, 12]);
    expect(file?.pagination).toMatchObject({
      currentPage: 2,
      totalPages: 2,
      hasMore: false,
    });
  });

  it('no per-file pagination when matches fit within matchesPerPage', async () => {
    const files = [makeFile('/test/a.ts', 3, 3)];
    const result = await buildSearchResult(files, baseQuery(), 'rg', []);
    expect(result.files?.[0]?.pagination).toBeUndefined();
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

describe('finalizeRipgrepResult - pass-through contract', () => {
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

  it(' — preserves full files[] and original hints', () => {
    const result = baseResult();
    const out = finalizeRipgrepResult(result, baseQuery({}), {
      totalMatches: 2,
      totalFiles: 1,
    });
    expect(out.files).toEqual(result.files);
    expect(out.hints).toEqual(result.hints);
  });

  it(' — preserves files[] when top file has no matches', () => {
    const result = baseResult({
      files: [{ path: '/test/a.ts', matchCount: 0, matches: [] }],
    });
    const out = finalizeRipgrepResult(result, baseQuery({}), {
      totalMatches: 0,
      totalFiles: 1,
    });
    expect(out.files).toEqual(result.files);
  });

  it(' — returns result unchanged when files is empty', () => {
    const result = baseResult({ files: [] });
    const out = finalizeRipgrepResult(result, baseQuery({}), {
      totalMatches: 0,
      totalFiles: 0,
    });
    expect(out.files).toEqual([]);
    expect(out.hints).toEqual(result.hints);
  });

  it(' — returns result unchanged when status is set', () => {
    const r = baseResult({ status: 'empty' });
    const out = finalizeRipgrepResult(r, baseQuery({}), {
      totalMatches: 0,
      totalFiles: 0,
    });
    expect(out).toBe(r);
    expect(out.files?.length).toBe(1);
  });

  it(' — all hints preserved', () => {
    const allHints = [
      'Large result set - narrow: add langType',
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
    const out = finalizeRipgrepResult(result, baseQuery({}), {
      totalMatches: 0,
      totalFiles: 0,
    });
    expect(out.hints).toEqual(allHints);
  });

  it('returns result untouched (pass-through)', () => {
    const result: any = { files: [], pagination: {}, hints: ['x'] };
    const out = finalizeRipgrepResult(result, baseQuery({}), {
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
    expect(result.pagination?.totalFiles).toBe(0);
    expect(result.pagination?.totalPages).toBe(0);
  });
});

describe('buildSearchResult - stable path tiebreak branches', () => {
  it('sorts tied match counts by path', async () => {
    mockFsStat.mockRejectedValue(new Error('nope'));
    const files = [makeFile('/test/b.ts', 1), makeFile('/test/a.ts', 1)];
    const result = await buildSearchResult(files, baseQuery(), 'rg', []);
    expect(result.files?.map(f => f.path)).toEqual([
      '/test/a.ts',
      '/test/b.ts',
    ]);
  });
});
