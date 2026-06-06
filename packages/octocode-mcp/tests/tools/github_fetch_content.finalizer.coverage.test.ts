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
      { owner: 'o', repo: 'r', path: 'src/a.ts', verbose: false },
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
            totalBytes: Number.NaN,
            filesPerPage: 'nope',
          },
          isPartial: true,
          startLine: 1,
          endLine: 11,
          lastModified: '2026-05-01',
          lastModifiedBy: 'alice',
          warnings: ['w1', 2, 'w2'],
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
    expect(pg.totalBytes).toBeUndefined();
    expect(pg.filesPerPage).toBeUndefined();
  });

  it('falls back to query.path and empty content when data fields are missing/invalid', () => {
    const queries: Query[] = [{ owner: 'o', repo: 'r', path: 'fallback.ts' }];
    const results: FlatQueryResult[] = [
      {
        id: 'q1',
        data: {
          content: 123,
          pagination: { currentPage: 'x' },
          warnings: [1, 2, 3],
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
            'not-a-record',
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
    const queries: Query[] = [{ owner: 'o', repo: 'r', type: 'directory' }];
    const results: FlatQueryResult[] = [
      {
        id: 'q1',
        data: {
          files: 'not-an-array',
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
    expect(data.hints?.some(h => /served from cache/.test(h)) ?? false).toBe(
      false
    );
  });

  it('reads a file entry with neither data.path nor query.path (double fallback)', () => {
    const queries: Query[] = [{ owner: 'o', repo: 'r' }];
    const results: FlatQueryResult[] = [{ id: 'q1', data: { content: 'c' } }];
    const out = run(queries, results);
    const data = out.structuredContent as {
      results: Array<{ files?: Array<{ path: string }> }>;
    };
    expect(data.results[0].files![0].path).toBe('');
  });

  it('drops a result whose query slot is missing (results longer than queries)', () => {
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
      { owner: '', repo: 'r', path: 'missing-owner.ts' },
      { owner: 'o2', repo: 'r2', path: 'errored.ts' },
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
    expect(data.results).toHaveLength(1);
    expect(data.results[0].owner).toBe('o');
  });
});

describe('buildGithubFetchContentFinalizer — partial file continuation hints', () => {
  it('partial file emits startLine continuation in hints[], not evidence.reason', () => {
    const queries: Query[] = [{ owner: 'o', repo: 'r', path: 'large.ts' }];
    const results: FlatQueryResult[] = [
      {
        id: 'q1',
        data: {
          path: 'large.ts',
          content: 'hello',
          totalLines: 200,
          isPartial: true,
          startLine: 1,
          endLine: 50,
        },
      },
    ];

    const out = run(queries, results, { peerEvidence: true } as never);
    const data = out.structuredContent as {
      hints?: string[];
      evidence?: { reason?: string; incompleteReasons?: string[] };
    };

    expect(data.hints?.some(h => /startLine=51/.test(h))).toBe(true);

    const reasonStr = Array.isArray(data.evidence?.incompleteReasons)
      ? data.evidence.incompleteReasons.join(' ')
      : (data.evidence?.reason ?? '');
    expect(reasonStr).not.toMatch(/startLine=51/);
  });

  it('evidence.reason describes the partial state without navigation details', () => {
    const queries: Query[] = [{ owner: 'o', repo: 'r', path: 'large.ts' }];
    const results: FlatQueryResult[] = [
      {
        id: 'q1',
        data: {
          path: 'large.ts',
          content: 'hello',
          totalLines: 200,
          isPartial: true,
          startLine: 1,
          endLine: 50,
        },
      },
    ];

    const out = run(queries, results, { peerEvidence: true } as never);
    const data = out.structuredContent as {
      evidence?: { reason?: string };
    };

    const reason = data.evidence?.reason ?? '';
    expect(reason.length).toBeGreaterThan(0);
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
  it('handles multiple queries and returns results for all', () => {
    const big = 'X'.repeat(20_000);
    const queries: Query[] = [
      { owner: 'o', repo: 'r', path: 'a.ts' },
      { owner: 'o', repo: 'r', path: 'b.ts' },
    ];
    const results: FlatQueryResult[] = [
      { id: 'q1', data: { path: 'a.ts', content: big } },
      { id: 'q2', data: { path: 'b.ts', content: big } },
    ];
    const out = run(queries, results, {});
    const data = out.structuredContent as {
      results: Array<{ files?: Array<{ content: string }> }>;
    };
    expect(data.results.length).toBeGreaterThan(0);
  });

  it('returns large file content without truncation warning', () => {
    const huge = 'Z'.repeat(60_000);
    const queries: Query[] = [{ owner: 'o', repo: 'r', path: 'huge.ts' }];
    const results: FlatQueryResult[] = [
      { id: 'q1', data: { path: 'huge.ts', content: huge } },
    ];
    const out = run(queries, results, {});
    const data = out.structuredContent as {
      warnings?: Array<{ kind: string; path?: string }>;
      results: Array<{ files?: Array<{ content: string }> }>;
    };
    expect(
      data.warnings?.some(w => w.kind === 'content-truncated') ?? false
    ).toBe(false);
    expect(data.results[0].files![0].content).not.toMatch(
      /\[(truncated|clipped)\]/i
    );
  });

  it('paginates a group of small files + a directory without truncating any item', () => {
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
    expect(
      data.warnings?.some(w => w.kind === 'content-truncated') ?? false
    ).toBe(false);
    expect(data.results[0].files).toHaveLength(2);
    expect(data.results[0].directories).toHaveLength(1);
    expect(data.hints?.some(h => /charOffset=7\b/.test(h))).toBe(true);
  });
});

describe('applyGithubFetchContentVerbosity', () => {
  it('strips lastModified/lastModifiedBy metadata when no queries are verbose (default)', () => {
    const responseData = {
      results: [
        {
          id: 'o/r',
          owner: 'o',
          repo: 'r',
          files: [
            {
              path: 'a',
              content: 'x',
              lastModified: '2026',
              lastModifiedBy: 'alice',
            },
          ],
        },
      ],
      hints: ['keep me'],
    } as never;
    applyGithubFetchContentVerbosity(responseData, [] as never);
    const file = (responseData as any).results[0].files[0];
    expect(file).not.toHaveProperty('lastModified');
  });

  it('preserves all metadata when at least one query has verbose=true', () => {
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
          ],
        },
      ],
      hints: ['old hint'],
    } as Record<string, unknown>;
    applyGithubFetchContentVerbosity(
      responseData as never,
      [{ owner: 'o', repo: 'r', verbose: true }] as never
    );
    const results = responseData.results as Array<{
      files: Array<Record<string, unknown>>;
    }>;
    const file0 = results[0].files[0];
    expect(file0.lastModified).toBe('2026-01-01');
    expect(file0.lastModifiedBy).toBe('bob');
  });

  it('passes through results with no files array without error', () => {
    const responseData = {
      results: [{ id: 'o/r', owner: 'o', repo: 'r', directories: [] }],
    } as Record<string, unknown>;
    expect(() =>
      applyGithubFetchContentVerbosity(
        responseData as never,
        [{ owner: 'o', repo: 'r' }] as never
      )
    ).not.toThrow();
    expect(responseData.hints).toBeUndefined();
  });

  it('passes through missing results without error', () => {
    const responseData = {} as Record<string, unknown>;
    expect(() =>
      applyGithubFetchContentVerbosity(
        responseData as never,
        [{ owner: 'o', repo: 'r' }] as never
      )
    ).not.toThrow();
  });

  it('no warnings injected by verbosity layer', () => {
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
    applyGithubFetchContentVerbosity(
      responseData as never,
      [{ owner: 'o', repo: 'r' }] as never
    );
    const warnings = responseData.warnings as Array<{ kind: string }>;
    expect(warnings.some(w => w.kind === 'pre-existing')).toBe(true);
  });

  it('hints are not modified by verbosity layer', () => {
    const allHints = [
      'file_too_large to display fully',
      'too large to display',
      'useful hint 1',
      'useful hint 2',
    ];
    const responseData = {
      results: [
        {
          id: 'o/r',
          owner: 'o',
          repo: 'r',
          files: [{ path: 'a', content: 'x' }],
        },
      ],
      hints: [...allHints],
    } as Record<string, unknown>;
    applyGithubFetchContentVerbosity(
      responseData as never,
      [{ owner: 'o', repo: 'r' }] as never
    );
    const hints = responseData.hints as string[];
    expect(hints).toEqual(allHints);
    expect(hints.some(h => /too large/.test(h))).toBe(true);
  });
});
