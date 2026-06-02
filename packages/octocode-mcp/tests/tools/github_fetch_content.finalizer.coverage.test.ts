/**
 * Branch-coverage tests for `src/tools/github_fetch_content/finalizer.ts`.
 *
 * Both exported functions are unit-testable in isolation with hand-built
 * `results`/`queries` arrays, so these tests drive the finalizer and the
 * verbosity shaper directly rather than through the MCP server. They target
 * the narrowing helpers (pagination / file / directory entries), runtime
 * hints, error hints, char-pagination, and the concise/compact verbosity paths.
 */
import { describe, it, expect } from 'vitest';
import {
  buildGithubFetchContentFinalizer,
  applyGithubFetchContentVerbosity,
} from '../../src/tools/github_fetch_content/finalizer.js';
import type { FlatQueryResult } from '../../src/types/toolResults.js';

type Query = Record<string, unknown>;

type Config = {
  toolName: string;
  responseCharOffset?: number;
  responseCharLength?: number;
};

// Build the finalizer once; it is stateless.
const finalize = buildGithubFetchContentFinalizer<Query>();

function run(
  queries: Query[],
  results: FlatQueryResult[],
  config: Partial<Config> = {}
) {
  return finalize({
    queries: queries as never,
    results,
    config: { toolName: 'githubGetFileContent', ...config } as never,
  });
}

describe('buildGithubFetchContentFinalizer — group building & narrowing', () => {
  it('reads a full file entry with pagination, partial flags and warnings', () => {
    const queries: Query[] = [
      { owner: 'o', repo: 'r', path: 'src/a.ts', verbosity: 'basic' },
    ];
    const results: FlatQueryResult[] = [
      {
        id: 'q1',
        data: {
          path: 'src/a.ts',
          content: 'hello world',
          totalLines: 12,
          resolvedBranch: 'main',
          pagination: {
            currentPage: 1,
            totalPages: 3,
            hasMore: true,
            charOffset: 0,
            charLength: 11,
            totalChars: 99,
            // non-finite / wrong-type optional fields are skipped (line 144)
            totalBytes: Number.NaN,
            filesPerPage: 'nope',
          },
          isPartial: true,
          startLine: 1,
          endLine: 11,
          lastModified: '2026-05-01',
          lastModifiedBy: 'alice',
          warnings: ['w1', 2, 'w2'], // mixed array -> filtered to strings
        },
      },
    ];

    const out = run(queries, results);
    const data = out.structuredContent as {
      results: Array<{ files?: Array<Record<string, unknown>> }>;
    };
    const file = data.results[0].files![0];
    expect(file.path).toBe('src/a.ts');
    expect(file.content).toBe('hello world');
    expect(file.isPartial).toBe(true);
    expect(file.totalLines).toBe(12);
    expect(file.warnings).toEqual(['w1', 'w2']);
    const pg = file.pagination as Record<string, unknown>;
    expect(pg.charOffset).toBe(0);
    // NaN / non-number optional fields were dropped.
    expect(pg.totalBytes).toBeUndefined();
    expect(pg.filesPerPage).toBeUndefined();
  });

  it('falls back to query.path and empty content when data fields are missing/invalid', () => {
    const queries: Query[] = [{ owner: 'o', repo: 'r', path: 'fallback.ts' }];
    const results: FlatQueryResult[] = [
      {
        id: 'q1',
        data: {
          // no path, non-string content, invalid pagination, empty warnings
          content: 123,
          pagination: { currentPage: 'x' }, // fails number check -> undefined
          warnings: [1, 2, 3], // all non-string -> readStringArray returns undefined
          isPartial: false,
        },
      },
    ];
    const out = run(queries, results);
    const data = out.structuredContent as {
      results: Array<{ files?: Array<Record<string, unknown>> }>;
    };
    const file = data.results[0].files![0];
    expect(file.path).toBe('fallback.ts');
    expect(file.content).toBe('');
    expect(file.pagination).toBeUndefined();
    expect(file.warnings).toBeUndefined();
    expect(file.isPartial).toBeUndefined();
  });

  it('reads a directory entry with files, cache flag and fileCount fallback', () => {
    const queries: Query[] = [
      { owner: 'o', repo: 'r', path: 'src', type: 'directory' },
    ];
    const results: FlatQueryResult[] = [
      {
        id: 'q1',
        data: {
          localPath: '/tmp/clone/src',
          totalSize: 4096,
          cached: true,
          resolvedBranch: 'dev',
          files: [
            { path: 'a.ts', size: 10, type: 'file' },
            'not-a-record', // filtered by isRecord
            {
              /* defaults */
            },
          ],
        },
      },
    ];
    const out = run(queries, results);
    const data = out.structuredContent as {
      results: Array<{ directories?: Array<Record<string, unknown>> }>;
    };
    const dir = data.results[0].directories![0];
    expect(dir.path).toBe('src');
    expect(dir.localPath).toBe('/tmp/clone/src');
    expect(dir.cached).toBe(true);
    // fileCount falls back to files.length (2 records survived).
    expect(dir.fileCount).toBe(2);
    const files = dir.files as Array<Record<string, unknown>>;
    expect(files).toHaveLength(2);
    expect(files[1]).toEqual({ path: '', size: 0, type: 'file' });
  });

  it('emits a cache hint for a cached directory', () => {
    const queries: Query[] = [
      { owner: 'o', repo: 'r', path: 'pkg', type: 'directory' },
    ];
    const results: FlatQueryResult[] = [
      {
        id: 'q1',
        data: { localPath: '/x', totalSize: 1, cached: true, files: [] },
      },
    ];
    const out = run(queries, results);
    const data = out.structuredContent as { hints?: string[] };
    expect(data.hints?.some(h => /served from cache/.test(h))).toBe(true);
  });

  it('reads a directory entry with all defaults (no files array, no cache, no path)', () => {
    // Hits the default sides: data.files not an array, query.path missing,
    // localPath/totalSize defaults, cached !== true, directory not cached.
    const queries: Query[] = [{ owner: 'o', repo: 'r', type: 'directory' }];
    const results: FlatQueryResult[] = [
      {
        id: 'q1',
        data: {
          // files is not an array -> rawFiles = []
          files: 'not-an-array',
          // no localPath, no totalSize, no fileCount -> all defaults
          // cached omitted -> not cached
        },
      },
    ];
    const out = run(queries, results);
    const data = out.structuredContent as {
      results: Array<{ directories?: Array<Record<string, unknown>> }>;
      hints?: string[];
    };
    const dir = data.results[0].directories![0];
    expect(dir.path).toBe('');
    expect(dir.localPath).toBe('');
    expect(dir.totalSize).toBe(0);
    expect(dir.fileCount).toBe(0);
    expect(dir.cached).toBeUndefined();
    expect(dir.files).toBeUndefined();
    // Not cached -> no cache hint emitted.
    expect(data.hints?.some(h => /served from cache/.test(h)) ?? false).toBe(
      false
    );
  });

  it('reads a file entry with neither data.path nor query.path (double fallback)', () => {
    // Hits `String(query.path ?? '')` -> '' side on line 173.
    const queries: Query[] = [{ owner: 'o', repo: 'r' }];
    const results: FlatQueryResult[] = [{ id: 'q1', data: { content: 'c' } }];
    const out = run(queries, results);
    const data = out.structuredContent as {
      results: Array<{ files?: Array<{ path: string }> }>;
    };
    expect(data.results[0].files![0].path).toBe('');
  });

  it('drops a result whose query slot is missing (results longer than queries)', () => {
    // queries[1] is undefined -> `if (!query) return` true side (line 218).
    const queries: Query[] = [{ owner: 'o', repo: 'r', path: 'a.ts' }];
    const results: FlatQueryResult[] = [
      { id: 'q1', data: { path: 'a.ts', content: 'x' } },
      { id: 'q2', data: { path: 'b.ts', content: 'y' } },
    ];
    const out = run(queries, results);
    const data = out.structuredContent as { results: Array<unknown> };
    expect(data.results).toHaveLength(1);
  });

  it('skips error results and queries with missing owner/repo', () => {
    const queries: Query[] = [
      { owner: 'o', repo: 'r', path: 'ok.ts' },
      { owner: '', repo: 'r', path: 'missing-owner.ts' }, // dropped
      { owner: 'o2', repo: 'r2', path: 'errored.ts' }, // error status
    ];
    const results: FlatQueryResult[] = [
      { id: 'a', data: { path: 'ok.ts', content: 'x' } },
      { id: 'b', data: { path: 'missing-owner.ts', content: 'y' } },
      {
        id: 'c',
        status: 'error',
        data: { error: 'boom' },
      },
    ];
    const out = run(queries, results);
    const data = out.structuredContent as {
      results: Array<{ owner: string }>;
    };
    // Only the first query produced a group.
    expect(data.results).toHaveLength(1);
    expect(data.results[0].owner).toBe('o');
  });
});

describe('buildGithubFetchContentFinalizer — runtime hints', () => {
  it('emits a charOffset continuation hint when a file paginates', () => {
    const queries: Query[] = [{ owner: 'o', repo: 'r', path: 'big.ts' }];
    const results: FlatQueryResult[] = [
      {
        id: 'q1',
        data: {
          path: 'big.ts',
          content: 'partial',
          pagination: {
            currentPage: 1,
            totalPages: 5,
            hasMore: true,
            charOffset: 100,
            charLength: 50,
          },
        },
      },
    ];
    const out = run(queries, results);
    const data = out.structuredContent as { hints?: string[] };
    expect(data.hints?.some(h => /charOffset=150/.test(h))).toBe(true);
  });
});

describe('buildGithubFetchContentFinalizer — error hints', () => {
  function errorResult(error: string, status?: number): FlatQueryResult {
    return { id: 'e1', status: 'error', data: { error, status } };
  }

  it('attaches 404 / not-found hints', () => {
    const out = run(
      [{ owner: 'o', repo: 'r', path: 'gone.ts' }],
      [errorResult('Not Found', 404)],
      {}
    );
    const data = out.structuredContent as {
      errors?: Array<{ hints?: string[]; owner?: string; path?: string }>;
    };
    expect(data.errors![0].owner).toBe('o');
    expect(data.errors![0].path).toBe('gone.ts');
    expect(
      data.errors![0].hints?.some(h => /githubViewRepoStructure/.test(h))
    ).toBe(true);
    // groups empty + errors present -> isError flagged
    expect(out.isError).toBe(true);
  });

  it('attaches 403 / forbidden hints', () => {
    const out = run(
      [{ owner: 'o', repo: 'r' }],
      [errorResult('403 Forbidden')],
      {}
    );
    const data = out.structuredContent as {
      errors?: Array<{ hints?: string[]; path?: string }>;
    };
    expect(data.errors![0].path).toBeUndefined();
    expect(data.errors![0].hints?.some(h => /token permissions/.test(h))).toBe(
      true
    );
  });

  it('attaches 429 / rate-limit hints', () => {
    const out = run(
      [{ owner: 'o', repo: 'r', path: 'x' }],
      [errorResult('API rate limit exceeded', 429)],
      {}
    );
    const data = out.structuredContent as {
      errors?: Array<{ hints?: string[] }>;
    };
    expect(data.errors![0].hints?.some(h => /Retry after reset/.test(h))).toBe(
      true
    );
  });

  it('returns no hints for an unrecognized error', () => {
    const out = run(
      [{ owner: 'o', repo: 'r', path: 'x' }],
      [errorResult('something weird', 500)],
      {}
    );
    const data = out.structuredContent as {
      errors?: Array<{ hints?: string[] }>;
    };
    expect(data.errors![0].hints).toBeUndefined();
  });
});

describe('buildGithubFetchContentFinalizer — char pagination & truncation', () => {
  it('paginates the bulk response and emits a responseCharOffset hint', () => {
    const big = 'X'.repeat(20_000);
    const queries: Query[] = [
      { owner: 'o', repo: 'r', path: 'a.ts' },
      { owner: 'o', repo: 'r', path: 'b.ts' },
    ];
    const results: FlatQueryResult[] = [
      { id: 'q1', data: { path: 'a.ts', content: big } },
      { id: 'q2', data: { path: 'b.ts', content: big } },
    ];
    const out = run(queries, results, {
      responseCharOffset: 0,
      responseCharLength: 5_000,
    });
    const data = out.structuredContent as {
      responsePagination?: { hasMore?: boolean };
      hints?: string[];
      warnings?: Array<{ kind: string }>;
    };
    expect(data.responsePagination).toBeDefined();
    if (data.responsePagination?.hasMore) {
      expect(data.hints?.some(h => /responseCharOffset=/.test(h))).toBe(true);
    }
  });

  it('windows an oversized single file by char pagination — no truncation warning', () => {
    const huge = 'Z'.repeat(60_000);
    const queries: Query[] = [{ owner: 'o', repo: 'r', path: 'huge.ts' }];
    const results: FlatQueryResult[] = [
      { id: 'q1', data: { path: 'huge.ts', content: huge } },
    ];
    const out = run(queries, results, {
      responseCharLength: 5_000,
    });
    const data = out.structuredContent as {
      warnings?: Array<{ kind: string; path?: string }>;
      results: Array<{ files?: Array<{ content: string }> }>;
      responsePagination?: { hasMore: boolean };
    };
    // No truncation warnings — the content is paginated, not clipped.
    expect(data.warnings).toBeUndefined();
    const content = data.results[0].files![0].content;
    expect(content.length).toBeLessThan(huge.length);
    expect(content).not.toMatch(/\[(truncated|clipped)\]/i);
    expect(data.responsePagination?.hasMore).toBe(true);
  });

  it('paginates a group of small files + a directory without truncating any item', () => {
    // Small files all fit the budget (truncator content-fits side, line 274),
    // the directory passes the truncator unchanged (no `content`, line 268),
    // getGroupItems concatenates directories (line 244), and setGroupItems
    // re-splits files/directories. responseCharLength omitted -> MAX_SAFE
    // default (line 388 `?? MAX`).
    const queries: Query[] = [
      { owner: 'o', repo: 'r', path: 'a.ts' },
      { owner: 'o', repo: 'r', path: 'b.ts' },
      { owner: 'o', repo: 'r', path: 'dir', type: 'directory' },
    ];
    const results: FlatQueryResult[] = [
      {
        id: 'q1',
        data: {
          path: 'a.ts',
          content: 'short',
          // pagination with hasMore but no charLength -> `?? 0` side (line 307)
          pagination: {
            currentPage: 1,
            totalPages: 2,
            hasMore: true,
            charOffset: 7,
          },
        },
      },
      { id: 'q2', data: { path: 'b.ts', content: 'tiny' } },
      {
        id: 'q3',
        data: { localPath: '/x', totalSize: 1, files: [], path: 'dir' },
      },
    ];
    const out = run(queries, results, {
      // offset set but length omitted -> charLength defaults to MAX_SAFE.
      responseCharOffset: 0,
    });
    const data = out.structuredContent as {
      results: Array<{
        files?: Array<{ content: string }>;
        directories?: Array<unknown>;
      }>;
      warnings?: Array<{ kind: string }>;
      hints?: string[];
    };
    // Nothing truncated.
    expect(
      data.warnings?.some(w => w.kind === 'content-truncated') ?? false
    ).toBe(false);
    expect(data.results[0].files).toHaveLength(2);
    expect(data.results[0].directories).toHaveLength(1);
    // charOffset continuation hint uses charLength default of 0 -> charOffset=7.
    expect(data.hints?.some(h => /charOffset=7\b/.test(h))).toBe(true);
  });
});

describe('applyGithubFetchContentVerbosity', () => {
  it('returns false and passes through when no queries opt in', () => {
    const responseData = {
      results: [
        {
          id: 'o/r',
          owner: 'o',
          repo: 'r',
          files: [{ path: 'a', content: 'x' }],
        },
      ],
      hints: ['keep me'],
    } as never;
    const applied = applyGithubFetchContentVerbosity(responseData, [
      { owner: 'o', repo: 'r', verbosity: 'basic' },
    ] as never);
    expect(applied).toBe(false);
  });

  it('returns false when queries array is empty (no allConcise)', () => {
    const responseData = { results: [] } as never;
    expect(applyGithubFetchContentVerbosity(responseData, [])).toBe(false);
  });

  it('minifies content, drops metadata and rewrites hints under concise', () => {
    const responseData = {
      results: [
        {
          id: 'o/r',
          owner: 'o',
          repo: 'r',
          files: [
            {
              path: 'a.ts',
              content: '// comment\nconst   x   =   1;\n',
              totalLines: 3,
              lastModified: '2026-01-01',
              lastModifiedBy: 'bob',
            },
            { path: '', content: 'no path -> raw' },
          ],
        },
      ],
      hints: ['old hint'],
    } as Record<string, unknown>;
    const applied = applyGithubFetchContentVerbosity(
      responseData as never,
      [{ owner: 'o', repo: 'r', verbosity: 'concise' }] as never
    );
    expect(applied).toBe(true);
    const results = responseData.results as Array<{
      files: Array<Record<string, unknown>>;
    }>;
    const file0 = results[0].files[0];
    expect(file0.lastModified).toBeUndefined();
    expect(file0.lastModifiedBy).toBeUndefined();
    const hints = responseData.hints as string[];
    expect(hints).toHaveLength(1);
    expect(hints[0]).toMatch(/files,.*lines,.*tokens \(minified\)/);
  });

  it('appends a verbosity-downgrade warning when fullContent=true under concise', () => {
    const responseData = {
      results: [
        {
          id: 'o/r',
          owner: 'o',
          repo: 'r',
          files: [{ path: 'a.ts', content: 'x' }],
        },
      ],
      warnings: [{ kind: 'pre-existing' }],
    } as Record<string, unknown>;
    const applied = applyGithubFetchContentVerbosity(
      responseData as never,
      [
        { owner: 'o', repo: 'r', verbosity: 'concise', fullContent: true },
      ] as never
    );
    expect(applied).toBe(true);
    const warnings = responseData.warnings as Array<{
      kind: string;
      field?: string;
    }>;
    expect(warnings.some(w => w.kind === 'verbosity-downgrade')).toBe(true);
    expect(warnings.some(w => w.field === 'fullContent')).toBe(true);
  });

  it('handles concise with missing results/files arrays gracefully', () => {
    const responseData = {} as Record<string, unknown>;
    const applied = applyGithubFetchContentVerbosity(
      responseData as never,
      [{ owner: 'o', repo: 'r', verbosity: 'concise' }] as never
    );
    expect(applied).toBe(true);
    const hints = responseData.hints as string[];
    expect(hints[0]).toMatch(/0 files, 0 lines/);
  });

  it('handles concise group without a files array and accumulates totalLines', () => {
    const responseData = {
      results: [
        // group with no `files` key -> `g.files ?? []` and reduce fallback
        { id: 'o/r', owner: 'o', repo: 'r', directories: [] },
        {
          id: 'o/r2',
          owner: 'o',
          repo: 'r2',
          files: [{ path: 'b.ts', content: 'const y=2;', totalLines: 5 }],
        },
      ],
    } as Record<string, unknown>;
    const applied = applyGithubFetchContentVerbosity(
      responseData as never,
      [{ owner: 'o', repo: 'r', verbosity: 'concise' }] as never
    );
    expect(applied).toBe(true);
    const hints = responseData.hints as string[];
    // 1 file, 5 lines accumulated from the second group only.
    expect(hints[0]).toMatch(/1 files, 5 lines/);
  });

  it('accumulates zero lines under concise when files lack totalLines', () => {
    // Forces `f.totalLines ?? 0` onto the `?? 0` side (line 484).
    const responseData = {
      results: [
        {
          id: 'o/r',
          owner: 'o',
          repo: 'r',
          files: [{ path: 'a.ts', content: 'const z = 3;' }],
        },
      ],
    } as Record<string, unknown>;
    const applied = applyGithubFetchContentVerbosity(
      responseData as never,
      [{ owner: 'o', repo: 'r', verbosity: 'concise' }] as never
    );
    expect(applied).toBe(true);
    const hints = responseData.hints as string[];
    expect(hints[0]).toMatch(/1 files, 0 lines/);
  });

  it('creates a fresh warnings array for fullContent downgrade when none exists', () => {
    const responseData = {
      results: [
        {
          id: 'o/r',
          owner: 'o',
          repo: 'r',
          files: [{ path: 'a.ts', content: 'x' }],
        },
      ],
      // no `warnings` key -> `responseData.warnings ?? []` false-undefined side
    } as Record<string, unknown>;
    const applied = applyGithubFetchContentVerbosity(
      responseData as never,
      [
        { owner: 'o', repo: 'r', verbosity: 'concise', fullContent: true },
      ] as never
    );
    expect(applied).toBe(true);
    const warnings = responseData.warnings as Array<{ kind: string }>;
    expect(warnings).toHaveLength(1);
    expect(warnings[0].kind).toBe('verbosity-downgrade');
  });

  it('trims advisory hints under compact (anyCompact)', () => {
    const responseData = {
      results: [
        {
          id: 'o/r',
          owner: 'o',
          repo: 'r',
          files: [{ path: 'a', content: 'x' }],
        },
      ],
      hints: [
        'file_too_large to display fully',
        'too large to display',
        'useful hint 1',
        'useful hint 2',
      ],
    } as Record<string, unknown>;
    const applied = applyGithubFetchContentVerbosity(
      responseData as never,
      [
        { owner: 'o', repo: 'r', verbosity: 'compact' },
        { owner: 'o', repo: 'r', verbosity: 'basic' },
      ] as never
    );
    expect(applied).toBe(false);
    const hints = responseData.hints as string[];
    // Advisory hints (file_too_large / too large) stripped under compact.
    expect(hints.some(h => /too large/.test(h))).toBe(false);
  });
});
